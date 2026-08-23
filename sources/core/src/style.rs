use egui::{Color32, FontFamily, FontId, StrokeKind as EguiStrokeKind};

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
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const GRAY: Self = Self {
        r: 160,
        g: 160,
        b: 160,
        a: 255,
    };
}

#[utils::dynamic_methods]
impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub fn gray(level: u8) -> Self {
        Self {
            r: level,
            g: level,
            b: level,
            a: 255,
        }
    }
    pub fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
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
    pub const NONE: Self = Self {
        width: 0.0,
        color: Color::TRANSPARENT,
    };
}

#[utils::dynamic_methods]
impl Stroke {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
    pub fn none() -> Self {
        Self {
            width: 0.0,
            color: Color::transparent(),
        }
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

/// A font: a size, and whether it is monospaced. Named font families are not
/// yet exposed to scripts.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct Font {
    pub size: f32,
    pub monospace: bool,
}

#[utils::dynamic_methods]
impl Font {
    pub fn proportional(size: f32) -> Self {
        Self {
            size,
            monospace: false,
        }
    }
    pub fn monospaced(size: f32) -> Self {
        Self {
            size,
            monospace: true,
        }
    }
}

impl From<Font> for FontId {
    fn from(f: Font) -> Self {
        let family = if f.monospace {
            FontFamily::Monospace
        } else {
            FontFamily::Proportional
        };
        FontId::new(f.size, family)
    }
}

impl From<FontId> for Font {
    fn from(f: FontId) -> Self {
        Self {
            size: f.size,
            monospace: matches!(f.family, FontFamily::Monospace),
        }
    }
}
