use dex_core::prelude::*;
use egui::{Color32, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::primitives::shapes::{self, Line};

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
            stroke_kind: StrokeKind::Middle,
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

#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
pub struct SectionDivider;

#[typetag::serde]
impl Node for SectionDivider {
    fn type_name(&self) -> String {
        "Section Divider".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        const DIVIDER_HEIGHT: f32 = 2.0;
        const DIVIDER_COLOR: Color32 = Color32::DARK_GRAY;

        let Some(width) = ctx.constraints.x.map(|x_ax| x_ax.provided_value()) else {
            return DrawResult::Complete {
                region: Some(ScreenRegion::empty()),
            };
        };

        Line {
            span: Vector { x: width, y: 0.0 },
            stroke: Stroke {
                width: DIVIDER_HEIGHT,
                color: DIVIDER_COLOR,
            },
        }
        .paint(ctx.ui.painter(), ctx.constraints.pos);

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                ctx.constraints.pos,
                Vector {
                    x: width,
                    y: DIVIDER_HEIGHT,
                },
            )),
        }
    }
}

defhandlers! { SectionDivider {} }
