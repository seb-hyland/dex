use crate::prelude::*;
use eframe::egui::{CornerRadius, Shadow, Stroke, Visuals};

pub static LIGHT_THEME: Theme = Theme {
    text: Color32::BLACK,
    faint_background: Color32::LIGHT_GRAY,
    background: Color32::WHITE,
    border: Stroke {
        width: 1.0,
        color: Color32::GRAY,
    },
    hover_overlay: Theme::const_gamma_multiply(Color32::LIGHT_BLUE, 0.3),
    shadow: Shadow {
        offset: [8; 2],
        blur: 20,
        spread: 10,
        color: Theme::const_gamma_multiply(Color32::GRAY, 0.6),
    },
    corner_radius: CornerRadius::same(2),
};

#[derive(Clone, Copy)]
pub struct Theme {
    pub text: Color32,
    pub faint_background: Color32,
    pub background: Color32,
    pub border: Stroke,
    pub hover_overlay: Color32,
    pub shadow: Shadow,
    pub corner_radius: CornerRadius,
}

impl Theme {
    pub const COLOR_PALETTE: [Color32; 6] = [
        Color32::GREEN,
        Color32::MAGENTA,
        Color32::RED,
        Color32::YELLOW,
        Color32::BLUE,
        Color32::BROWN,
    ];

    pub fn palette_next(current_color: Color32) -> Color32 {
        let palette = Self::COLOR_PALETTE;
        let idx = palette
            .into_iter()
            .position(|c| c == current_color)
            .unwrap();
        palette[(idx + 1) % palette.len()]
    }

    pub const fn const_gamma_multiply(color: Color32, factor: f32) -> Color32 {
        let [r, g, b, a] = color.to_array();
        Color32::from_rgba_premultiplied(
            (r as f32 * factor + 0.5) as u8,
            (g as f32 * factor + 0.5) as u8,
            (b as f32 * factor + 0.5) as u8,
            (a as f32 * factor + 0.5) as u8,
        )
    }
}

impl From<Theme> for Visuals {
    fn from(theme: Theme) -> Self {
        Self {
            override_text_color: Some(theme.text),
            faint_bg_color: theme.faint_background,
            window_fill: theme.background,
            window_stroke: theme.border,
            window_shadow: theme.shadow,
            window_corner_radius: theme.corner_radius,
            ..Visuals::light()
        }
    }
}
