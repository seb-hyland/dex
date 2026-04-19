use crate::node::NodeVariant;
use crate::node::view::ResizeDir;
use crate::prelude::*;
use crate::{
    node::{
        DrawContext, LayoutContext, NodeDynamics, NodeInitialization,
        view::{HeadlessWindow, Window},
    },
    text::BackingValue,
};

use std::f64;

use eframe::egui::{Frame, TextEdit, TextStyle};
use egui::{Align, FontFamily, FontId, Id, Layout};

// Text Node ----------

#[derive(Clone)]
pub struct TextPayload {
    pub text: Buffer<String>,
    bold: bool,
    italic: bool,
    alignment: Align,
    size: f32,
    view: HeadlessWindow,
}

action! {
    Set { idx: NodeIdx, v: String }
        does(ctx) {
            ctx.unwrap_mut_with(idx, NodeVariant::try_as_text_mut)
                .text
                .set(v);
        }
}

action! {
    FlipBold { idx: NodeIdx }
        does(ctx) {
            let node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_text_mut);
            node.bold = !node.bold;
        }
}

action! {
    FlipItalic { idx: NodeIdx }
        does(ctx) {
            let node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_text_mut);
            node.italic = !node.italic;
        }
}

action! {
    SetAlignment { idx: NodeIdx, align: Align }
        does(ctx) {
            let node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_text_mut);
            node.alignment = align;
        }
}

action! {
    SetSize { idx: NodeIdx, size: f32 }
        does(ctx) {
            let node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_text_mut);
            node.size = size;
        }
}

impl NodeInitialization for TextPayload {
    type Origin = String;

    fn init_from(text: Self::Origin, idx: u32) -> Self {
        Self {
            text: Buffer::new(text, Id::new(idx).with("value")),

            bold: false,
            italic: false,
            alignment: Align::Min,
            size: 16.0,

            view: HeadlessWindow::default(),
        }
    }
}

impl TextPayload {
    fn context_menu(&self, idx: NodeIdx, ui: &mut Ui, actions: &mut Actions) {
        ui.label("Text Settings");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Align:");
            if ui
                .selectable_label(self.alignment == Align::Min, "Left")
                .clicked()
            {
                actions.push(SetAlignment {
                    idx,
                    align: Align::Min,
                });
            };
            if ui
                .selectable_label(self.alignment == Align::Center, "Center")
                .clicked()
            {
                actions.push(SetAlignment {
                    idx,
                    align: Align::Center,
                });
            };
            if ui
                .selectable_label(self.alignment == Align::Max, "Right")
                .clicked()
            {
                actions.push(SetAlignment {
                    idx,
                    align: Align::Max,
                });
            };
        });
        if ui.selectable_label(self.bold, "Bold").clicked() {
            actions.push(FlipBold { idx });
        };
        if ui.selectable_label(self.italic, "Italic").clicked() {
            actions.push(FlipItalic { idx });
        };
        ui.horizontal(|ui| {
            ui.label("Size:");
            let mut size = self.size;
            ui.add(egui::Slider::new(&mut size, 4.0..=300.0).suffix("pt"));
            if size != self.size {
                actions.push(SetSize { idx, size });
            }
        });
    }
}

impl NodeDynamics for TextPayload {
    fn step(&self, ctx: &mut DrawContext<'_>) {
        self.text
            .resolve_pending_actions(ctx.ui, ctx.action_queue, |v| Set { idx: ctx.index, v });
    }

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        let idx = ctx.index;

        self.view.show(ctx, |ui, actions| {
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
                let editor = self.text.show(|text, id| {
                    TextEdit::multiline(text)
                        .id(id)
                        .background_color(Color32::TRANSPARENT)
                        .frame(Frame::NONE)
                        .clip_text(false)
                        .desired_rows(0)
                        .desired_width(0.0)
                        .hint_text("...")
                        .layouter(&mut Window::wrapping_layouter(
                            Some(font.clone()),
                            ctx.theme.text,
                            self.alignment,
                            ui.available_width(),
                        ))
                        .show(ui)
                });

                editor.response.context_menu(|ui| {
                    self.context_menu(idx, ui, actions);
                });
                editor.text_clip_rect
            };

            ui.with_layout(Layout::top_down(alignment), inner).inner
        });
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.size().0
    }
}

// Numeric Node ----------
pub trait Numeric: Default + Copy + Sized + BackingValue {
    fn set_action(self, idx: NodeIdx) -> Box<dyn Action>;
}

action! {
    SetFloat { idx: NodeIdx, val: f64 }
        does(ctx) {
            ctx.unwrap_mut_with(idx, NodeVariant::try_as_float_mut).buf.set(val);
        }
}

impl Numeric for f64 {
    fn set_action(self, idx: NodeIdx) -> Box<dyn Action> {
        Box::new(SetFloat { idx, val: self })
    }
}

action! {
    SetInteger { idx: NodeIdx, val: i32 }
        does(ctx) {
            ctx.unwrap_mut_with(idx, NodeVariant::try_as_integer_mut).buf.set(val);
        }
}

impl Numeric for i32 {
    fn set_action(self, idx: NodeIdx) -> Box<dyn Action> {
        Box::new(SetInteger { idx, val: self })
    }
}

#[derive(Clone)]
pub struct NumericPayload<N: Numeric> {
    view: HeadlessWindow,
    buf: Buffer<N>,
}

impl<N: Numeric> NumericPayload<N> {
    pub fn val(&self) -> N {
        *self.buf.backing_value()
    }
}

impl<N: Numeric> NodeInitialization for NumericPayload<N> {
    type Origin = N;

    fn init_from(num: Self::Origin, idx: u32) -> Self {
        Self {
            buf: Buffer::new(num, Id::new(idx).with("value")),
            view: HeadlessWindow::default().auto_width(),
        }
    }
}

impl<N: Numeric> NodeDynamics for NumericPayload<N> {
    fn step(&self, ctx: &mut DrawContext<'_>) {
        self.buf
            .resolve_pending_actions(ctx.ui, ctx.action_queue, |v| {
                Numeric::set_action(v, ctx.index)
            });
    }

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        self.view.show(ctx, |ui, _actions| {
            let text_edit = self.buf.show(|num, id| {
                TextEdit::singleline(num)
                    .id(id)
                    .clip_text(false)
                    .desired_width(0.0)
                    .background_color(Color32::TRANSPARENT)
                    .font(TextStyle::Heading)
                    .frame(Frame::NONE)
                    .show(ui)
            });

            text_edit.text_clip_rect
        });
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.size().0
    }
}
