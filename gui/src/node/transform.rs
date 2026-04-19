use crate::canvas::{AddConnectedDataframe, AddConnectedNode, CanvasGraph};
use crate::node::view::ResizeDir;
use crate::node::{LayoutContext, Node, NodeInitialization};
use crate::prelude::*;
use crate::theme::Theme;
use crate::{
    node::{
        DrawContext, NodeDynamics, NodeVariant,
        primitives::{NumericPayload, TextPayload},
        view::Window,
    },
    registry::{Registry, RegistryItemInner},
};

use eframe::egui::{Button, Frame, Sense, Stroke, StrokeKind, TextStyle};
use egui::{Id, UiBuilder};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use lib::compute::{self, TransformErr, TransformValue};
use petgraph::visit::EdgeRef;

use std::collections::HashSet;
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct TransformPayload {
    unit_arg_name: Buffer<String>,
    pub args: TransformArgs,

    active_lang: TransformLang,
    python: Buffer<String>,
    error: Transient<String>,

    view: Window,
}

impl TransformPayload {
    fn warnings(&self, graph: &CanvasGraph) -> Option<String> {
        let mut warnings = Vec::new();
        let mut duplicate_names = HashSet::new();
        let mut seen_arg_names = HashSet::new();

        for arg in self.args.iter() {
            let arg_name = &arg.name;
            if !seen_arg_names.insert(arg_name.backing_value()) {
                duplicate_names.insert(arg_name.backing_value());
            }
            if arg_name
                .backing_value()
                .contains(|c: char| c.is_whitespace())
            {
                warnings.push(format!(
                    "{} may not be a valid identifier",
                    arg_name.backing_value()
                ));
            }
            if graph.node_edge_count(arg.node.unwrap()) == 0 {
                warnings.push(format!(
                    "{} is not connected to a value",
                    arg_name.backing_value()
                ));
            }
        }
        for name in duplicate_names {
            warnings.push(format!("Argument name {name} is used multiple times"));
        }

        warnings
            .into_iter()
            .map(|s| "⚠ ".to_owned() + &s)
            .reduce(|item1, item2| item1 + "\n" + &item2)
    }

    fn execute(
        &self,
        graph: &CanvasGraph,
        registry: &Registry,
    ) -> Result<TransformValue, TransformErr> {
        let args: Vec<_> = self
            .args
            .iter()
            .map(|arg| {
                let arg_idx = graph.get_first_edge(arg.node.unwrap()).target();
                let arg_node = graph.get_node(arg_idx);
                argument_from_node(arg_node, arg.name.backing_value().clone(), registry)
            })
            .collect();
        match self.active_lang {
            TransformLang::Python => lib::compute::python::apply_transform(
                self.python.backing_value(),
                args,
                Some(
                    #[cfg(target_os = "macos")]
                    "/Users/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages",
                    #[cfg(target_os = "linux")]
                    "/home/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages",
                ),
            ),
        }
    }
}

