use crate::node::view::Window;
use crate::node::{DrawContext, NodeDynamics, view::HeadlessWindow};
use crate::prelude::*;

use std::f64;

use eframe::egui::{Frame, TextEdit, TextStyle};
use egui::{Align, FontFamily, FontId, Layout};

#[derive(Serialize, Deserialize)]
pub struct TextPayload {
    view: HeadlessWindow,
    pub text: String,
    bold: bool,
    italic: bool,
    alignment: Align,
    size: f32,
}

impl Default for TextPayload {
    fn default() -> Self {
        Self {
            view: HeadlessWindow::default(),
            text: String::new(),
            bold: false,
            italic: false,
            alignment: Align::Min,
            size: 16.0,
        }
    }
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
            let font = {
                let font_name = match (self.bold, self.italic) {
                    (false, false) => "inter",
                    (true, false) => "inter_bold",
                    (false, true) => "inter_italic",
                    (true, true) => "inter_bold_italic",
                };
                FontId::new(self.size, FontFamily::Name(font_name.into()))
            };

            let alignment = self.alignment;
            let inner = |ui: &mut Ui| {
                let editor = TextEdit::singleline(&mut self.text)
                    .background_color(Color32::TRANSPARENT)
                    .frame(Frame::NONE)
                    .clip_text(false)
                    .desired_width(0.0)
                    .hint_text("...")
                    .layouter(&mut Window::wrapping_layouter(
                        Some(font),
                        ctx.theme.text,
                        self.alignment,
                        ui.available_width(),
                    ))
                    .show(ui);

                editor.response.context_menu(|ui| {
                    ui.label("Text Settings");
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Align:");
                        ui.selectable_value(&mut self.alignment, Align::Min, "Left");
                        ui.selectable_value(&mut self.alignment, Align::Center, "Center");
                        ui.selectable_value(&mut self.alignment, Align::Max, "Right");
                    });
                    ui.checkbox(&mut self.bold, "Bold");
                    ui.checkbox(&mut self.italic, "Italic");
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        ui.add(egui::Slider::new(&mut self.size, 4.0..=300.0).suffix("pt"));
                    });
                });
                editor.text_clip_rect
            };

            ui.with_layout(Layout::top_down(alignment), inner).inner
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

#[derive(Serialize, Deserialize)]
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
