use egui::{Color32, FontFamily, FontId, StrokeKind as EguiStrokeKind, TextFormat};

/// An sRGBA colour with unmultiplied alpha.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::gray(0);
    pub const WHITE: Self = Self::gray(255);
    pub const GRAY: Self = Self::gray(160);
}

#[utils::dynamic_methods]
impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const fn gray(level: u8) -> Self {
        Self::rgb(level, level, level)
    }
    pub const fn transparent() -> Self {
        Self::TRANSPARENT
    }
}

impl From<Color> for Color32 {
    fn from(c: Color) -> Self {
        Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
    }
}

impl From<Color32> for Color {
    fn from(c: Color32) -> Self {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        Self { r, g, b, a }
    }
}

/// A line style: a width and a colour.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

impl Stroke {
    pub const NONE: Self = Self::new(0.0, Color::TRANSPARENT);
}

#[utils::dynamic_methods]
impl Stroke {
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
    pub const fn none() -> Self {
        Self::NONE
    }
}

impl From<Stroke> for egui::Stroke {
    fn from(s: Stroke) -> Self {
        egui::Stroke::new(s.width, s.color)
    }
}

impl From<egui::Stroke> for Stroke {
    fn from(s: egui::Stroke) -> Self {
        Self {
            width: s.width,
            color: s.color.into(),
        }
    }
}

/// Where a stroke sits relative to a shape's edge.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum StrokeKind {
    Inside,
    Middle,
    Outside,
}

#[utils::dynamic_methods]
impl StrokeKind {
    pub fn inside() -> Self {
        Self::Inside
    }
    pub fn middle() -> Self {
        Self::Middle
    }
    pub fn outside() -> Self {
        Self::Outside
    }
}

impl From<StrokeKind> for EguiStrokeKind {
    fn from(s: StrokeKind) -> Self {
        match s {
            StrokeKind::Inside => Self::Inside,
            StrokeKind::Middle => Self::Middle,
            StrokeKind::Outside => Self::Outside,
        }
    }
}

impl From<EguiStrokeKind> for StrokeKind {
    fn from(s: EguiStrokeKind) -> Self {
        match s {
            EguiStrokeKind::Inside => Self::Inside,
            EguiStrokeKind::Middle => Self::Middle,
            EguiStrokeKind::Outside => Self::Outside,
        }
    }
}

/// How a run of text is broken when it is laid out.
#[derive(Copy, Default)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct TextWrap {
    /// Whether a newline in the text starts a new line on screen.
    pub break_on_newline: bool,
    /// The width to break at. Infinite for "however wide it comes out".
    pub max_width: f32,
    /// Cut an over-wide line off rather than wrapping it.
    pub truncate: bool,
}

#[utils::dynamic_methods]
impl TextWrap {
    /// One line. Newlines are not breaks, and nothing wraps.
    pub const fn singleline() -> Self {
        Self {
            break_on_newline: false,
            max_width: f32::INFINITY,
            truncate: false,
        }
    }
    /// Break where the text says so, but never wrap a long line.
    pub const fn multiline() -> Self {
        Self {
            break_on_newline: true,
            max_width: f32::INFINITY,
            truncate: false,
        }
    }
    /// Break at newlines, and wrap anything wider than `width`.
    pub const fn wrapped(width: f32) -> Self {
        Self {
            break_on_newline: true,
            max_width: width,
            truncate: false,
        }
    }
    /// Lay out on one line, cutting it off at `width`.
    pub const fn truncated(width: f32) -> Self {
        Self {
            break_on_newline: false,
            max_width: width,
            truncate: true,
        }
    }
}

/// What a run of text came out as, without drawing it.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    /// The height of the first line — the one a control centres against.
    pub row_height: f32,
    /// How many lines the text came to.
    pub rows: u32,
}

pub const BOLD_FAMILY: &str = "Literata-Bold";
pub const ITALIC_FAMILY: &str = "Literata-Italic";
pub const BOLD_ITALIC_FAMILY: &str = "Literata-BoldItalic";

#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct Font {
    pub size: f32,
    pub monospace: bool,

    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[utils::dynamic_methods]
impl Font {
    pub const fn proportional(size: f32) -> Self {
        Self::plain(size, false)
    }
    pub const fn monospaced(size: f32) -> Self {
        Self::plain(size, true)
    }
}

