use crate::node::data::DataPayload;
use crate::node::{DrawInteraction, NodeVariant};
use crate::prelude::*;
use crate::registry::RegistryItemInner;
use crate::table::draw_record_batch;
use crate::{
    canvas::NodeIdx,
    impl_NodeDynamics,
    node::{DrawContext, NodeDynamics},
};

use eframe::egui::{Sense, StrokeKind, TextStyle, UiBuilder};

pub struct ViewNode {
    size: Vec2,
}

impl Default for ViewNode {
    fn default() -> Self {
        Self {
            size: Vec2 { x: 500.0, y: 300.0 },
        }
    }
}

impl ViewNode {
    fn show<Fn1, Fn2>(&mut self, ctx: &mut DrawContext<'_>, add_header: Fn1, add_main: Fn2)
    where
        Fn1: FnOnce(&mut Ui),
        Fn2: FnOnce(&mut Ui, f32),
    {
        let padding = 20.0;
        let bounding_rect = Rect::from_center_size(ctx.screen_location, self.size);
        ctx.painter.rect(
            bounding_rect.expand(padding),
            ctx.theme.corner_radius,
            ctx.theme.faint_background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        // Interaction layer for title bar
        let mut ui = ctx.ui.new_child(UiBuilder::new().max_rect(bounding_rect));
        let top_bar_height = ui.text_style_height(&TextStyle::Heading);
        let top_rect = Rect::from_min_size(
            bounding_rect.left_top(),
            Vec2::new(bounding_rect.width(), top_bar_height),
        );
        let top_resp = ui.interact(top_rect, ui.id().with("header_bar"), Sense::all());

        ui.horizontal(|ui| {
            add_header(ui);
        });
        ui.separator();

        let h = ui.available_height();
        add_main(&mut ui, h);

        if top_resp.dragged() {
            *ctx.node_location() += top_resp.drag_delta();
        }
    }
}

pub enum ViewPayload {
    DataTable(DataTableView),
}

impl_NodeDynamics!(for ViewPayload where variants = { DataTable });

pub struct DataTableView {
    pub data_ref: NodeIdx,
    pub view: ViewNode,
}

impl NodeDynamics for DataTableView {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) -> DrawInteraction {
        let node = unsafe { ctx.canvas.as_mut() }
            .graph
            .node_weight(self.data_ref)
            .unwrap();
        let data_node = if let NodeVariant::Data(DataPayload::Dataframe(node)) = &node.variant {
            node
        } else {
            unreachable!("Data table view for non-df node")
        };

        let item = ctx.registry.get(data_node.data_idx).unwrap();
        let df = if let RegistryItemInner::Dataframe(batch) = &item.inner {
            batch
        } else {
            unreachable!("Data table view for non-df registry item")
        };
        self.view.show(
            ctx,
            |ui| {
                ui.heading(&data_node.name);
            },
            |ui, height| {
                draw_record_batch(ui, df, height);
            },
        );

        DrawInteraction::None
    }

    #[inline(always)]
    fn size(&self, _ctx: &mut DrawContext<'_>) -> Vec2 {
        self.view.size
    }
}
