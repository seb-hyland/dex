use egui::{Color32, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use workspace::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct Rect {
    pub corner_radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
}

#[typetag::serde]
impl Node for Rect {
    fn draw(&self, ctx: &mut DrawContext) -> DrawResult {
        let (Some(width), Some(height)) = (ctx.constraints.x, ctx.constraints.y) else {
            // Cannot draw; no constraints provided!
            return DrawResult::Complete {
                region: ScreenRegion::empty(),
            };
        };
        let size = egui::Vec2 {
            x: width.provided_value(),
            y: height.provided_value(),
        };
        let region = match ctx.constraints.pos {
            PositionConstraint::Center(c) => egui::Rect::from_center_size(c.into(), size),
            PositionConstraint::TopLeft(tl) => egui::Rect::from_min_size(tl.into(), size),
        };

        ctx.ui.painter().rect(
            region,
            self.corner_radius,
            self.fill_color,
            self.border,
            StrokeKind::Middle,
        );

        DrawResult::Complete {
            region: ScreenRegion::from(region),
        }
    }

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {}
}

impl Requestable for Rect {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
