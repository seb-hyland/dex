use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

/**
    Lay out `children` left-to-right, separated by `spacing`.
    When `allow_wrap` is set, wrap onto new rows.
*/
#[derive(Clone, Serialize, Deserialize, Reset)]
pub struct HorizontalLayout {
    pub children: Vec<Arc<dyn Node>>,
    pub spacing: f32,
    pub allow_wrap: bool,
}

defhandlers! { HorizontalLayout {} }

#[typetag::serde]
impl Node for HorizontalLayout {
    fn type_name(&self) -> String {
        "Horizontal Layout".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;

        let avail_w = constraints
            .x
            .map(|x_ax| x_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = constraints
            .y
            .map(|y_ax| y_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let should_clip = constraints.should_clip;

        let origin = constraints.pos;

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
        fn start_newline_still_has_space(
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
            } else if *cur_used_height >= avail_h {
                // No more space
                true
            } else {
                // Can start newline!
                *cur_used_height += padding;
                false
            }
        }

        let compute_draw_res = |origin: ScreenPos, width: f32, height: f32| -> DrawResult {
            DrawResult::Complete {
                region: Some(ScreenRegion::from_min_size(
                    origin,
                    Vector {
                        x: width,
                        y: height,
                    },
                )),
            }
        };

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
                if start_newline_still_has_space(
                    &mut cur_line_width,
                    &mut cur_line_highest_element,
                    &mut consumed_height,
                    &mut max_line_width,
                    self.spacing,
                    avail_h,
                    self.allow_wrap,
                ) {
                    // No space left at all
                    return compute_draw_res(origin, max_line_width, consumed_height);
                }
            }

            // Gap before every item but the first on its line.
            if cur_line_width > 0.0 {
                cur_line_width += self.spacing;
            }

            let child_constraints = DrawConstraints {
                pos: origin
                    + Vector {
                        x: cur_line_width,
                        y: consumed_height,
                    },
                x: Some(AxisConstraint::AtMost(avail_w - cur_line_width)),
                y: Some(AxisConstraint::AtMost(avail_h - consumed_height)),
                wrap: if self.allow_wrap {
                    WrapConstraints::CanRequest {
                        at_start_of_line: cur_line_width == 0.0,
                        continuation: None,
                    }
                } else {
                    WrapConstraints::NotAllowed
                },
                should_clip,
            };
            let mut child_draw_res = ctx.draw_node(&**child, child_constraints);

            let maybe_region = child_draw_res.region();
            update_cursors(
                maybe_region,
                &mut cur_line_width,
                &mut cur_line_highest_element,
            );

            while self.allow_wrap
                && let DrawResult::Wrap { continuation, .. } = child_draw_res
            {
                // Need to draw again on a new line

                if start_newline_still_has_space(
                    &mut cur_line_width,
                    &mut cur_line_highest_element,
                    &mut consumed_height,
                    &mut max_line_width,
                    self.spacing,
                    avail_h,
                    true,
                ) {
                    // No space left at all
                    return compute_draw_res(origin, max_line_width, consumed_height);
                }

                let child_constraints = DrawConstraints {
                    pos: origin
                        + Vector {
                            x: cur_line_width,
                            y: consumed_height,
                        },
                    x: Some(AxisConstraint::AtMost(avail_w - cur_line_width)),
                    y: Some(AxisConstraint::AtMost(avail_h - consumed_height)),
                    wrap: WrapConstraints::CanRequest {
                        at_start_of_line: true,
                        continuation: Some(continuation),
                    },
                    should_clip,
                };
                let child_continuation_draw_res = ctx.draw_node(&**child, child_constraints);

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
}
