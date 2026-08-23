use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

/// Lay out `children` top-to-bottom, `spacing` apart.
#[derive(Clone, Serialize, Deserialize, Reset)]
pub struct VerticalLayout {
    pub children: Vec<Arc<dyn Node>>,
    pub spacing: f32,
}

#[typetag::serde]
impl Node for VerticalLayout {
    fn type_name(&self) -> String {
        "Vertical Layout".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx
            .constraints
            .x
            .map(|x_ax| x_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = ctx
            .constraints
            .y
            .map(|y_ax| y_ax.provided_value())
            .unwrap_or(f32::INFINITY);

        let mut consumed_height = 0.0;
        let mut max_width = 0.0_f32;
        let mut is_first = true;

        for child in &self.children {
            let y_offset = if is_first {
                // No spacing on top of first child
                consumed_height
            } else {
                consumed_height + self.spacing
            };

            if y_offset >= avail_h {
                // No vertical space left
                break;
            }

            let child_constraints = DrawConstraints {
                pos: ctx.constraints.pos
                    + Vector {
                        x: 0.0,
                        y: y_offset,
                    },
                x: Some(AxisConstraint::AtMost(avail_w)),
                y: Some(AxisConstraint::AtMost(avail_h - y_offset)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: ctx.constraints.should_clip,
            };

            if let Some(region) = ctx.draw_node(&**child, child_constraints).region() {
                let size = region.size();
                consumed_height = y_offset + size.y;
                max_width = max_width.max(size.x);
                is_first = false;
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                ctx.constraints.pos,
                Vector {
                    x: max_width,
                    y: consumed_height,
                },
            )),
        }
    }
}

defhandlers! { VerticalLayout {} }
