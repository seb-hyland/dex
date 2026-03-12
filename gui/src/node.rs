use crate::theme::Theme;

use std::{path::PathBuf, sync::Arc};

use arrow::array::RecordBatch;
use eframe::egui::{
    Align, FontId, Galley, Painter, Pos2, Rect, StrokeKind, Ui, Vec2, text::LayoutJob,
};

pub struct Node {
    pub location: Pos2,
    pub payload: NodePayload,
}

pub enum NodePayload {
    Dataframe {
        name: String,
        df: RecordBatch,
        path: Option<PathBuf>,
    },
    Transform {
        name: String,
        code: String,
    },
}

impl NodePayload {
    pub const PADDING: Vec2 = Vec2::splat(20.0);

    pub fn name(&'_ self) -> &'_ str {
        match self {
            NodePayload::Dataframe { name, .. } => name,
            NodePayload::Transform { name, .. } => name,
        }
    }

    pub fn get_galley(&self, ui: &Ui, scale: f32) -> Arc<Galley> {
        let name = self.name();
        let theme: Theme = ui.ctx().into();
        ui.fonts_mut(|f| {
            let mut layout = LayoutJob::simple(
                name.to_owned(),
                FontId::proportional(28.0 * scale),
                theme.text,
                350.0 * scale,
            );
            layout.halign = Align::Center;

            f.layout_job(layout)
        })
    }

    pub fn draw_node(&self, ui: &mut Ui, painter: &Painter, rect: Rect, scale: f32) {
        let theme: Theme = ui.ctx().into();
        let galley = self.get_galley(ui, scale);

        painter.rect(
            rect,
            2.0,
            theme.faint_background,
            theme.border,
            StrokeKind::Middle,
        );
        let text_pos = rect.center()
            - Vec2 {
                x: 0.0,
                y: galley.size().y / 2.0,
            };
        painter.galley(text_pos, galley, theme.text);
    }

    pub fn draw_node_dragged(&self, ui: &mut Ui, painter: &Painter, rect: Rect, scale: f32) {
        let theme: Theme = ui.ctx().into();
        painter.add(theme.shadow.as_shape(rect, 2.0));

        self.draw_node(ui, painter, rect, scale);
    }

    fn nearest_boundary_point(&self, dir: Vec2, ui: &Ui, global_scale: f32) -> Pos2 {
        let size = self.get_galley(ui, global_scale).size() + Self::PADDING;
        let half_size = size / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        (dir * scale).to_pos2()
    }
}
