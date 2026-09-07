//! A list of choices, showing the one taken.
//!
//! The list opens *over* whatever is around it rather than pushing it aside.
//! These controls sit inside frames that clip — a canvas item, a lambda's
//! argument row — and a list drawn inside one would be cut off after the first
//! row. Which is also why the open list is placed in screen coordinates: a
//! canvas draws its items on a panned and zoomed layer, so the anchor is mapped
//! out the way a lambda's wires are. Off such a layer the mapping is identity.

use dex_core::prelude::*;
use dex_core::theme;
use egui::Sense;
use utils::Transient;

use crate::primitives::icon::{Glyph, Icon};

/// Space between a boxed row's text and its edges.
pub const PAD_X: f32 = theme::SPACE_MD;
pub const PAD_Y: f32 = theme::SPACE_SM;
/// The caret's side, and the room left for it beside the word.
pub const CARET: f32 = 8.0;
pub const CARET_GAP: f32 = theme::SPACE_SM;
/// How far the open list stands off the row that opened it.
const LIST_GAP: f32 = theme::SPACE_XS;
/// The panel's own margin around its rows.
const LIST_INSET: f32 = theme::SPACE_XS;

/// How a dropdown's rows are drawn.
#[derive(Clone, Copy)]
pub struct RowStyle {
    pub font: Font,
    pub color: Color,
    /// Whether a row wears a ground and a border, as a control does. A
    /// dropdown standing in a line of text does not; one standing alone does.
    pub boxed: bool,
    /// Whether a row leaves room around its text. Separate from [`RowStyle::boxed`]
    /// because the open list wants the room without the frame: its rows sit on
    /// one panel, and a border apiece would read as a stack of little controls.
    pub padded: bool,
}

impl Default for RowStyle {
    fn default() -> Self {
        Self {
            font: theme::text(),
            color: theme::INK,
            boxed: true,
            padded: true,
        }
    }
}

/**
    The room a row leaves around its text.

    None at all when it is unboxed. An unboxed dropdown is a word standing in a
    line of other words — the type in a lambda's argument row — and any padding
    at all drops it below the baseline of the labels beside it, so it reads as a
    control that has come adrift rather than as part of the sentence.
*/
fn padding(style: RowStyle) -> Vector {
    if style.padded {
        Vector { x: PAD_X, y: PAD_Y }
    } else {
        Vector::ZERO
    }
}

/**
    How tall a row showing `text` comes out.

    The height the text actually laid out to, not the font's nominal row height:
    a label beside it is sized by its own layout, and the two differ by a couple
    of points — enough for an unboxed dropdown to sit visibly off the line it is
    meant to be standing on.
*/
pub fn row_height(ctx: &DrawContext, text: &str, style: RowStyle) -> f32 {
    let measured = text_size(ctx, text, style);
    measured.height.max(measured.row_height) + 2.0 * padding(style).y
}

/// How `text` lays out in this row's font.
fn text_size(ctx: &DrawContext, text: &str, style: RowStyle) -> TextMetrics {
    ctx.measure_text(text.to_owned(), style.font, TextWrap::singleline())
}

/// How wide a row showing `text` comes out, with no caret on it.
pub fn item_width(ctx: &DrawContext, text: &str, style: RowStyle) -> f32 {
    text_size(ctx, text, style).width + 2.0 * padding(style).x
}

/// How wide a row showing `text` comes out, with room for the caret.
pub fn text_width(ctx: &DrawContext, text: &str, style: RowStyle) -> f32 {
    item_width(ctx, text, style) + CARET + CARET_GAP
}

/// How wide a dropdown must be to show any of `options` without changing size.
pub fn row_width(ctx: &DrawContext, options: &[String], style: RowStyle) -> f32 {
    options
        .iter()
        .map(|word| text_width(ctx, word, style))
        .fold(0.0f32, f32::max)
}