impl NodeDynamics for TransformPayload {
    fn step(&self, ctx: &mut DrawContext<'_>) {
        let ui = &mut ctx.ui;
        let idx = ctx.index;

        action! {
            SetUnitArgName { idx: NodeIdx, val: String }
                does(ctx) {
                    let transform_node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_transform_mut);
                    transform_node.unit_arg_name.set(val);
                }
        }
        action! {
            SetPython { idx: NodeIdx, val: String }
                does(ctx) {
                    let transform_node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_transform_mut);
                    transform_node.python.set(val);
                }
        }
        action! {
            SetArgName { idx: NodeIdx, arg_idx: usize, val: String }
                does(ctx) {
                    let transform_node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_transform_mut);
                    transform_node.args.get_mut(arg_idx).unwrap().name.set(val);
                }
        }
        action! {
            SetArgLabel { idx: NodeIdx, arg_idx: usize, val: String }
                does(ctx) {
                    let transform_node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_transform_mut);
                    transform_node.args.get_mut(arg_idx).unwrap().label.set(val);
                }
        }

        if ui.memory(|mem| mem.has_focus(self.python.id)) {
            println!("Focused")
        } else {
            println!("Not focused")
        }

        self.unit_arg_name
            .resolve_pending_actions(ui, ctx.action_queue, |s| SetUnitArgName { idx, val: s });
        for (i, arg) in self.args.iter().enumerate() {
            arg.name
                .resolve_pending_actions(ui, ctx.action_queue, |s| SetArgName {
                    idx,
                    val: s,
                    arg_idx: i,
                });
            arg.label
                .resolve_pending_actions(ui, ctx.action_queue, |s| SetArgLabel {
                    idx,
                    val: s,
                    arg_idx: i,
                });
        }

        self.python
            .resolve_pending_actions(ui, ctx.action_queue, |s| SetPython { idx, val: s });
    }

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        let bounding_rect = self.rect(ctx.layout, ctx.screen_location);

        let cur_index = ctx.index;
        let graph = ctx.graph;
        let id = ctx.id;
        let default_border = ctx.theme.border;

        let mut should_execute = false;
        self.view.show(
            ctx,
            ctx.theme.background,
            |ui, actions| {
                ui.horizontal_wrapped(|ui| {
                    let hovered = ui.rect_contains_pointer(bounding_rect);
                    let default_x_spacing = ui.spacing().item_spacing.x;

                    if self.args.is_empty() {
                        self.unit_arg_name.show(|buf, id| {
                            ui.editable_label(buf, id);
                        });
                    } else {
                        for (i, arg) in self.args.iter().enumerate() {
                            let  default_spacing =
                                |ui: &mut Ui| ui.spacing_mut().item_spacing.x = default_x_spacing;
                            let  zero_spacing = |ui: &mut Ui|
                                ui.spacing_mut().item_spacing.x = 0.0;

                            zero_spacing(ui);
                            arg.label.show(|s, id| ui.editable_label(s, id));
                            default_spacing(ui)
                            ;

                            ui.label(":");

                            zero_spacing(ui);
                            let arg_node_idx = arg.node.unwrap();
                            let frame_color = if graph.node_edge_count(arg_node_idx) != 0 {
                                graph
                                    .get_node(arg.node.unwrap())
                                    .variant
                                    .override_edge_color()
                                    .unwrap()

                                } else {
                                    default_border.color
                                    };
                            let arg_resp = arg.name.show(|s, id| ui.editable_label_with(s, id, |editor|
                                editor
                                    .frame(Frame::new().corner_radius(ctx.theme.corner_radius).stroke(
                                        Stroke {
                                            color: frame_color,
                                            ..default_border
                                        },
                                    ))
                            ));

                            default_spacing(ui);
                            if ui.add_visible(hovered, Button::new("x")).clicked() {
                                // Remove this node
                                action! {
                                    RemoveArg { node_index: NodeIdx, arg_index: usize }
                                        does(ctx) {
                                            ctx.unwrap_mut_with(node_index, NodeVariant::try_as_transform_mut).args.remove(arg_index);
                                        }
                                }
                                actions.push(RemoveArg { node_index: cur_index, arg_index: i });
                            }

                            graph.get_node(arg_node_idx).variant.try_as_transform_arg_ref().unwrap().cached_rect.set(arg_resp.response.rect);
                        }
                    }

                    if ui.add_visible(hovered, Button::new("+")).clicked() {
                        let label = if self.args.is_empty() {
                            self.unit_arg_name.backing_value().clone()
                        } else {
                            "Action on".to_owned()
                        };

                        action! {
                            AddTransformArg { origin: NodeIdx, id: Id, label: String }
                                does(ctx) {
                                    let color = {
                                        let transform_args =
                                            &mut ctx.unwrap_mut_with(origin, NodeVariant::try_as_transform_mut).args;
                                        transform_args.push(|i| TransformArg {
                                            label: Buffer::new(label, id.with(i).with("label")),
                                            name: Buffer::new("thisValue".to_owned(), id.with(i).with("arg_name")),
                                            node: None
                                        });
                                        transform_args.advance_color()
                                    };

                                    let canvas = ctx.unwrap_active_canvas();
                                    let new_idx =
                                        canvas.add_node_noplacing(NodeVariant::TransformArg(TransformArgPayload {
                                            cached_rect: Transient::from(Rect::ZERO),
                                            color,
                                        }));
                                    ctx.unwrap_mut_with(origin, NodeVariant::try_as_transform_mut)
                                        .args.last_mut().unwrap()
                                        .node = Some(new_idx);
                                }
                        }
                        actions.push(AddTransformArg { origin: cur_index, id, label });
                    }
                });
            },
            |ui, _actions| {
                ui
                    .horizontal(|ui| {

                        should_execute = ui.button("Execute transform").clicked();
                    })
                    ;

                if let Some(warnings) = self.warnings(graph) {
                    ui.colored_label(Color32::BROWN, warnings);
                }
                ui.colored_label(Color32::RED, &*self.error.val());

                let editor = CodeEditor::default()
                    .with_syntax(match self.active_lang {
                        TransformLang::Python => Syntax::python(),
                    })
                    .with_fontsize(ui.text_style_height(&TextStyle::Monospace))
                    .with_theme(ColorTheme::AYU);

                    match self.active_lang {
                        TransformLang::Python => self.python.show(|s, id| {
                            ui.add_sized(ui.available_size(), |ui: &mut Ui| editor.id(id).show(ui, s).response.response)
                    }),
            };
            }
        );

        if should_execute {
            let transform_result = self.execute(graph, ctx.registry);
            match transform_result {
                Ok(v) => {
                    self.error.modify(|s| s.clear());
                    ctx.action_queue.push(command_from_transform(v, ctx.index));
                }
                Err(e) => self.error.set(format!("{e:?}")),
            }
        }
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.sizes().1
    }

    fn edge_target(&self, ctx: &mut DrawContext<'_>) -> Option<(NodeIdx, bool)> {
        for arg in self.args.iter() {
            let node_idx = arg.node.unwrap();
            let node = ctx.graph.get_node(node_idx);
            let NodeVariant::TransformArg(arg) = &node.variant else {
                unreachable!()
            };
            let rect = *arg.cached_rect.val();

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

impl NodeInitialization for TransformPayload {
    type Origin = ();

    fn init_from(_f: Self::Origin, seed: u32) -> Self {
        Self {
            unit_arg_name: Buffer::new(
                "My Transform".to_owned(),
                Id::new(seed).with("unit_arg_name"),
            ),
            args: TransformArgs::default(),
            active_lang: TransformLang::Python,
            python: Buffer::new(
                r#"
    def transform():
        # Add code here!"#
                    .trim_start()
                    .to_owned(),
                Id::new(seed).with("python"),
            ),
            view: Window::default(),
            error: Transient::from(String::new()),
        }
    }
}

#[derive(Clone, Default)]
pub struct TransformArgs {
    args: Vec<TransformArg>,
    stable_index: u32,
    current_color: Option<Color32>,
}

impl TransformArgs {
    pub fn advance_color(&mut self) -> Color32 {
        let color_ref = self.current_color.get_or_insert(Theme::COLOR_PALETTE[0]);

        let current_color = *color_ref;
        *color_ref = Theme::palette_next(current_color);

        current_color
    }

    pub fn push(&mut self, f: impl FnOnce(u32) -> TransformArg) {
        let new_arg = f(self.stable_index);
        self.stable_index += 1;
        self.args.push(new_arg);
    }

    pub fn remove(&mut self, index: usize) {
        self.args.remove(index);
    }
}

impl Deref for TransformArgs {
    type Target = Vec<TransformArg>;

    fn deref(&self) -> &Self::Target {
        &self.args
    }
}

impl DerefMut for TransformArgs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.args
    }
}

