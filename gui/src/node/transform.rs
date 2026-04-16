use crate::node::Node;
use crate::prelude::*;
use crate::{
    canvas::CanvasCommand,
    node::{
        DrawContext, NodeDynamics, NodeVariant,
        primitives::{NumericPayload, TextPayload},
        view::Window,
    },
    registry::{Registry, RegistryItemInner},
};

use eframe::egui::{Button, ComboBox, Frame, Sense, Stroke, StrokeKind, TextEdit, TextStyle};
use egui::FontId;
use egui_code_editor::{CodeEditor, ColorTheme, Completer, Syntax};
use lib::compute::{self, TransformValue, python};
use petgraph::visit::EdgeRef;

use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
pub struct TransformPayload {
    unit_arg_name: String,
    pub args: Vec<TransformArg>,
    pub last_color: Option<Color32>,
    active_lang: TransformLang,
    #[serde(skip, default = "default_completer")]
    completer: Completer,
    python: String,
    view: Window,
    error: String,
}

impl Default for TransformPayload {
    fn default() -> Self {
        Self {
            unit_arg_name: "My Transform".to_owned(),
            args: Vec::new(),
            active_lang: TransformLang::Python,
            completer: Completer::new_with_syntax(&Syntax::python()),
            python: r#"
def transform():
    # Add code here!"#
                .trim_start()
                .to_owned(),
            view: Window::default(),
            last_color: None,
            error: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct TransformArg {
    label: String,
    arg_name: String,
    pub node: Option<NodeIdx>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum TransformLang {
    Python,
}

#[derive(Serialize, Deserialize)]
pub struct TransformArgPayload {
    pub cached_rect: Rect,
    pub color: Color32,
}

impl NodeDynamics for TransformArgPayload {
    /// This panics. Drawing should be handled by the [`TransformPayload`] that this arg belongs to.
    fn draw(&mut self, _ctx: &mut DrawContext<'_>) {
        unreachable!("Never directly call NodeDynamics impls for TransformArgPayload")
    }

    fn rect(&self, _ctx: &mut DrawContext<'_>) -> Rect {
        self.cached_rect
    }

    fn override_edge_color(&self) -> Option<Color32> {
        Some(self.color)
    }
}

impl NodeDynamics for TransformPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let self_rect = self.rect(ctx);
        let cur_index = ctx.index;
        let graph = ctx.graph_ref;
        let default_border = ctx.theme.border;

        let mut warnings = Vec::new();
        let mut duplicate_names = HashSet::new();
        let mut seen_arg_names = HashSet::new();

        for arg in self.args.iter() {
            let arg_name = &arg.arg_name;
            if !seen_arg_names.insert(arg_name) {
                duplicate_names.insert(arg_name);
            }
            if arg_name.contains(|c: char| c.is_whitespace()) {
                warnings.push(format!("{} may not be a valid identifier", arg_name));
            }
            if graph.edge_count(arg.node.unwrap()) == 0 {
                warnings.push(format!("{} is not connected to a value", arg_name));
            }
        }
        for name in duplicate_names {
            warnings.push(format!("Argument name {name} is used multiple times"));
        }

        let warnings_text = warnings
            .into_iter()
            .map(|s| "⚠ ".to_owned() + &s)
            .reduce(|item1, item2| item1 + "\n" + &item2);

        let mut execute = false;
        let mut commands = Vec::new();
        self.view.show(
            ctx,
            ctx.theme.background,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let hovered = ui.rect_contains_pointer(self_rect);
                    let default_x_spacing = ui.spacing().item_spacing.x;

                    if self.args.is_empty() {
                        TextEdit::singleline(&mut self.unit_arg_name)
                            .background_color(Color32::TRANSPARENT)
                            .frame(Frame::NONE)
                            .clip_text(false)
                            .desired_width(0.0)
                            // .layouter(&mut Window::wrapping_layouter(
                            //     None,
                            //     ctx.theme.text,
                            //     Align::Min,
                            //     ui.available_width(),
                            // ))
                            .show(ui);
                    } else {
                        self.args.retain_mut(|arg| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            TextEdit::singleline(&mut arg.label)
                                .background_color(Color32::TRANSPARENT)
                                .frame(Frame::NONE)
                                .clip_text(false)
                                .desired_width(0.0)
                                // .layouter(&mut Window::wrapping_layouter(
                                //     None,
                                //     ctx.theme.text,
                                //     Align::Min,
                                //     ui.available_width(),
                                // ))
                                .show(ui);

                            ui.spacing_mut().item_spacing.x = default_x_spacing;
                            ui.label(":");

                            ui.spacing_mut().item_spacing.x = 0.0;
                            let arg_node_idx = arg.node.unwrap();
                            let is_connected = graph.edge_count(arg_node_idx) != 0;
                            let arg_resp = TextEdit::singleline(&mut arg.arg_name)
                                .background_color(Color32::TRANSPARENT)
                                .frame(Frame::new().corner_radius(ctx.theme.corner_radius).stroke(
                                    Stroke {
                                        color: if is_connected {
                                            graph
                                                .get(arg.node.unwrap())
                                                .unwrap()
                                                .variant
                                                .override_edge_color()
                                                .unwrap()
                                        } else {
                                            default_border.color
                                        },
                                        ..default_border
                                    },
                                ))
                                .clip_text(false)
                                .desired_width(0.0)
                                // .layouter(&mut Window::wrapping_layouter(
                                //     None,
                                //     ctx.theme.text,
                                //     Align::Min,
                                //     ui.available_width(),
                                // ))
                                .show(ui);

                            ui.spacing_mut().item_spacing.x = default_x_spacing;
                            let retained = !ui.add_visible(hovered, Button::new("x")).clicked();

                            commands.push(CanvasCommand::UpdateTransformArgLocation {
                                idx: arg.node.unwrap(),
                                new_rect: arg_resp.response.rect,
                            });

                            retained
                        });
                    }

                    if ui.add_visible(hovered, Button::new("+")).clicked() {
                        let label = if self.args.is_empty() {
                            self.unit_arg_name.clone()
                        } else {
                            "Action on".to_owned()
                        };
                        self.args.push(TransformArg {
                            label,
                            arg_name: "thisValue".to_owned(),
                            node: None,
                        });
                        commands.push(CanvasCommand::AddTransformArg { origin: cur_index });
                    }
                });
            },
            |ui| {
                execute = ui
                    .horizontal(|ui| {
                        ComboBox::from_id_salt(ui.id().with("transform_language"))
                            .selected_text(format!("{:?}", self.active_lang))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.active_lang,
                                    TransformLang::Python,
                                    "Python",
                                );
                            });

