use eframe::egui::Id;

use crate::prelude::*;
use crate::{
    canvas::{CanvasCommand, NodeIdx, ViewState},
    impl_NodeDynamics,
    node::{data::TransformPayload, dataframe::DataframeView},
    registry::Registry,
    theme::Theme,
};

pub mod data;
pub mod dataframe;
mod macros;
pub mod view;

pub struct Node {
    pub location: Pos2,
    pub variant: NodeVariant,
}

pub trait NodeDynamics {
    fn draw(&mut self, ctx: &mut DrawContext<'_>);
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect;
    fn nearest_boundary_point(&self, dir: Vec2, ctx: &mut DrawContext<'_>) -> Pos2 {
        let bounds = self.rect(ctx);
        let half_size = bounds.size() / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        bounds.center() + dir * scale
    }
}

pub enum NodeVariant {
    Transform(TransformPayload),
    Dataframe(DataframeView),
}
impl_NodeDynamics!(for NodeVariant where variants = { Transform, Dataframe });

#[macro_export]
macro_rules! impl_NodeDynamics {
    (for $type_name:ty where variants = { $($variant:ident),+ }) => {
        impl NodeDynamics for $type_name {
            fn draw(&mut self, ctx: &mut DrawContext<'_>) {
                match self { $(Self::$variant(inner) => inner.draw(ctx),)* }
            }

            fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
                match self { $(Self::$variant(inner) => inner.rect(ctx),)* }
            }
        }
    };
}

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