impl Font {
    /// An unstyled face at `size`, proportional or monospaced.
    const fn plain(size: f32, monospace: bool) -> Self {
        Self {
            size,
            monospace,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    fn styled_family(self) -> Option<&'static str> {
        // Only the proportional family ships styled faces.
        match (self.monospace, self.bold, self.italic) {
            (true, _, _) | (false, false, false) => None,
            (false, true, false) => Some(BOLD_FAMILY),
            (false, false, true) => Some(ITALIC_FAMILY),
            (false, true, true) => Some(BOLD_ITALIC_FAMILY),
        }
    }

    pub fn font_id_in(self, ctx: &egui::Context) -> FontId {
        let Some(name) = self.styled_family() else {
            return self.into();
        };
        let family = FontFamily::Name(name.into());
        if ctx.fonts(|fonts| fonts.families().contains(&family)) {
            FontId::new(self.size, family)
        } else {
            FontId::new(self.size, FontFamily::Proportional)
        }
    }

    pub fn text_format(self, ctx: &egui::Context, color: Color) -> TextFormat {
        TextFormat {
            underline: if self.underline {
                egui::Stroke::new(1.0, color)
            } else {
                egui::Stroke::NONE
            },
            ..TextFormat::simple(self.font_id_in(ctx), color.into())
        }
    }
}

/// Loses [`Font::underline`], which is not a property of a face. Prefer [`Font::font_id_in`].
impl From<Font> for FontId {
    fn from(f: Font) -> Self {
        let family = match f.styled_family() {
            Some(name) => FontFamily::Name(name.into()),
            None if f.monospace => FontFamily::Monospace,
            None => FontFamily::Proportional,
        };
        FontId::new(f.size, family)
    }
}

impl From<FontId> for Font {
    fn from(f: FontId) -> Self {
        let styled = match &f.family {
            FontFamily::Name(name) => &**name,
            _ => "",
        };
        Self {
            size: f.size,
            monospace: matches!(f.family, FontFamily::Monospace),
            bold: styled == BOLD_FAMILY || styled == BOLD_ITALIC_FAMILY,
            italic: styled == ITALIC_FAMILY || styled == BOLD_ITALIC_FAMILY,
            underline: false,
        }
    }
}

/// Declare [`CursorIcon`] and its two-way mapping onto egui's, from one list.
macro_rules! cursor_icons {
    ($($variant:ident $(as $egui:ident)?),* $(,)?) => {
        /**
            The pointer shape a node asks for while it is hovered.

            Set it from a `draw` with `ctx.set_cursor(...)`; it lasts for the
            frame, so a node that wants a cursor asks for it every frame.

            ```python
            if hovered:
                ctx.set_cursor(dex.CursorIcon.PointingHand)
            ```
        */
        #[derive(Copy, Default)]
        #[utils::dynamic_type]
        #[utils::portable(noop_reset)]
        pub enum CursorIcon {
            #[default]
            $($variant),*
        }

        impl From<CursorIcon> for egui::CursorIcon {
            fn from(c: CursorIcon) -> Self {
                match c {
                    $(CursorIcon::$variant => cursor_icons!(@egui $variant $($egui)?)),*
                }
            }
        }

        impl From<egui::CursorIcon> for CursorIcon {
            fn from(c: egui::CursorIcon) -> Self {
                match c {
                    $(cursor_icons!(@egui $variant $($egui)?) => Self::$variant),*
                }
            }
        }
    };
    // The egui variant a name maps onto: the same name unless renamed.
    (@egui $variant:ident) => { egui::CursorIcon::$variant };
    (@egui $variant:ident $egui:ident) => { egui::CursorIcon::$egui };
}

cursor_icons! {
    // `None` is a Python keyword, so the hidden cursor is named for what it does.
    Default, Hidden as None, ContextMenu, Help, PointingHand, Progress, Wait, Cell,
    Crosshair, Text, VerticalText, Alias, Copy, Move, NoDrop, NotAllowed,
    Grab, Grabbing, AllScroll, ResizeHorizontal, ResizeNeSw, ResizeNwSe,
    ResizeVertical, ResizeEast, ResizeSouthEast, ResizeSouth, ResizeSouthWest,
    ResizeWest, ResizeNorthWest, ResizeNorth, ResizeNorthEast, ResizeColumn,
    ResizeRow, ZoomIn, ZoomOut,
}
