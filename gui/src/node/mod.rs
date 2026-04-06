use crate::prelude::*;
use crate::{
    canvas::{CanvasCommand, DisjointGraphRef, ViewState},
    node::{
        dataframe::DataframePayload,
        primitives::{NumericPayload, TextPayload},
        transform::{TransformArgPayload, TransformPayload},
    },
    registry::Registry,
    theme::Theme,
};

use eframe::egui::{Id, Sense, Stroke, StrokeKind};
use enum_dispatch::enum_dispatch;

pub mod dataframe;
pub mod primitives;
pub mod transform;
pub mod view;

#[derive(Serialize, Deserialize)]
pub struct Node {
    pub location: Pos2,
    pub variant: NodeVariant,
}

#[enum_dispatch]
pub trait NodeDynamics {
    fn draw(&mut self, ctx: &mut DrawContext<'_>);

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect;

    /// If interacted, returns `Some((interacted_index, clicked))`
    fn edge_target(&self, ctx: &mut DrawContext<'_>) -> Option<(NodeIdx, bool)> {
        let bounding_rect = self.rect(ctx);
        let interaction = ctx.ui.interact(
            bounding_rect,
            ctx.id.with("edge_target"),
            Sense::HOVER | Sense::CLICK,
        );

        if interaction.clicked() {
            Some((ctx.index, true))
        } else if interaction.hovered() {
            ctx.ui.painter().rect(
                bounding_rect,
                ctx.theme.corner_radius,
                ctx.theme.faint_background.gamma_multiply(0.3),
                Stroke::NONE,
                StrokeKind::Middle,
            );
            Some((ctx.index, false))
        } else {
            None
        }
    }

    fn override_edge_color(&self) -> Option<Color32> {
        None
    }
}

impl Node {
    pub fn nearest_boundary_point(origin: Rect, dest: Rect) -> (Pos2, Pos2) {
        let dir = dest.center() - origin.center();

        let origin_half = origin.size() / 2.0;
        let x_ratio_o = (origin_half.x / dir.x.abs()).abs();
        let y_ratio_o = (origin_half.y / dir.y.abs()).abs();
        let scale_o = x_ratio_o.min(y_ratio_o);
        let pos_o = origin.center() + dir * scale_o;

        let dest_half = dest.size() / 2.0;
        let x_ratio_d = (dest_half.x / dir.x.abs()).abs();
        let y_ratio_d = (dest_half.y / dir.y.abs()).abs();
        let scale_d = x_ratio_d.min(y_ratio_d);
        let pos_d = dest.center() - dir * scale_d;

        (pos_o, pos_d)
    }
}

#[enum_dispatch(NodeDynamics)]
#[derive(Serialize, Deserialize)]
pub enum NodeVariant {
    Dataframe(DataframePayload),
    Transform(TransformPayload),
    TransformArg(TransformArgPayload),
    Text(TextPayload),
    Integer(NumericPayload<i32>),
    Float(NumericPayload<f64>),
}

pub struct DrawContext<'ctx> {
    pub index: NodeIdx,
    pub id: Id,
    pub screen_location: Pos2,
    pub command_queue: &'ctx mut Vec<CanvasCommand>,
    pub view_state: &'ctx ViewState,
    pub registry: &'ctx Registry,
    pub graph_ref: DisjointGraphRef,
    pub ui: &'ctx mut Ui,
    pub theme: &'ctx Theme,
}
