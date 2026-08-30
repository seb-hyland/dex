use dex_core::prelude::*;

use crate::layouts::child::LayoutChild;
use crate::primitives::shapes::Rect;

#[utils::dynamic_type]
#[utils::portable]
pub struct Bordered {
    // `LayoutChild` isn't bindable; compose it from node handles/values instead.
    #[dynamic(skip)]
    pub child: LayoutChild,
    pub padding: f32,
    pub corner_radius: f32,
    pub fill_color: Color,
    pub border_width: f32,
    pub border_color: Color,
}

#[utils::dynamic_methods]
impl Bordered {
    /// Wrap `child` in a border with sensible defaults. Tweak the `padding`,
    /// `corner_radius`, colours, and `border_width` fields to taste.
    pub fn new(child: LayoutChild) -> Bordered {
        Bordered {
            child,
            padding: 8.0,
            corner_radius: 4.0,
            fill_color: Color::TRANSPARENT,
            border_width: 1.0,
            border_color: Color::gray(170),
        }
    }
}

#[utils::dynamic_node]
impl Node for Bordered {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Bordered Node".into()
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

        // The box sizes to the child, so we can't paint until the child is drawn, but the fill must sit behind.
        // This reserves a slot.
        let bg_idx = ctx.ui.painter().add(egui::Shape::Noop);

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
            let rect = egui::Rect::from_min_size(
                ctx.constraints.pos.into(),
                egui::vec2(box_size.x, box_size.y),
            );

            // Fill occupies the reserved slot (behind the child).
            ctx.ui.painter().set(
                bg_idx,
                egui::Shape::rect_filled(rect, self.corner_radius, self.fill_color),
            );
            Rect {
                size: box_size,
                corner_radius: self.corner_radius,
                fill_color: Color::TRANSPARENT,
                border: Stroke::new(self.border_width, self.border_color),
                stroke_kind: StrokeKind::Inside,
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
