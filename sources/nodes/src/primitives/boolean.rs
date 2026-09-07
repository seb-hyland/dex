//! A boolean, chosen rather than typed.

use dex_core::prelude::*;
use dex_core::theme;
use egui::Sense;
use utils::Transient;

use crate::primitives::icon::{Glyph, Icon};
use crate::primitives::text::GetText;

/// The two values, in the order the list offers them.
const CHOICES: [(&str, bool); 2] = [("True", true), ("False", false)];

/// Space between a row's text and its edges.
const PAD_X: f32 = theme::SPACE_MD;
const PAD_Y: f32 = theme::SPACE_SM;
/// The caret's side, and the room left for it beside the word.
const CARET: f32 = 8.0;
const CARET_GAP: f32 = theme::SPACE_MD;

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
}

#[utils::dynamic_node]
impl Node for Bool {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Boolean".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let origin = ctx.constraints.pos;
        let font = theme::text();
        let row_h = ctx.row_height(font) + 2.0 * PAD_Y;

        // Wide enough for either word, so the control does not change size when
        // the value does.
        let widest = CHOICES
            .iter()
            .map(|(word, _)| ctx.measure_text((*word).to_owned(), font, TextWrap::singleline()))
            .fold(0.0f32, |w, m| w.max(m.width));
        let natural_w = widest + CARET + CARET_GAP + 2.0 * PAD_X;
        let width = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) => w,
            Some(AxisConstraint::AtMost(w)) => natural_w.min(w),
            None => natural_w,
        };

        let header = ScreenRegion::from_min_size(origin, Vector { x: width, y: row_h });
        let open = (*self.open.val()).unwrap_or(false);

        // A sizing pass paints and senses nothing; it only reports how big this
        // comes out, and an interaction recorded here would be a phantom.
        if ctx.measuring() {
            return DrawResult::Complete {
                region: Some(header),
            };
        }

        let hovered = ctx
            .ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| header.contains(p.into()));
        paint_row(&mut ctx, header, Self::word(self.value), hovered || open);

        // The caret, at the right-hand end of the row.
        let caret_tl = ScreenPos {
            x: header.max.x - PAD_X - CARET,
            y: origin.y + (row_h - CARET) * 0.5,
        };
        Icon::new(Glyph::ChevronDown, CARET, theme::INK_FAINT).paint(ctx.ui.painter(), caret_tl);

        let header_hit = ctx
            .ui
            .interact(header.into(), ctx.widget_id(), Sense::CLICK);
        if header_hit.clicked() {
            self.open.set(!open);
        }
        if header_hit.hovered() {
            ctx.set_cursor(CursorIcon::PointingHand);
        }

        if open {
            self.draw_choices(&mut ctx, header);
        }

        DrawResult::Complete {
            region: Some(header),
        }
    }
}

impl Bool {
    /// Draw the open list under `header`, and act on a choice made in it.
    fn draw_choices(&self, ctx: &mut DrawContext, header: ScreenRegion) {
        let to_global = ctx.ui.ctx().layer_transform_to_global(ctx.ui.layer_id());
        let anchor = |p: ScreenPos| -> ScreenPos {
            to_global.map_or(p, |t| ScreenPos::from(t.mul_pos(egui::Pos2::from(p))))
        };
        let top_left = anchor(ScreenPos {
            x: header.min.x,
            y: header.max.y,
        });
        let size = header.size();

        let pointer: Option<ScreenPos> = ctx.ui.ctx().pointer_latest_pos().map(Into::into);
        let mut picked: Option<bool> = None;
        let mut left_open = false;
        ctx.overlay(|ctx| {
            for (i, (word, value)) in CHOICES.iter().enumerate() {
                let row = ScreenRegion::from_min_size(
                    ScreenPos {
                        x: top_left.x,
                        y: top_left.y + i as f32 * size.y,
                    },
                    size,
                );
                let hovered = pointer.is_some_and(|p| row.contains(p));
                paint_row(ctx, row, word, hovered);

                let hit = ctx.ui.interact(
                    row.into(),
                    ctx.widget_id().with(("dex_bool_choice", i)),
                    Sense::CLICK,
                );
                if hit.hovered() {
                    ctx.set_cursor(CursorIcon::PointingHand);
                }
                if hit.clicked() {
                    picked = Some(*value);
                }
                // A press anywhere else puts the list away; a press on a row is
                // the choice itself, which closes it either way.
                left_open |= hit.hovered() || hit.is_pointer_button_down_on();
            }
            if ctx.ui.ctx().input(|i| i.pointer.any_pressed()) && !left_open {
                self.open.val_mut().replace(false);
            }
        });

        if let Some(value) = picked {
            self.open.val_mut().replace(false);
            if value != self.value {
                ctx.submit_action_for_self::<Self, _>(SetBool { value }, "Chose a boolean");
            }
        }
    }
}

/// One row of the control: a bordered ground with a word on it.
fn paint_row(ctx: &mut DrawContext, region: ScreenRegion, word: &str, lit: bool) {
    let font = theme::text();
    crate::primitives::shapes::Rect {
        size: region.size(),
        corner_radius: theme::RADIUS_SM,
        fill_color: if lit {
            theme::SURFACE_ALT
        } else {
            theme::SURFACE
        },
        border: if lit {
            theme::border_hover()
        } else {
            theme::border()
        },
        stroke_kind: StrokeKind::Inside,
    }
    .paint(ctx.ui.painter(), region.min);

    let galley = ctx.lay_out_text(word, font, theme::INK, TextWrap::singleline());
    let text_pos = egui::Pos2 {
        x: region.min.x + PAD_X,
        y: region.min.y + (region.size().y - galley.rect.height()) * 0.5,
    };
    ctx.ui.painter().galley(text_pos, galley, theme::INK.into());
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
