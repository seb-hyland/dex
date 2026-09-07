//! A boolean, chosen rather than typed.
//!
//! [`Integer`](crate::primitives::number::Integer) and
//! [`Float`](crate::primitives::number::Float) are fields, because a number is
//! written. A boolean has two values and no spelling worth arguing about, so it
//! is a two-item list: what it holds is always one of the two things it offers,
//! and there is no half-typed state to validate.

use dex_core::prelude::*;
use egui::Sense;
use utils::Transient;

use crate::primitives::dropdown::{RowStyle, draw_open_list, paint_row, row_height, row_width};
use crate::primitives::text::GetText;

/// The two values, in the order the list offers them.
const CHOICES: [(&str, bool); 2] = [("True", true), ("False", false)];

/// A boolean, picked from a two-item dropdown.
#[utils::dynamic_type]
#[utils::portable]
pub struct Bool {
    pub value: bool,
    /// Whether the list is showing.
    open: Transient<bool>,
}

#[utils::dynamic_methods]
impl Bool {
    pub fn new(value: bool) -> Self {
        Self {
            value,
            open: Transient::default(),
        }
    }
}

impl Bool {
    /// The word this value goes by.
    fn word(value: bool) -> &'static str {
        if value { "True" } else { "False" }
    }

    /// The two words, as the shared control wants them.
    fn words() -> Vec<String> {
        CHOICES.iter().map(|(word, _)| (*word).to_owned()).collect()
    }
}

#[utils::dynamic_node]
impl Node for Bool {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Boolean".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let style = RowStyle::default();
        let words = Self::words();
        // Wide enough for either word, so the control does not change size when
        // the value does.
        let natural = Vector {
            x: row_width(&ctx, &words, style),
            y: row_height(&ctx, Self::word(self.value), style),
        };
        let width = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) => w,
            Some(AxisConstraint::AtMost(w)) => natural.x.min(w),
            None => natural.x,
        };
        let header = ScreenRegion::from_min_size(
            ctx.constraints.pos,
            Vector {
                x: width,
                y: natural.y,
            },
        );

        // A sizing pass paints and senses nothing; it only reports how big this
        // comes out, and an interaction recorded here would be a phantom.
        if ctx.measuring() {
            return DrawResult::Complete {
                region: Some(header),
            };
        }

        let open = (*self.open.val()).unwrap_or(false);
        let hovered = ctx
            .ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| header.contains(p.into()));
        paint_row(
            &mut ctx,
            header,
            Self::word(self.value),
            style,
            hovered || open,
            true,
        );

        let hit = ctx
            .ui
            .interact(header.into(), ctx.widget_id(), Sense::CLICK);
        if hit.hovered() {
            ctx.set_cursor(CursorIcon::PointingHand);
        }
        if hit.clicked() {
            self.open.set(!open);
        }

        if open {
            let outcome = draw_open_list(&mut ctx, header, &words, style, "dex_bool_choice");
            if outcome.dismissed {
                self.open.set(false);
            }
            if let Some(index) = outcome.chosen {
                let value = CHOICES[index].1;
                if value != self.value {
                    ctx.submit_action_for_self::<Self, _>(SetBool { value }, "Chose a boolean");
                }
            }
        }

        DrawResult::Complete {
            region: Some(header),
        }
    }
}

defhandlers! { Bool {
    actions: [
        SetBool { value: bool } => (this, a) { this.value = a.value; },
    ],
    requests: [
        GetBool => (this, _q): bool { this.value },
    ],
    extern_requests: [
        // Read as text the way a number is, so a boolean wired into a
        // text-shaped sink says what it holds.
        GetText => (this, _q): String { Bool::word(this.value).to_owned() },
    ],
}}
