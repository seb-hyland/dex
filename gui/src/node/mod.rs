use crate::actions::DoActionContext;
use crate::canvas::CanvasGraph;
use crate::node::image::ImagePayload;
use crate::node::view::ResizeDir;
use crate::prelude::*;
use crate::{
    node::{
        dataframe::{DataframePayload, plot::DataframePlotPayload},
        primitives::{NumericPayload, TextPayload},
        transform::{TransformArgPayload, TransformPayload},
        typst::TypstPayload,
    },
    registry::Registry,
    theme::Theme,
};

use eframe::egui::{Id, Sense, Stroke, StrokeKind};
use enum_dispatch::enum_dispatch;
use strum::{EnumDiscriminants, EnumTryAs, VariantArray};

pub mod dataframe;
pub mod image;
pub mod primitives;
pub mod transform;
pub mod typst;
pub mod view;
// pub mod webview;

#[derive(Clone)]
pub struct Node {
    pub location: Pos2,
    pub id: Id,
    pub variant: NodeVariant,
}

#[derive(EnumTryAs, EnumDiscriminants, Clone)]
#[strum_discriminants(derive(VariantArray))]
#[enum_dispatch(NodeDynamics)]
pub enum NodeVariant {
    Dataframe(DataframePayload),
    DataframePlot(DataframePlotPayload),

    Text(TextPayload),
    Integer(NumericPayload<i32>),
    Float(NumericPayload<f64>),

    Typst(TypstPayload),
    // Webview(WebviewPayload),
    Image(ImagePayload),

    Transform(TransformPayload),
    TransformArg(TransformArgPayload),
}

pub struct DrawContext<'ctx> {
    pub index: NodeIdx,
    pub id: Id,
    pub screen_location: Pos2,
    pub action_queue: &'ctx mut Actions,
    pub layout: LayoutContext,
    pub registry: &'ctx Registry,
    pub graph: &'ctx CanvasGraph,
    pub ui: &'ctx mut Ui,
    pub theme: &'ctx Theme,
}

#[derive(Clone, Copy)]
pub struct LayoutContext {
    pub scale: f32,
}

pub trait NodeInitialization: Sized {
    type Origin: Default;
    fn init_from(f: Self::Origin, seed: u32) -> Self;

    fn init(seed: u32) -> Self {
        Self::init_from(Self::Origin::default(), seed)
    }
}

#[enum_dispatch]
pub trait NodeDynamics {
    fn step(&self, _ctx: &mut DrawContext<'_>);

    fn draw(&self, ctx: &mut DrawContext<'_>);

    fn size(&self, ctx: LayoutContext) -> Vec2;

    fn rect(&self, ctx: LayoutContext, pos: Pos2) -> Rect {
        Rect::from_min_size(pos, self.size(ctx))
    }

    fn resize(&mut self, _dir: ResizeDir, _delta: Vec2);

    /// If interacted, returns `Some((interacted_index, clicked))`
    fn edge_target(&self, ctx: &mut DrawContext<'_>) -> Option<(NodeIdx, bool)> {
        let bounding_rect = self.rect(ctx.layout, ctx.screen_location);

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

impl<'ctx> DoActionContext<'ctx> {
    pub fn unwrap_mut_with<Variant>(
        &mut self,
        idx: NodeIdx,
        f: impl Fn(&mut NodeVariant) -> Option<&mut Variant>,
    ) -> &mut Variant {
        f(&mut self.unwrap_active_canvas().get_node_mut(idx).variant).unwrap()
    }
}

action! {
    MoveNode { idx: NodeIdx, delta: Vec2 }
        does(ctx) {
            let canvas = ctx.unwrap_active_canvas();
            canvas.set_interacted(idx);
            canvas.get_node_mut(idx).location += delta;
        }
}
