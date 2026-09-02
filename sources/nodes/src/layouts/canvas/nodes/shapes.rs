use dex_core::prelude::*;
use dex_core::theme;

use crate::primitives::shapes::{self, Path};

#[derive(Copy)]
#[utils::portable(noop_reset)]
pub struct CanvasRect;

#[utils::dynamic_node(skip)]
impl Node for CanvasRect {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Rect".into()
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
            corner_radius: theme::RADIUS_MD,
            fill_color: Color::rgb(255, 0, 0),
            stroke_kind: StrokeKind::Middle,
        }
        .paint(ctx.ui.painter(), origin);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { CanvasRect {} }

#[derive(Copy)]
#[utils::portable(noop_reset)]
pub struct CanvasCircle;

#[utils::dynamic_node(skip)]
impl Node for CanvasCircle {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Circle".into()
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
            fill_color: Color::rgb(255, 0, 0),
        }
        .paint(ctx.ui.painter(), center);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { CanvasCircle {} }

#[derive(Copy)]
#[utils::portable(noop_reset)]
pub struct SectionDivider;

#[utils::dynamic_node(skip)]
impl Node for SectionDivider {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Section Divider".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        const DIVIDER_HEIGHT: f32 = theme::HAIRLINE;
        const DIVIDER_COLOR: Color = theme::LINE;

        let Some(width) = ctx.constraints.x.map(|x_ax| x_ax.provided_value()) else {
            return DrawResult::Complete {
                region: Some(ScreenRegion::empty()),
            };
        };

        Path::span(
            Vector { x: width, y: 0.0 },
            Stroke {
                width: DIVIDER_HEIGHT,
                color: DIVIDER_COLOR,
            },
        )
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