#[derive(Clone)]
pub struct TransformArg {
    label: Buffer<String>,
    name: Buffer<String>,
    pub node: Option<NodeIdx>,
}

#[derive(Clone)]
pub struct TransformArgPayload {
    pub cached_rect: Transient<Rect>,
    pub color: Color32,
}

impl NodeDynamics for TransformArgPayload {
    fn step(&self, _ctx: &mut DrawContext<'_>) {}

    /// This panics. Drawing should be handled by the [`TransformPayload`] that this arg belongs to.
    fn draw(&self, _ctx: &mut DrawContext<'_>) {
        unreachable!("Never directly call NodeDynamics impls for TransformArgPayload")
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.cached_rect.val().size()
    }

    fn rect(&self, _ctx: LayoutContext, _pos: Pos2) -> Rect {
        *self.cached_rect.val()
    }

    fn resize(&mut self, _dir: ResizeDir, _delta: Vec2) {
        unreachable!("Attempted to call resize on a TransformArgPayload")
    }

    fn override_edge_color(&self) -> Option<Color32> {
        Some(self.color)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum TransformLang {
    Python,
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
            value: TransformValue::Float(f.val()),
        },
        NodeVariant::Integer(i) => compute::TransformArgument {
            name: arg_name,
            value: TransformValue::Int(i.val()),
        },
        NodeVariant::Text(str) => compute::TransformArgument {
            name: arg_name,
            value: TransformValue::String(str.text.backing_value().clone()),
        },
        _ => unimplemented!(),
    }
}

fn command_from_transform(arg: compute::TransformValue, origin: NodeIdx) -> Box<dyn Action> {
    match arg {
        compute::TransformValue::Dataframe(df) => Box::new(AddConnectedDataframe {
            origin,
            df,
            name: "Transform output".to_owned(),
        }),
        compute::TransformValue::Int(int) => Box::new(AddConnectedNode {
            origin,
            constructor: move |i| NodeVariant::Integer(NumericPayload::init_from(int, i)),
        }),
        compute::TransformValue::Float(f) => Box::new(AddConnectedNode {
            origin,
            constructor: move |i| NodeVariant::Float(NumericPayload::init_from(f, i)),
        }),
        compute::TransformValue::String(s) => Box::new(AddConnectedNode {
            origin,
            constructor: |i| NodeVariant::Text(TextPayload::init_from(s, i)),
        }),
    }
}
