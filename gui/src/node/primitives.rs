use crate::node::view::Window;
use crate::node::{DrawContext, NodeDynamics, view::HeadlessWindow};
use crate::prelude::*;

use std::f64;

use eframe::egui::{Frame, TextEdit, TextStyle};

#[derive(Default)]
pub struct TextPayload {
    view: HeadlessWindow,
    pub text: String,
}

impl TextPayload {
    pub fn new(text: String) -> Self {
        Self {
            text,
            ..Default::default()
        }
    }
}

impl NodeDynamics for TextPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        self.view.show(ctx, |ui| {
            TextEdit::singleline(&mut self.text)
                .background_color(Color32::TRANSPARENT)
                .font(TextStyle::Heading)
                .frame(Frame::NONE)
                .clip_text(false)
                .desired_width(0.0)
                .hint_text("...")
                .layouter(&mut Window::wrapping_layouter(
                    ctx.theme.text,
                    ui.available_width(),
                    "",
                ))
                .show(ui)
                .text_clip_rect
        });
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rect(ctx).0
    }
}

pub trait Numeric: Default + Copy + Sized {
    fn parse(s: &str) -> Option<Self>;

    fn format(self) -> String;
}

impl Numeric for f64 {
    fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    fn format(self) -> String {
        // Displays x.x to differentiate from integers
        format!("{self:?}")
    }
}

impl Numeric for i32 {
    fn parse(s: &str) -> Option<Self> {
        if let Ok(v) = s.parse::<i32>() {
            Some(v)
        } else if let Ok(v) = s.parse::<f64>() {
            Some(v.round() as i32)
        } else {
            None
        }
    }

    fn format(self) -> String {
        format!("{self}")
    }
}

pub struct NumericPayload<N: Numeric> {
    view: HeadlessWindow,
    str: String,
    pub num: N,
}

impl<N: Numeric> Default for NumericPayload<N> {
    fn default() -> Self {
        let default_num = N::default();
        Self {
            view: HeadlessWindow::default().auto_width(),
            num: default_num,
            str: default_num.format(),
        }
    }
}

impl<N: Numeric> NumericPayload<N> {
    pub fn new(num: N) -> Self {
        Self {
            num,
            str: num.format(),
            ..Default::default()
        }
    }
}

impl<N: Numeric> NodeDynamics for NumericPayload<N> {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        self.view.show(ctx, |ui| {
            let text_edit = TextEdit::singleline(&mut self.str)
                .clip_text(false)
                .desired_width(0.0)
                .background_color(Color32::TRANSPARENT)
                .font(TextStyle::Heading)
                .frame(Frame::NONE)
                .show(ui);

            if text_edit.response.lost_focus() {
                let parse_result = Numeric::parse(&self.str);
                if let Some(parsed_value) = parse_result {
                    self.num = parsed_value;
                    self.str = parsed_value.format();
                } else {
                    self.str = self.num.format();
                }
            }

            text_edit.text_clip_rect
        });
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rect(ctx).0
    }
}
