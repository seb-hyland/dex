//! The one place the app's look is decided.

use crate::style::{Color, Font, Stroke};

// ======================================================================
// Palette
// ======================================================================

/// The accent: selection, focus, the active tab, a live wire.
pub const ACCENT: Color = Color::rgb(70, 130, 180);
/// The accent, darkened for text on a light ground.
pub const ACCENT_STRONG: Color = Color::rgb(40, 80, 120);
/// The accent at a whisper, for the fill behind a selected row.
pub const ACCENT_SOFT: Color = Color::rgba(70, 130, 180, 26);
/// The accent drained of most of its colour, for the *edge* of a selected
/// thing. A full-strength border around a pale fill is all anyone sees, so
/// selection is carried by the fill and only hinted at by the outline.
pub const ACCENT_MUTED: Color = Color::rgb(168, 194, 219);

/// Primary text. Softer than pure black, which reads as harsh on white.
pub const INK: Color = Color::gray(34);
/// Secondary text: a heading over a group, a value beside its name.
pub const INK_MUTED: Color = Color::gray(110);
/// Tertiary text: hints, placeholders, an empty state.
pub const INK_FAINT: Color = Color::gray(150);

/// A hairline: the usual border, and the rule between sections. Very slightly
/// cool, so it sits under the text rather than competing with it.
pub const LINE: Color = Color::rgb(212, 216, 222);
/// A border that needs to be seen — hover, or a control's own edge.
pub const LINE_STRONG: Color = Color::rgb(169, 176, 186);

/// The page.
pub const SURFACE: Color = Color::WHITE;
/// A surface lifted off the page: a hovered control, a header row.
pub const SURFACE_ALT: Color = Color::rgb(242, 244, 247);
/// A surface pressed into the page: a held button, a well.
pub const SURFACE_SUNKEN: Color = Color::rgb(226, 230, 236);

/// Destructive actions, and anything that failed.
pub const DANGER: Color = Color::rgb(200, 60, 60);

// ======================================================================
// Type scale
// ======================================================================

/// A caption or a section heading over a group.
pub const TEXT_XS: f32 = 11.0;
/// Chrome: tab titles, small controls, secondary rows.
pub const TEXT_SM: f32 = 12.0;
/// The default. Buttons, labels, most of the interface.
pub const TEXT_BASE: f32 = 13.0;
/// Content on the canvas, where the text is the subject rather than the frame.
pub const TEXT_LG: f32 = 15.0;
/// A title.
pub const TEXT_XL: f32 = 18.0;

// ======================================================================
// Spacing and radii
// ======================================================================

/// A hair of separation: between a glyph and its label.
pub const SPACE_XS: f32 = 2.0;
/// Within a control.
pub const SPACE_SM: f32 = 4.0;
/// Between related controls in a row.
pub const SPACE_MD: f32 = 6.0;
/// Between rows, and around a panel's contents.
pub const SPACE_LG: f32 = 10.0;
/// Between sections.
pub const SPACE_XL: f32 = 16.0;

/// A checkbox, a swatch: something small enough that more would look round.
pub const RADIUS_SM: f32 = 3.0;
/// Buttons, pills, fields.
pub const RADIUS_MD: f32 = 5.0;
/// Panels, cards, a node on the canvas.
pub const RADIUS_LG: f32 = 8.0;

/// The width of every hairline in the UI.
pub const HAIRLINE: f32 = 1.0;

// ======================================================================
// Ready-made styles
// ======================================================================

/// The default hairline border.
pub const fn border() -> Stroke {
    Stroke {
        width: HAIRLINE,
        color: LINE,
    }
}

/// The border a control wears while the pointer is over it.
pub const fn border_hover() -> Stroke {
    Stroke {
        width: HAIRLINE,
        color: LINE_STRONG,
    }
}

/// The interface's default text.
pub const fn text() -> Font {
    Font::proportional(TEXT_BASE)
}

/// Chrome text: one step down from [`text`].
pub const fn text_small() -> Font {
    Font::proportional(TEXT_SM)
}

/// A section heading over a group of controls.
pub const fn text_heading() -> Font {
    Font::proportional(TEXT_XS)
}

// ======================================================================
// The script-facing view of all of the above
// ======================================================================

/**
    The app's design tokens, so a script-defined node can match the
    interface around it.

    ```python
    label.color = dex.Theme.ink_muted()
    label.font = dex.Theme.text_small()
    ```
*/
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct Theme;

#[utils::dynamic_methods]
impl Theme {
    /// Selection, focus, the active tab.
    pub fn accent() -> Color {
        ACCENT
    }
    /// The accent, darkened for text on a light ground.
    pub fn accent_strong() -> Color {
        ACCENT_STRONG
    }
    /// The accent at a whisper, for a selected row's fill.
    pub fn accent_soft() -> Color {
        ACCENT_SOFT
    }
    /// The accent drained of most of its colour, for a selected thing's edge.
    pub fn accent_muted() -> Color {
        ACCENT_MUTED
    }
    /// Primary text.
    pub fn ink() -> Color {
        INK
    }
    /// Secondary text.
    pub fn ink_muted() -> Color {
        INK_MUTED
    }
    /// Hints, placeholders, empty states.
    pub fn ink_faint() -> Color {
        INK_FAINT
    }
    /// The usual hairline border colour.
    pub fn line() -> Color {
        LINE
    }
    /// A border that needs to be seen.
    pub fn line_strong() -> Color {
        LINE_STRONG
    }
    /// The page.
    pub fn surface() -> Color {
        SURFACE
    }
    /// A surface lifted off the page: hover, a header row.
    pub fn surface_alt() -> Color {
        SURFACE_ALT
    }
    /// A surface pressed into the page.
    pub fn surface_sunken() -> Color {
        SURFACE_SUNKEN
    }
    /// Destructive actions, and anything that failed.
    pub fn danger() -> Color {
        DANGER
    }

    /// The interface's default text.
    pub fn text() -> Font {
        self::text()
    }
    /// Chrome text, one step down from [`Theme::text`].
    pub fn text_small() -> Font {
        self::text_small()
    }
    /// A section heading over a group of controls.
    pub fn text_heading() -> Font {
        self::text_heading()
    }
    /// Canvas content, where the text is the subject rather than the frame.
    pub fn text_content() -> Font {
        Font::proportional(TEXT_LG)
    }

    /// The default hairline border.
    pub fn border() -> Stroke {
        self::border()
    }
    /// The border a control wears while hovered.
    pub fn border_hover() -> Stroke {
        self::border_hover()
    }

    /// Within a control.
    pub fn space_sm() -> f32 {
        SPACE_SM
    }
    /// Between related controls in a row.
    pub fn space_md() -> f32 {
        SPACE_MD
    }
    /// Between rows, and around a panel's contents.
    pub fn space_lg() -> f32 {
        SPACE_LG
    }
    /// Between sections.
    pub fn space_xl() -> f32 {
        SPACE_XL
    }

    /// A checkbox, a swatch.
    pub fn radius_sm() -> f32 {
        RADIUS_SM
    }
    /// Buttons, pills, fields.
    pub fn radius_md() -> f32 {
        RADIUS_MD
    }
    /// Panels, cards, a node on the canvas.
    pub fn radius_lg() -> f32 {
        RADIUS_LG
    }
}
