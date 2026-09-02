use dex_core::prelude::*;
use dex_core::theme;

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
            padding: theme::SPACE_LG,
            corner_radius: theme::RADIUS_MD,
            fill_color: Color::TRANSPARENT,
            border_width: theme::HAIRLINE,
            border_color: theme::LINE,
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
            let avail = ctx.constraints.available();

            let child_size_with_padding = region
                .size()
                .map(|d| d + 2.0 * (self.padding + self.border_width));
            let box_size = Vector {
                x: child_size_with_padding.x.min(avail.x),
                y: child_size_with_padding.y.min(avail.y),
            };
            // Fill and border occupy the reserved slot, behind the child.
            let frame = Rect {
                size: box_size,
                corner_radius: self.corner_radius,
                fill_color: self.fill_color,
                border: Stroke::new(self.border_width, self.border_color),
                stroke_kind: StrokeKind::Inside,
            };
            ctx.ui
                .painter()
                .set(bg_idx, frame.shape(ctx.constraints.pos));

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
