use std::sync::LazyLock;

use eframe::egui::{Color32, Context, Stroke, Visuals};

pub static LIGHT_THEME: LazyLock<Visuals> = LazyLock::new(|| {
    let mut vis = Visuals::light();
    vis.override_text_color = Some(Color32::BLACK);
    vis.faint_bg_color = Color32::LIGHT_GRAY;
    vis
});

pub struct Theme {
    pub text: Color32,
    pub background: Color32,
    pub border: Stroke,
    pub faint_background: Color32,
}

impl From<&Visuals> for Theme {
    fn from(vis: &Visuals) -> Self {
        Self {
            text: vis.override_text_color.unwrap(),
            background: vis.window_fill,
            border: vis.window_stroke,
            faint_background: vis.faint_bg_color,
        }
    }
}

impl From<&Context> for Theme {
    fn from(ctx: &Context) -> Self {
        Self::from(&ctx.style().visuals)
    }
}
