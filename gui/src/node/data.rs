use eframe::egui::text::LayoutJob;
use eframe::egui::{Align, CursorIcon, FontId, Galley, Sense, StrokeKind};

use crate::node::view::{DataTableView, ViewNode, ViewPayload};
use crate::node::{DrawInteraction, Node, NodeDynamics, NodeVariant};
use crate::{impl_NodeDynamics, prelude::*};
use crate::{node::DrawContext, registry::RegistryHandle};

pub enum DataPayload {
    Dataframe(DataframePayload),
    Transform(TransformPayload),
}

impl_NodeDynamics!(for DataPayload where variants = { Dataframe, Transform });

pub struct DataframePayload {
    pub name: String,
    pub data_idx: RegistryHandle,
}

impl DataframePayload {
    const TEXT_PADDING: Vec2 = Vec2 { x: 20.0, y: 20.0 };

    fn galley(&self, ctx: &DrawContext<'_>) -> Arc<Galley> {
        ctx.painter.fonts_mut(|fonts| {
            let scale = unsafe { ctx.canvas.as_ref() }.view_state.scale();
            let mut layout = LayoutJob::simple(
                self.name.clone(),
                FontId::proportional(28.0 * scale),
                ctx.theme.text,
                350.0 * scale,
            );
            layout.halign = Align::Center;

            fonts.layout_job(layout)
        })
    }
}

impl NodeDynamics for DataframePayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) -> DrawInteraction {
        let galley = self.galley(ctx);

        let rect = Rect::from_center_size(ctx.screen_location, galley.size() + Self::TEXT_PADDING);

        let resp = ctx.ui.interact(rect, ctx.id, Sense::all());
        let interaction: DrawInteraction = resp.into();

        match interaction {
            DrawInteraction::Dragged(delta) => {
                ctx.painter.add(ctx.theme.shadow.as_shape(rect, 2.0));
                *ctx.node_location() += delta;
            }
            DrawInteraction::Hovered => {
                ctx.ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            DrawInteraction::Clicked => {
                let view_node = Node {
                    location: Pos2::ZERO,
                    variant: NodeVariant::View(ViewPayload::DataTable(DataTableView {
                        data_ref: ctx.index,
                        view: ViewNode::default(),
                    })),
                };
                unsafe { ctx.canvas.as_mut() }.add_node(view_node);
            }
            _ => {}
        }

        ctx.painter.rect(
            rect,
            2.0,
            ctx.theme.faint_background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        let text_pos = rect.center()
            - Vec2 {
                x: 0.0,
                y: galley.size().y / 2.0,
            };
        ctx.painter.galley(text_pos, galley, ctx.theme.text);

        interaction
    }

    fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2 {
        self.galley(ctx).size() + Self::TEXT_PADDING
    }
}

pub struct TransformPayload {
    name: Vec<String>,
    code: String,
}

impl NodeDynamics for TransformPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) -> DrawInteraction {
        todo!()
    }

    fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2 {
        todo!()
    }
}
