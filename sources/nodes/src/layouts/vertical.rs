use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::layouts::LayoutChild;

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct VerticalLayout {
    pub children: Vec<LayoutChild>,
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

        let origin = match ctx.constraints.pos {
            PositionConstraint::TopLeft(tl) => tl,
            PositionConstraint::Center(_) => {
                let last_known_size = ctx.this_node_last_frame_location().map(|reg| reg.size());
                let last_size_estimate = match last_known_size {
                    Some(s) => s,
                    None => {
                        // We're going to guess, don't draw this frame
                        ctx.request_skip_frame();
                        Vector::splat(300.0)
                    }
                };
                ctx.constraints.pos.to_top_left(last_size_estimate)
            }
        };

        let mut consumed_height = 0.0;
        let mut max_width = 0.0_f32;
        let mut is_first = true;

        for (idx, child) in self.children.iter().enumerate() {
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

            let constraints = DrawConstraints {
                pos: PositionConstraint::TopLeft(
                    origin
                        + Vector {
                            x: 0.0,
                            y: y_offset,
                        },
                ),
                x: Some(AxisConstraint::AtMost(avail_w)),
                y: Some(AxisConstraint::AtMost(avail_h - y_offset)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: ctx.constraints.should_clip,
            };

            let local_id = NodeUid::new_local(ctx.id, idx);
            if let Some(region) = child
                .draw(&mut ctx, local_id, constraints)
                .and_then(|res| res.region())
            {
                let size = region.size();
                consumed_height = y_offset + size.y;
                max_width = max_width.max(size.x);
                is_first = false;
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: max_width,
                    y: consumed_height,
                },
            )),
        }
    }
}

defhandlers! { VerticalLayout {} }
