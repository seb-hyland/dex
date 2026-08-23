use dex_core::prelude::*;

use crate::layouts::child::LayoutChild;
use crate::primitives::shapes::Rect;

#[utils::portable]
pub struct Bordered {
    pub child: LayoutChild,
    pub padding: f32,
    pub corner_radius: f32,
    pub fill_color: Color,
    pub border_width: f32,
    pub border_color: Color,
}

#[typetag::serde]
impl Node for Bordered {
    fn type_name(&self) -> String {
        "Bordered".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let child_pos = ctx.constraints.pos + Vector::splat(self.padding + self.border_width);
        let child_constraints = DrawConstraints {
            pos: child_pos,
            wrap: WrapConstraints::NotAllowed,
            ..ctx.constraints.shrunk_by_per_side(
                self.padding + self.border_width,
                self.padding + self.border_width,
            )
        };
        let child_res = self.child.draw(&mut ctx, child_constraints);

        if let DrawResult::Complete {
            region: maybe_region,
        } = child_res
            && let Some(region) = maybe_region
        {
            let avail_x = ctx
                .constraints
                .x
                .map(|x_ax| x_ax.provided_value())
                .unwrap_or(f32::INFINITY);
            let avail_y = ctx
                .constraints
                .y
                .map(|y_ax| y_ax.provided_value())
                .unwrap_or(f32::INFINITY);

            let child_size_with_padding = region
                .size()
                .map(|d| d + 2.0 * (self.padding + self.border_width));
            let box_size = Vector {
                x: child_size_with_padding.x.min(avail_x),
                y: child_size_with_padding.y.min(avail_y),
            };

            Rect {
                size: box_size,
                corner_radius: self.corner_radius,
                fill_color: self.fill_color.into(),
                border: egui::Stroke::new(self.border_width, self.border_color),
                stroke_kind: egui::StrokeKind::Inside,
            }
            .paint(ctx.ui.painter(), ctx.constraints.pos);

            DrawResult::Complete {
                region: Some(ScreenRegion::from_min_size(ctx.constraints.pos, box_size)),
            }
        } else {
            // Child failed to draw
            DrawResult::Complete { region: None }
        }
    }
}

defhandlers! { Bordered {} }