/// Draw one row: a ground, the word on it, and — on the closed row — a caret.
pub fn paint_row(
    ctx: &mut DrawContext,
    region: ScreenRegion,
    word: &str,
    style: RowStyle,
    lit: bool,
    caret: bool,
) {
    if style.boxed || lit {
        crate::primitives::shapes::Rect {
            size: region.size(),
            corner_radius: theme::RADIUS_SM,
            fill_color: if lit {
                theme::SURFACE_ALT
            } else {
                theme::SURFACE
            },
            border: if !style.boxed {
                Stroke::NONE
            } else if lit {
                theme::border_hover()
            } else {
                theme::border()
            },
            stroke_kind: StrokeKind::Inside,
        }
        .paint(ctx.ui.painter(), region.min);
    }

    let pad = padding(style);
    let galley = ctx.lay_out_text(word, style.font, style.color, TextWrap::singleline());
    let text_pos = egui::Pos2 {
        x: region.min.x + pad.x,
        y: region.min.y + (region.size().y - galley.rect.height()) * 0.5,
    };
    ctx.ui
        .painter()
        .galley(text_pos, galley, style.color.into());

    if caret {
        let caret_tl = ScreenPos {
            x: region.max.x - pad.x - CARET,
            y: region.min.y + (region.size().y - CARET) * 0.5,
        };
        // The caret belongs to the word, so it is drawn in the word's own ink
        // rather than in one of its own.
        Icon::new(Glyph::ChevronDown, CARET, style.color).paint(ctx.ui.painter(), caret_tl);
    }
}

/// What pressing about in an open list came to.
pub struct ListOutcome {
    /// The option pressed, if one was.
    pub chosen: Option<usize>,
    /// Whether the list should be put away: a press landed outside it.
    pub dismissed: bool,
}

/**
    Draw the open list of `options` under `header`, over everything else.

    `salt` separates one list's row ids from another's, for a node drawing more
    than one.
*/
pub fn draw_open_list(
    ctx: &mut DrawContext,
    header: ScreenRegion,
    options: &[String],
    style: RowStyle,
    salt: &'static str,
) -> ListOutcome {
    let to_global = ctx.ui.ctx().layer_transform_to_global(ctx.ui.layer_id());
    let anchor = |p: ScreenPos| -> ScreenPos {
        to_global.map_or(p, |t| ScreenPos::from(t.mul_pos(egui::Pos2::from(p))))
    };
    let top_left = anchor(ScreenPos {
        x: header.min.x,
        y: header.max.y + LIST_GAP,
    });

    /*
        The list is one panel with rows on it, not a column of controls.

        Its rows always take their padding, whatever the closed row does — a
        list of words with nothing around them is a wall of text — but wear no
        frame of their own, because the panel is the frame. And they are sized
        to the longest option rather than to the header, which after the header
        learned to shrink to its own word is nowhere near wide enough.
    */
    let listed = RowStyle {
        boxed: false,
        padded: true,
        ..style
    };
    let size = Vector {
        x: options
            .iter()
            .map(|word| item_width(ctx, word, listed))
            .fold(header.size().x, f32::max),
        y: options
            .iter()
            .map(|word| row_height(ctx, word, listed))
            .fold(0.0, f32::max),
    };
    let panel = ScreenRegion::from_min_size(
        top_left,
        Vector {
            x: size.x + 2.0 * LIST_INSET,
            y: size.y * options.len() as f32 + 2.0 * LIST_INSET,
        },
    );

    let pointer: Option<ScreenPos> = ctx.ui.ctx().pointer_latest_pos().map(Into::into);
    let mut chosen = None;
    let mut within = false;
    ctx.overlay(|ctx| {
        // A shadow, so the list reads as lying over the page rather than in it.
        crate::primitives::shapes::Rect {
            size: panel.size(),
            corner_radius: theme::RADIUS_MD,
            fill_color: Color::rgba(0, 0, 0, 26),
            border: Stroke::NONE,
            stroke_kind: StrokeKind::Inside,
        }
        .paint(ctx.ui.painter(), panel.min + Vector { x: 0.0, y: 2.0 });
        crate::primitives::shapes::Rect {
            size: panel.size(),
            corner_radius: theme::RADIUS_MD,
            fill_color: theme::SURFACE,
            border: theme::border(),
            stroke_kind: StrokeKind::Inside,
        }
        .paint(ctx.ui.painter(), panel.min);

        for (i, word) in options.iter().enumerate() {
            let row = ScreenRegion::from_min_size(
                ScreenPos {
                    x: panel.min.x + LIST_INSET,
                    y: panel.min.y + LIST_INSET + i as f32 * size.y,
                },
                size,
            );
            let hovered = pointer.is_some_and(|p| row.contains(p));
            paint_row(ctx, row, word, listed, hovered, false);

            let hit = ctx
                .ui
                .interact(row.into(), ctx.widget_id().with((salt, i)), Sense::CLICK);
            if hit.hovered() {
                ctx.set_cursor(CursorIcon::PointingHand);
            }
            if hit.clicked() {
                chosen = Some(i);
            }
            within |= hit.hovered() || hit.is_pointer_button_down_on();
        }
        // The gap between the header and the panel is still part of the list:
        // crossing it must not count as pressing outside.
        within |= pointer.is_some_and(|p| panel.expand(LIST_GAP).contains(p));
    });

    let pressed_away = ctx.ui.ctx().input(|i| i.pointer.any_pressed()) && !within;
    ListOutcome {
        chosen,
        dismissed: chosen.is_some() || pressed_away,
    }
}

