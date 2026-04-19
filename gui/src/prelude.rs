pub(crate) use crate::{
    action,
    actions::{Action, Actions},
    canvas::NodeIdx,
    components::UiComponents,
    cursor_icon,
    text::Buffer,
    theme::LIGHT_THEME,
    types::{Rigid, Transient},
};

pub use rclite::Rc;

pub use arrow::array::RecordBatch;
pub use egui::{self, Color32, Painter, Pos2, Rect, Ui, Vec2};
pub use serde::{Deserialize, Serialize};

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
