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

/// The region a child settled on, or `None` if it drew nothing or asked to wrap.
fn completed(res: &DrawResult) -> Option<ScreenRegion> {
    match res {
        DrawResult::Complete { region } => *region,
        DrawResult::Wrap { .. } => None,
    }
}

impl Bordered {
    /// The frame around a child that occupied `region`, clamped to what this node was offered.
    fn frame(&self, region: ScreenRegion, constraints: &DrawConstraints) -> (Rect, Vector) {
        let avail = constraints.available();
        let inset = self.padding + self.border_width;
        let with_padding = region.size().map(|d| d + 2.0 * inset);
        let size = Vector {
            x: with_padding.x.min(avail.x),
            y: with_padding.y.min(avail.y),
        };
        let frame = Rect {
            size,
            corner_radius: self.corner_radius,
            fill_color: self.fill_color,
            border: Stroke::new(self.border_width, self.border_color),
            stroke_kind: StrokeKind::Inside,
        };
        (frame, size)
    }
}

#[utils::dynamic_node]
impl Node for Bordered {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Bordered Node".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let inset = self.padding + self.border_width;
        let child_constraints = DrawConstraints {
            pos: constraints.pos + Vector::splat(inset),
            wrap: WrapConstraints::NotAllowed,
            ..constraints.shrunk_by_per_side(inset, inset)
        };

        let child_res = ctx.with_backdrop(
            |ctx| self.child.draw(ctx, child_constraints),
            // Only a child that finished gets a frame.
            // One that ran out of room and asked to wrap has not settled on a size yet.
            |res| completed(res).map(|r| self.frame(r, &constraints).0.shape(constraints.pos)),
        );

        let Some(region) = completed(&child_res) else {
            return DrawResult::Complete { region: None };
        };
        let (_, size) = self.frame(region, &constraints);
        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(constraints.pos, size)),
        }
    }
}

defhandlers! { Bordered {} }