/// A list of named choices, holding the one taken.
#[utils::dynamic_type]
#[utils::portable]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected: usize,
    pub font: Font,
    pub color: Color,
    /// Whether the closed row wears a ground and a border. See [`RowStyle`].
    pub boxed: bool,
    /**
        Whether the closed row is only as wide as the option it is showing.

        A dropdown standing on its own keeps one width, so it does not jump
        about as the choice changes. One standing in a line of text is a word,
        and a word is as wide as it is — `any` should not be padded out to the
        width of `any satisfying` on the strength of a choice nobody made.
    */
    pub shrink_to_text: bool,

    /// Whether the list is showing. A gesture in progress, not something a
    /// saved workspace should come back wearing.
    open: Transient<bool>,
    /// A choice made, until its owner takes it.
    chosen: Transient<usize>,
}

#[utils::dynamic_methods]
impl Dropdown {
    /// A dropdown over `options`, showing the first.
    pub fn new(options: Vec<String>) -> Self {
        Self {
            options,
            selected: 0,
            font: theme::text(),
            color: theme::INK,
            boxed: true,
            shrink_to_text: false,
            open: Transient::default(),
            chosen: Transient::default(),
        }
    }

    /// Build one into `ws`, for an owner that addresses it by id.
    pub fn build(ws: WorkspaceActionHandle, options: Vec<String>) -> NodeUid<Dropdown> {
        ws.insert_node(Self::new(options))
    }
}

impl Dropdown {
    fn style(&self) -> RowStyle {
        RowStyle {
            font: self.font,
            color: self.color,
            boxed: self.boxed,
            // A framed row takes its padding; an unframed one is a word in a
            // line of words and takes none.
            padded: self.boxed,
        }
    }

    /// The word currently showing.
    fn shown(&self) -> &str {
        self.options
            .get(self.selected)
            .map(String::as_str)
            .unwrap_or("")
    }
}

#[utils::dynamic_node]
impl Node for Dropdown {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Dropdown".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let style = self.style();
        let natural = Vector {
            x: if self.shrink_to_text {
                text_width(&ctx, self.shown(), style)
            } else {
                row_width(&ctx, &self.options, style)
            },
            y: row_height(&ctx, self.shown(), style),
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

        // A sizing pass reports how big this comes out and nothing else; an
        // interaction recorded here would be a phantom.
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
        paint_row(&mut ctx, header, self.shown(), style, hovered || open, true);

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
            let outcome = draw_open_list(&mut ctx, header, &self.options, style, "dex_dropdown");
            if outcome.dismissed {
                self.open.set(false);
            }
            if let Some(index) = outcome.chosen {
                self.chosen.set(index);
                ctx.submit_action_for_self::<Self, _>(
                    SetDropdownSelection { index },
                    "Chose from a dropdown",
                );
            }
        }

        DrawResult::Complete {
            region: Some(header),
        }
    }
}

defhandlers! { Dropdown {
    actions: [
        SetDropdownSelection { index: usize } => (this, a) {
            if a.index < this.options.len() {
                this.selected = a.index;
            }
        },
        SetDropdownOptions { options: Vec<String> } => (this, a) {
            this.selected = this.selected.min(a.options.len().saturating_sub(1));
            this.options = a.options;
        },
    ],
    requests: [
        // Which option is showing.
        DropdownSelection => (this, _q): usize { this.selected },
        // A choice made since this was last asked, if any.
        TakeDropdownChoice => (this, _q): Option<usize> { this.chosen.val_mut().take() },
    ],
}}
