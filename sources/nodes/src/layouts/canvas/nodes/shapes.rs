use dex_core::prelude::*;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::primitives::shapes;

#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
pub struct CanvasRect;

#[typetag::serde]
impl Node for CanvasRect {
    fn type_name(&self) -> String {
        "Canvas Rect".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let (Some(x), Some(y)) = (ctx.constraints.x, ctx.constraints.y) else {
            return DrawResult::Complete { region: None };
        };
        let size = Vector {
            x: x.provided_value(),
            y: y.provided_value(),
        };
        let origin = ctx.constraints.pos;
        let region = shapes::Rect {
            size,
            border: Stroke::NONE,
            corner_radius: 5.0,
            fill_color: Color32::RED,
        }
        .paint(ctx.ui.painter(), origin);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { CanvasRect {} }

#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
pub struct CanvasCircle;

#[typetag::serde]
impl Node for CanvasCircle {
    fn type_name(&self) -> String {
        "Canvas Circle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let (Some(x), Some(y)) = (ctx.constraints.x, ctx.constraints.y) else {
            return DrawResult::Complete { region: None };
        };
        let size = Vector {
            x: x.provided_value(),
            y: y.provided_value(),
        };
        let center = ctx.constraints.pos + size / 2.0;
        let region = shapes::Circle {
            radius: size.x.min(size.y) / 2.0,
            border: Stroke::NONE,
            fill_color: Color32::RED,
        }
        .paint(ctx.ui.painter(), center);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { CanvasCircle {} }
