use eframe::egui::{Color32, Context, Shadow, Stroke, Visuals};
use std::sync::LazyLock;

pub static LIGHT_THEME: LazyLock<Visuals> = LazyLock::new(|| {
    let mut vis = Visuals::light();
    vis.override_text_color = Some(Color32::BLACK);
    vis.faint_bg_color = Color32::LIGHT_GRAY;
    vis.window_stroke = Stroke {
        width: 2.0,
        color: Color32::GRAY,
    };
    vis.window_shadow = Shadow {
        offset: [8; 2],
        blur: 20,
        spread: 10,
        color: vis.window_stroke.color.gamma_multiply(0.6),
    };

    vis
});

pub struct Theme {
    pub text: Color32,
    pub background: Color32,
    pub border: Stroke,
    pub faint_background: Color32,
    pub shadow: Shadow,
}

impl From<&Visuals> for Theme {
    fn from(vis: &Visuals) -> Self {
        Self {
            text: vis.override_text_color.unwrap(),
            background: vis.window_fill,
            border: vis.window_stroke,
            faint_background: vis.faint_bg_color,
            shadow: vis.window_shadow,
        }
    }
}

impl From<&Context> for Theme {
    fn from(ctx: &Context) -> Self {
        Self::from(&ctx.style().visuals)
    }
}
