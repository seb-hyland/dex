use std::ptr::NonNull;

use eframe::egui::{Id, Response};

use crate::canvas::ViewState;
use crate::prelude::*;
use crate::{
    canvas::{Canvas, NodeIdx},
    impl_NodeDynamics,
    registry::Registry,
    theme::Theme,
};

pub mod data;
mod impl_macro;
pub mod view;

pub struct Node {
    pub location: Pos2,
    pub variant: NodeVariant,
}

pub trait NodeDynamics {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) -> DrawInteraction;
    fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2;
    fn nearest_boundary_point(&self, dir: Vec2, ctx: &mut DrawContext<'_>) -> Pos2 {
        let size = self.size(ctx);
        let half_size = size / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        let location = ctx.node_location();
        *location + dir * scale
    }
}

pub enum NodeVariant {
    Data(self::data::DataPayload),
    View(self::view::ViewPayload),
}

impl_NodeDynamics!(for NodeVariant where variants = { Data, View });

pub struct DrawContext<'ctx> {
    pub index: NodeIdx,
    pub id: Id,
    pub screen_location: Pos2,
    pub canvas: NonNull<Canvas>,
    pub registry: &'ctx mut Registry,
    pub ui: &'ctx mut Ui,
    pub painter: &'ctx Painter,
    pub theme: &'ctx Theme,
    pub placing: bool,
}

impl<'ctx> DrawContext<'ctx> {
    #[inline(always)]
    pub fn node_location(&mut self) -> &mut Pos2 {
        &mut unsafe { self.canvas.as_mut() }
            .graph
            .node_weight_mut(self.index)
            .unwrap()
            .location
    }

    #[inline(always)]
    pub fn view_state(&self) -> &ViewState {
        &unsafe { self.canvas.as_ref() }.view_state
    }
}

#[derive(Default, Clone, Copy)]
pub enum DrawInteraction {
    #[default]
    None,

    Hovered,
    Dragged(Vec2),
    Clicked,
}

impl From<Response> for DrawInteraction {
    fn from(resp: Response) -> Self {
        if resp.clicked() {
            DrawInteraction::Clicked
        } else if resp.dragged() {
            DrawInteraction::Dragged(resp.drag_delta())
        } else if resp.hovered() {
            DrawInteraction::Hovered
        } else {
            DrawInteraction::None
        }
    }
}
