pub use crate::{canvas::NodeIdx, cursor_icon, theme::LIGHT_THEME};

pub use arrow::array::RecordBatch;
pub use eframe::{
    egui,
    egui::{Color32, Painter, Pos2, Rect, Ui, Vec2},
};

#[macro_export]
macro_rules! cursor_icon {
    ($ui:expr, $icon:ident) => {
        $ui.ctx().set_cursor_icon(::eframe::egui::CursorIcon::$icon)
    };
}

#[derive(Default, Clone, Copy)]
pub enum DrawInteraction {
    #[default]
    None,

    Hovered,
    Dragged(Vec2),
    Clicked,
}

impl From<eframe::egui::Response> for DrawInteraction {
    fn from(resp: eframe::egui::Response) -> Self {
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
