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

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let mut inner_draw_res = || -> Option<DrawResult> {
            let x_size = ctx.constraints.x?.provided_value();
            let y_size = ctx.constraints.y?.provided_value();
            let draw_rect = shapes::Rect {
                size: Vector {
                    x: x_size,
                    y: y_size,
                },
                border: Stroke::NONE,
                corner_radius: 5.0,
                fill_color: Color32::RED,
            };

            Some(ctx.draw_node(
                &draw_rect,
                NodeUid::new_local(ctx.node.id, "inner rect"),
                ctx.constraints,
            ))
        };
        match inner_draw_res() {
            Some(res) => res,
            None => DrawResult::Complete { region: None },
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

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let mut inner_draw_res = || -> Option<DrawResult> {
            let x_size = ctx.constraints.x?.provided_value();
            let y_size = ctx.constraints.y?.provided_value();
            let draw_circle = shapes::Circle {
                radius: x_size.min(y_size) / 2.0,
                border: Stroke::NONE,
                fill_color: Color32::RED,
            };

            Some(ctx.draw_node(
                &draw_circle,
                NodeUid::new_local(ctx.node.id, "inner Circle"),
                ctx.constraints,
            ))
        };
        match inner_draw_res() {
            Some(res) => res,
            None => DrawResult::Complete { region: None },
        }
    }
}

defhandlers! { CanvasCircle {} }
