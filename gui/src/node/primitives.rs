use crate::node::view::Window;
use crate::node::{DrawContext, NodeDynamics, view::HeadlessWindow};
use crate::prelude::*;

use std::f32;
use std::fmt::Display;

use eframe::egui::{Align, DragValue, Frame, Layout, TextEdit, TextStyle};

#[derive(Default)]
pub struct TextPayload {
    view: HeadlessWindow,
    text: String,
}

impl NodeDynamics for TextPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        self.view.show(ctx, |ui| {
            let text_rect = TextEdit::singleline(&mut self.text)
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
                .text_clip_rect;

            (Some(text_rect.height()), None)
        });
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rect(ctx).0
    }
}

pub trait Numeric: Default + Display + Copy + Sized {
    fn parse(s: &str) -> Option<Self>;
}

impl Numeric for f32 {
    fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl Numeric for i32 {
    fn parse(s: &str) -> Option<Self> {
        if let Ok(v) = s.parse::<i32>() {
            Some(v)
        } else if let Ok(v) = s.parse::<f32>() {
            Some(v.round() as i32)
        } else {
            None
        }
    }
}

pub struct NumericPayload<N: Numeric> {
    view: HeadlessWindow,
    str: String,
    num: N,
}

impl<N: Numeric> Default for NumericPayload<N> {
    fn default() -> Self {
        let default_num = N::default();
        Self {
            view: HeadlessWindow::default(),
            num: default_num,
            str: default_num.to_string(),
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
                    self.str = parsed_value.to_string();
                } else {
                    self.str = self.num.to_string();
                }
            }

            let text_rect = text_edit.text_clip_rect;
            (Some(text_rect.height()), Some(text_rect.width()))
        });
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rect(ctx).0
    }
}
