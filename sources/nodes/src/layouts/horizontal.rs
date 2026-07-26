use serde::{Deserialize, Serialize};
use utils::Reset;
use workspace::prelude::*;

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct HorizontalLayout {
    pub children: Vec<NodeUid>,
    pub allow_wrap: bool,
    pub wrap_spacing: f32,
}

#[typetag::serde]
impl Node for HorizontalLayout {
    fn type_name(&self) -> String {
        "Horizontal Layout".into()
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
                let last_known_size = ctx.last_frame_location().map(|reg| reg.size());
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

        let mut cur_line_width = 0.0;
        let mut cur_line_highest_element = 0.0;
        let mut consumed_height = 0.0;
        let mut max_line_width = 0.0;

        /**
           Starts a new horizontal line.
           ## Returns
           - `true` if all space has been consumed and the caller should return
           - `false` otherwise
        */
        #[must_use]
        fn start_newline(
            cur_line_width: &mut f32,
            cur_line_highest_element: &mut f32,
            cur_used_height: &mut f32,
            cur_max_width: &mut f32,
            padding: f32,
            avail_h: f32,
            can_wrap: bool,
        ) -> bool {
            *cur_used_height += *cur_line_highest_element;
            *cur_max_width = cur_max_width.max(*cur_line_width);
            *cur_line_highest_element = 0.0;
            *cur_line_width = 0.0;

            if !can_wrap {
                // Cannot start new lines, must return
                true
            } else {
                if *cur_used_height >= avail_h {
                    // No more space
                    true
                } else {
                    // Can start newline!
                    *cur_used_height += padding;
                    false
                }
            }
        }

        fn compute_draw_res(origin: ScreenPos, width: f32, height: f32) -> DrawResult {
            let consumed_region = ScreenRegion {
                min: origin,
                max: origin
                    + Vector {
                        x: width,
                        y: height,
                    },
            };
            DrawResult::Complete {
                region: Some(consumed_region),
            }
        }

        fn update_cursors(
            maybe_new_region: Option<ScreenRegion>,
            cur_line_width: &mut f32,
            cur_line_highest_element: &mut f32,
        ) {
            let Some(new_region) = maybe_new_region else {
                return;
            };
            let consumed_size = new_region.size();
            *cur_line_width += consumed_size.x;
            *cur_line_highest_element = cur_line_highest_element.max(consumed_size.y);
        }

        for child in &self.children {
            if avail_w - cur_line_width <= 0.0 {
                // No space left in line
                if start_newline(
                    &mut cur_line_width,
                    &mut cur_line_highest_element,
                    &mut consumed_height,
                    &mut max_line_width,
                    self.wrap_spacing,
                    avail_h,
                    self.allow_wrap,
                ) {
                    // No space left at all
                    return compute_draw_res(origin, max_line_width, consumed_height);
                }
            }

            let constraints = DrawConstraints {
                pos: PositionConstraint::TopLeft(
                    origin
                        + Vector {
                            x: cur_line_width,
                            y: consumed_height,
                        },
                ),
                x: Some(AxisConstraint::AtMost(avail_w - cur_line_width)),
                y: Some(AxisConstraint::AtMost(avail_h - consumed_height)),
                can_request_wrap: self.allow_wrap,
                continuation: None,
            };
            let Some(mut child_draw_res) = ctx.draw_workspace_node(*child, constraints) else {
                // This child no longer exists
                // TODO: delete it
                continue;
            };

            let maybe_region = child_draw_res.region();
            update_cursors(
                maybe_region,
                &mut cur_line_width,
                &mut cur_line_highest_element,
            );

            while let DrawResult::Wrap { continuation, .. } = child_draw_res {
                // Need to draw again on a new line

                if start_newline(
                    &mut cur_line_width,
                    &mut cur_line_highest_element,
                    &mut consumed_height,
                    &mut max_line_width,
                    self.wrap_spacing,
                    avail_h,
                    self.allow_wrap,
                ) {
                    // No space left at all
                    return compute_draw_res(origin, max_line_width, consumed_height);
                }

                let constraints = DrawConstraints {
                    pos: PositionConstraint::TopLeft(
                        origin
                            + Vector {
                                x: cur_line_width,
                                y: consumed_height,
                            },
                    ),
                    x: Some(AxisConstraint::AtMost(avail_w - cur_line_width)),
                    y: Some(AxisConstraint::AtMost(avail_h - consumed_height)),
                    can_request_wrap: self.allow_wrap,
                    continuation: Some(continuation),
                };
                let child_continuation_draw_res = ctx
                    .draw_workspace_node(*child, constraints)
                    .expect("Child should not be deleted mid-draw");

                let maybe_region = child_continuation_draw_res.region();
                update_cursors(
                    maybe_region,
                    &mut cur_line_width,
                    &mut cur_line_highest_element,
                );

                // To keep the loop going
                child_draw_res = child_continuation_draw_res;
            }
        }

        // Final update pass on cursors
        consumed_height += cur_line_highest_element;
        max_line_width = max_line_width.max(cur_line_width);

        compute_draw_res(origin, max_line_width, consumed_height)
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for HorizontalLayout {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
