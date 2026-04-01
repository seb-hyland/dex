use eframe::egui::Id;

use crate::node::data::TransformPayload;
use crate::node::view::DataframeView;
use crate::prelude::*;
use crate::{
    canvas::{NodeIdx, ViewState},
    impl_NodeDynamics,
    node::command::CanvasCommand,
    registry::Registry,
    theme::Theme,
};

pub mod command;
pub mod data;
mod macros;
pub mod view;

pub struct Node {
    pub location: Pos2,
    pub variant: NodeVariant,
}

pub trait NodeDynamics {
    fn draw(&mut self, ctx: &mut DrawContext<'_>);
    fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2;
    fn nearest_boundary_point(&self, dir: Vec2, ctx: &mut DrawContext<'_>) -> Pos2 {
        let size = self.size(ctx);
        let half_size = size / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        let location = ctx.screen_location;
        location + dir * scale
    }
}

pub enum NodeVariant {
    Transform(TransformPayload),
    Dataframe(DataframeView),
}

impl_NodeDynamics!(for NodeVariant where variants = { Transform, Dataframe });

pub struct DrawContext<'ctx> {
    pub index: NodeIdx,
    pub id: Id,
    pub screen_location: Pos2,
    pub command_queue: &'ctx mut Vec<CanvasCommand>,
    pub view_state: &'ctx ViewState,
    pub registry: &'ctx mut Registry,
    pub ui: &'ctx mut Ui,
    pub theme: &'ctx Theme,
    pub noninteractive: bool,
}
