use std::ptr::NonNull;

use crate::canvas::CanvasCommand;
use crate::node::{DrawContext, NodeDynamics, view::Window};
use crate::prelude::*;

use eframe::egui::{Button, ComboBox, Frame, TextEdit, TextStyle};

pub struct TransformPayload {
    pub args: Vec<TransformArg>,
    active_lang: TransformLang,
    python: String,
    view: Window,
}

impl Default for TransformPayload {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            active_lang: TransformLang::Python,
            python: String::new(),
            view: Window::default(),
        }
    }
}

pub struct TransformArg {
    label: String,
    arg_name: String,
    pub node: Option<NodeIdx>,
}

#[derive(Debug, PartialEq)]
enum TransformLang {
    Python,
}

impl NodeDynamics for TransformPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let self_rect = self.rect(ctx);
        let mut command_queue = NonNull::from_mut(ctx.command_queue);
        let cur_index = ctx.index;

        self.view.show(
            ctx,
            |ui| {
                ui.allocate_ui(ui.available_size(), |ui| {
                    let hovered = ui.rect_contains_pointer(self_rect);
                    let default_x_spacing = ui.spacing().item_spacing.x;

                    self.args.retain_mut(|arg| {
                        TextEdit::singleline(&mut arg.label)
                            .background_color(Color32::TRANSPARENT)
                            .font(TextStyle::Heading)
                            .frame(Frame::NONE)
                            .clip_text(false)
                            .desired_width(0.0)
                            .layouter(&mut Window::wrapping_layouter(
                                ctx.theme.text,
                                ui.available_width(),
                                ":",
                            ))
                            .show(ui);

                        ui.spacing_mut().item_spacing.x = 0.0;
                        TextEdit::singleline(&mut arg.arg_name)
                            .background_color(Color32::TRANSPARENT)
                            .font(TextStyle::Heading)
                            .frame(
                                Frame::new()
                                    .corner_radius(ctx.theme.corner_radius)
                                    .stroke(ctx.theme.border),
                            )
                            .clip_text(false)
                            .desired_width(0.0)
                            .layouter(&mut Window::wrapping_layouter(
                                ctx.theme.text,
                                ui.available_width(),
                                "",
                            ))
                            .show(ui);
                        let retained = !ui.add_visible(hovered, Button::new("x")).clicked();

                        ui.spacing_mut().item_spacing.x = default_x_spacing;
                        retained
                    });

                    if ui.add_visible(hovered, Button::new("+")).clicked() {
                        self.args.push(TransformArg {
                            label: "An argument".to_owned(),
                            arg_name: "Unnamed arg".to_owned(),
                            node: None,
                        });
                        // A bad enough hack to probably make the front page of r/rust
                        // TODO: fix it
                        unsafe {
                            command_queue
                                .as_mut()
                                .push(CanvasCommand::AddTransformArg { origin: cur_index });
                        }
                    }
                })
                .response
                .rect
            },
            |ui| {
                ComboBox::from_label("Transform language")
                    .selected_text(format!("{:?}", self.active_lang))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.active_lang, TransformLang::Python, "Python");
                    });
                TextEdit::multiline(match self.active_lang {
                    TransformLang::Python => &mut self.python,
                })
                .show(ui);
            },
        );
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }
}

pub struct TransformArgPayload;

impl NodeDynamics for TransformArgPayload {
    /// This panics. Drawing should be handled by the [`TransformPayload`] that this arg belongs to.
    fn draw(&mut self, _ctx: &mut DrawContext<'_>) {
        unreachable!("Never directly call NodeDynamics impls for TransformArgPayload")
    }

    /// This panics. Drawing should be handled by the [`TransformPayload`] that this arg belongs to.
    fn rect(&self, _ctx: &mut DrawContext<'_>) -> Rect {
        unreachable!("Never directly call NodeDynamics impls for TransformArgPayload")
    }
}