                        ui.button("Execute transform").clicked()
                    })
                    .inner;

                ui.colored_label(Color32::BROWN, warnings_text.unwrap_or_default());
                ui.colored_label(Color32::RED, &self.error);

                let completer = &mut self.completer;

                ui.add_sized(ui.available_size(), |ui: &mut Ui| {
                    CodeEditor::default()
                        .id_source(ui.id().with("code_editor").value().to_string())
                        .with_syntax(match self.active_lang {
                            TransformLang::Python => Syntax::python(),
                        })
                        .with_fontsize(ui.text_style_height(&TextStyle::Monospace))
                        .with_theme(ColorTheme::AYU)
                        .show_with_completer(
                            ui,
                            match self.active_lang {
                                TransformLang::Python => &mut self.python,
                            },
                            completer,
                        )
                        .response
                        .response
                });
            },
        );

        ctx.command_queue.extend(commands);
        if execute {
            let transform_result = python::apply_transform(
                &self.python,
                self.args
                    .iter()
                    .map(|arg| {
                        let arg_idx = graph.get_edge(arg.node.unwrap()).unwrap().target();
                        let arg_node = graph.get(arg_idx).unwrap();
                        argument_from_node(arg_node, arg.arg_name.clone(), ctx.registry)
                    })
                    .collect(),
                Some(
                    #[cfg(target_os = "macos")]
                    "/Users/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages",
                    #[cfg(target_os = "linux")]
                    "/home/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages",
                ),
            );
            match transform_result {
                Ok(v) => {
                    self.error.clear();
                    ctx.command_queue.push(command_from_transform(v, ctx.index));
                }
                Err(e) => self.error = format!("{e:?}"),
            }
        }
    }

    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }

    fn edge_target(&self, ctx: &mut DrawContext<'_>) -> Option<(NodeIdx, bool)> {
        for arg in self.args.iter() {
            let node_idx = arg.node.unwrap();
            let node = ctx.graph_ref.get(node_idx).unwrap();
            let NodeVariant::TransformArg(arg) = &node.variant else {
                unreachable!()
            };
            let rect = arg.cached_rect;

            let interaction =
                ctx.ui
                    .interact(rect, ctx.id.with(node_idx), Sense::HOVER | Sense::CLICK);
            if interaction.clicked() {
                return Some((node_idx, true));
            } else if interaction.hovered() {
                ctx.ui.painter().rect(
                    rect,
                    ctx.theme.corner_radius,
                    arg.color.gamma_multiply(0.25),
                    Stroke {
                        color: arg.color,
                        ..ctx.theme.border
                    },
                    StrokeKind::Middle,
                );
                return Some((node_idx, false));
            }
        }

        None
    }
}

fn argument_from_node(
    node: &Node,
    arg_name: String,
    registry: &Registry,
) -> compute::TransformArgument {
    match &node.variant {
        NodeVariant::Dataframe(df) => {
            let registry_item = registry.get(df.data_ref).unwrap();
            match &registry_item.borrow().inner {
                RegistryItemInner::Dataframe { data, .. } => compute::TransformArgument {
                    name: arg_name,
                    value: TransformValue::Dataframe(data.clone()),
                },
            }
        }
        NodeVariant::Float(f) => compute::TransformArgument {
            name: arg_name,
            value: TransformValue::Float(f.num),
        },
        NodeVariant::Integer(i) => compute::TransformArgument {
            name: arg_name,
            value: TransformValue::Int(i.num),
        },
        NodeVariant::Text(str) => compute::TransformArgument {
            name: arg_name,
            value: TransformValue::String(str.text.clone()),
        },
        _ => unimplemented!(),
    }
}

fn command_from_transform(arg: compute::TransformValue, origin: NodeIdx) -> CanvasCommand {
    match arg {
        compute::TransformValue::Dataframe(df) => CanvasCommand::AddDataframe {
            origin,
            df,
            name: "Transform output".to_owned(),
        },
        compute::TransformValue::Int(i) => CanvasCommand::AddNode {
            origin,
            node: NodeVariant::Integer(NumericPayload::new(i)),
        },
        compute::TransformValue::Float(f) => CanvasCommand::AddNode {
            origin,
            node: NodeVariant::Float(NumericPayload::new(f)),
        },
        compute::TransformValue::String(s) => CanvasCommand::AddNode {
            origin,
            node: NodeVariant::Text(TextPayload::new(s)),
        },
    }
}

fn default_completer() -> Completer {
    Completer::new_with_syntax(&Syntax::python())
}
