use dex_core::prelude::*;

use egui::{Color32, Id, LayerId, Order, Stroke};
use utils::Transient;

use crate::scripting::{ScriptLang, ScriptOutput, is_valid_ident, run_script};

use crate::{
    composites::{
        button::Button,
        selection_box::{Selected, SelectionBox},
    },
    layouts::{
        Bordered, HorizontalLayout, LayoutChild, VerticalLayout,
        canvas::{
            layout::{CanvasNodeAt, CanvasNodeScreenRect},
            nodes::{CanvasNode, shapes::SectionDivider},
        },
        error::ErrorLayout,
        pending::{IsPending, PendingLayout},
    },
    primitives::{
        interaction::{DragPointerPos, InteractionBox, WasClicked, WasDragReleased},
        nothing::Nothing,
        shapes::{Circle, Line},
        text::{CodeEditor, GetText, Label, LabelEditable, ValueVersion},
    },
};

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaEditor {
    active: LambdaLang,
    lang_selector: NodeUid<SelectionBox>,
    steel: NodeUid<CodeEditor>,
    python: NodeUid<CodeEditor>,
}

#[utils::dynamic_methods]
impl LambdaEditor {
    /// Build a lambda editor into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<LambdaEditor> {
        let lang_selector =
            SelectionBox::build(ws.clone(), vec!["Python".to_owned(), "Steel".to_owned()]);

        let steel = ws.insert_node(CodeEditor::new(String::new(), "steel".to_owned()));
        let python = ws.insert_node(CodeEditor::new(String::new(), "python".to_owned()));

        ws.insert_node(Self {
            active: LambdaLang::Python,
            lang_selector,
            steel,
            python,
        })
    }
}

#[utils::dynamic_node]
impl Node for LambdaEditor {
    fn type_name(&self) -> String {
        "Lambda Editor".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const GAP: f32 = 4.0;

        // Keep `active` in sync with the language selector.
        let selected = ctx
            .node
            .workspace
            .send_request(self.lang_selector, Selected)
            .unwrap_or(0);
        let lang = match selected {
            0 => LambdaLang::Python,
            1 => LambdaLang::Steel,
            _ => unreachable!(),
        };
        if lang != self.active {
            ctx.submit_action_for_self::<Self, _>(SetActive { lang }, "Set lambda language");
        }
        let active_editor = match self.active {
            LambdaLang::Steel => self.steel.erase(),
            LambdaLang::Python => self.python.erase(),
        };

        let body = VerticalLayout {
            children: vec![
                LayoutChild::from(self.lang_selector),
                LayoutChild::Id(active_editor),
            ],
            spacing: GAP,
            fill_last: false,
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&body, constraints)
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.lang_selector.erase());
        ctx.workspace.delete_node(self.steel.erase());
        ctx.workspace.delete_node(self.python.erase());
    }
}

defhandlers! { LambdaEditor {
    actions: [
        SetActive { lang: LambdaLang } => (this, s) { this.active = s.lang; },
    ],
    requests: [
        // The active editor's source and language.
        ActiveScript => (this, _q, ctx): (String, ScriptLang) {
            let selected = ctx.workspace.send_request(this.lang_selector, Selected).unwrap_or(0);
            let (editor, lang) = if selected == 1 {
                (this.steel, ScriptLang::Steel)
            } else {
                (this.python, ScriptLang::Python)
            };
            let source = ctx.workspace.send_request(editor, GetText).unwrap_or_default();
            (source, lang)
        },
    ],
}}

#[derive(Copy, PartialEq)]
#[utils::portable(noop_reset)]
pub enum LambdaLang {
    Steel,
    Python,
}

/// A draggable connection knob for a lambda argument.
#[utils::dynamic_type]
#[utils::portable]
pub struct ConnectionPort {
    connected: Option<NodeUid<CanvasNode>>,
    drag_sensor: NodeUid<InteractionBox>,
    drag_pos: Transient<ScreenPos>,
}

#[utils::dynamic_methods]
impl ConnectionPort {
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<ConnectionPort> {
        let drag_sensor_uid = ws.insert_node(InteractionBox::sensing(false, false, true));
        ws.insert_node(Self {
            connected: None,
            drag_sensor: drag_sensor_uid,
            drag_pos: Transient::default(),
        })
    }
}

#[utils::dynamic_node]
impl Node for ConnectionPort {
    fn type_name(&self) -> String {
        "Connection Port".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let wire_color = Color32::from_rgb(70, 130, 180);
        let port_color = Color32::from_rgb(50, 110, 160);

        let outer_radius = 4.0;
        let port_center = ctx.constraints.pos
            + ScreenPos {
                x: outer_radius,
                y: outer_radius,
            };
        let outer_circle = Circle {
            radius: outer_radius,
            border: Stroke::NONE,
            fill_color: port_color,
        };
        outer_circle.paint(ctx.ui.painter(), port_center);

        let inner_radius = 3.0;
        let inner_circle = Circle {
            radius: inner_radius,
            border: Stroke::NONE,
            fill_color: if self.connected.is_some() && self.drag_pos.val().is_none() {
                port_color
            } else {
                Color32::WHITE
            },
        };
        inner_circle.paint(ctx.ui.painter(), port_center);

        let wire_stroke = Stroke::new(2.0, wire_color);
        let wire_painter = ctx
            .ui
            .ctx()
            .layer_painter(LayerId::new(Order::Background, Id::new("lambda_wires")));

        // Poll the drag sensor
        ctx.draw_workspace_node(
            self.drag_sensor,
            DrawConstraints {
                pos: ctx.constraints.pos,
                x: Some(AxisConstraint::Exactly(outer_radius)),
                y: Some(AxisConstraint::Exactly(outer_radius)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        let cur_drag_pos: Option<ScreenPos> = ctx
            .node
            .workspace
            .send_request(self.drag_sensor, DragPointerPos {})
            .flatten();

        if let Some(pos) = cur_drag_pos {
            // Update ongoing drag
            self.drag_pos.set(pos);
            Line {
                span: (pos - port_center).to_vector(),
                stroke: wire_stroke,
            }
            .paint(&wire_painter, port_center);
        } else if let Some(target) = self.connected
            && let Some(rect) = ctx
                .node
                .workspace
                .send_request(
                    ctx.node.workspace.root(),
                    CanvasNodeScreenRect { node: target },
                )
                .flatten()
        {
            let target_anchor = ScreenPos {
                x: (rect.min.x + rect.max.x) * 0.5,
                y: (rect.min.y + rect.max.y) * 0.5,
            };
            Line {
                span: (target_anchor - port_center).to_vector(),
                stroke: wire_stroke,
            }
            .paint(&wire_painter, port_center);
        }

        let drag_released = ctx
            .node
            .workspace
            .send_request(self.drag_sensor, WasDragReleased {})
            .unwrap_or(false);
        if drag_released && let Some(pos) = *self.drag_pos.val() {
            let target = ctx
                .node
                .workspace
                .send_request(ctx.node.workspace.root(), CanvasNodeAt { pos })
                .flatten();
            ctx.submit_action_for_self::<Self, _>(
                SetConnection { target },
                "Set lambda argument connection",
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_center_size(
                port_center,
                Vector::splat(outer_radius * 2.0),
            )),
        }
    }
}

defhandlers! { ConnectionPort {
    actions: [
        SetConnection { target: Option<NodeUid<CanvasNode>> } => (this, s) {
            this.connected = s.target;
        },
    ],
    requests: [
        // The canvas node this port is wired to, if any.
        ConnectedTarget => (this, _q): Option<NodeUid<CanvasNode>> { this.connected },
    ],
}}

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaArg {
    label: NodeUid<LabelEditable>,
    param_name: NodeUid<LabelEditable>,
    port: NodeUid<ConnectionPort>,
}

#[utils::dynamic_methods]
impl LambdaArg {
    /// Build an argument into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<LambdaArg> {
        let label = ws.insert_node(LabelEditable::new("label".to_owned()));
        let param_name = ws.insert_node(LabelEditable::new("param_name".to_owned()));
        let port = ConnectionPort::build(ws.clone());
        ws.insert_node(Self {
            label,
            param_name,
            port,
        })
    }
}

#[utils::dynamic_node]
impl Node for LambdaArg {
    fn type_name(&self) -> String {
        "Lambda Argument".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const ARG_LABEL_GAP: f32 = 4.0;
        const PORT_GAP: f32 = 10.0;

        let fields = HorizontalLayout {
            children: vec![
                LayoutChild::from(self.label),
                LayoutChild::Node(Arc::new(Label::new(":".to_owned()))),
                LayoutChild::from(self.param_name),
            ],
            spacing: ARG_LABEL_GAP,
            allow_wrap: false,
        };
        let row = HorizontalLayout {
            children: vec![
                LayoutChild::from(self.port),
                LayoutChild::Node(Arc::new(fields)),
            ],
            spacing: PORT_GAP,
            allow_wrap: false,
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&row, constraints)
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.label.erase());
        ctx.workspace.delete_node(self.param_name.erase());
        ctx.workspace.delete_node(self.port.erase());
    }
}

defhandlers! { LambdaArg {
    requests: [
        // This argument's parameter name and the canvas node it is wired to.
        ArgBinding => (this, _q, ctx): (String, Option<NodeUid<CanvasNode>>) {
            let name = ctx.workspace.send_request(this.param_name, GetText).unwrap_or_default();
            let target = ctx.workspace.send_request(this.port, ConnectedTarget).flatten();
            (name, target)
        },
    ],
}}

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaArgs {
    args: Vec<NodeUid<LambdaArg>>,
    delete_buttons: Vec<NodeUid<Button>>,
    add_button: NodeUid<Button>,
}

#[utils::dynamic_methods]
impl LambdaArgs {
    /// Build the (empty) args row into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<LambdaArgs> {
        let add_button = Button::build(ws.clone(), Label::new("+".to_owned()));
        ws.insert_node(Self {
            args: Vec::new(),
            delete_buttons: Vec::new(),
            add_button,
        })
    }
}

#[utils::dynamic_node]
impl Node for LambdaArgs {
    fn type_name(&self) -> String {
        "Lambda Arguments".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const V_ROW_GAP: f32 = 2.0;
        const ARG_DELETE_GAP: f32 = 4.0;

        let constraints = ctx.constraints;

        let mut rows: Vec<LayoutChild> = self
            .args
            .iter()
            .zip(&self.delete_buttons)
            .map(|(arg, del)| {
                // Each row is a horizontal layout of [`arg`, `delete button`]
                LayoutChild::Node(Arc::new(HorizontalLayout {
                    children: vec![LayoutChild::from(*arg), LayoutChild::from(*del)],
                    spacing: ARG_DELETE_GAP,
                    allow_wrap: false,
                }))
            })
            .collect();
        rows.push(LayoutChild::from(self.add_button));

        let layout = VerticalLayout {
            children: rows,
            spacing: V_ROW_GAP,
            fill_last: false,
        };
        let region = ctx
            .draw_node(&layout, constraints)
            .region()
            .unwrap_or_else(|| {
                ScreenRegion::from_min_size(constraints.pos, Vector { x: 0.0, y: 0.0 })
            });

        // Poll the (now-drawn) buttons and dispatch add/delete.
        for i in 0..self.delete_buttons.len() {
            if ctx
                .node
                .workspace
                .send_request(self.delete_buttons[i].erase(), WasClicked)
                .unwrap_or(false)
            {
                ctx.submit_action_for_self::<Self, _>(DeleteArg { index: i }, "Delete argument");
            }
        }
        if ctx
            .node
            .workspace
            .send_request(self.add_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            ctx.submit_action_for_self::<Self, _>(AddArg, "Add argument");
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        for arg in &self.args {
            ctx.workspace.delete_node(arg.erase());
        }
        for btn in &self.delete_buttons {
            ctx.workspace.delete_node(btn.erase());
        }
        ctx.workspace.delete_node(self.add_button.erase());
    }
}

defhandlers! { LambdaArgs {
    actions: [
        AddArg => (this, _a, ctx) {
            let arg = LambdaArg::build(ctx.workspace.action_handle());

            // A delete button, polled by this row.
            let mut delete_label = Label::new("×".to_owned());
            delete_label.font = Font::proportional(11.0);
            delete_label.color = Color::gray(120);
            let delete_button = Button::build_with(ctx.workspace.action_handle(), delete_label, |b| {
                b.padding = 1.0;
                b.border = dex_core::Stroke::NONE;
            });

            this.args.push(arg);
            this.delete_buttons.push(delete_button);
        },
        DeleteArg { index: usize } => (this, s, ctx) {
            if s.index < this.args.len() {
                let arg = this.args.remove(s.index);
                let btn = this.delete_buttons.remove(s.index);
                // Each cascades to its own children via their on_delete.
                ctx.workspace.delete_node(arg.erase());
                ctx.workspace.delete_node(btn.erase());
            }
        },
    ],
    requests: [
        // Each argument's parameter name and wired canvas node, in order.
        ArgBindings => (this, _q, ctx): Vec<(String, Option<NodeUid<CanvasNode>>)> {
            this.args
                .iter()
                .map(|arg| ctx.workspace.send_request(*arg, ArgBinding).unwrap_or_default())
                .collect()
        },
    ],
}}

#[utils::dynamic_type]
#[utils::portable]
pub struct Lambda {
    /// An editable display name.
    name: NodeUid<LabelEditable>,
    args: NodeUid<LambdaArgs>,
    editor: NodeUid<LambdaEditor>,
    update_button: NodeUid<Button>,
    /// The node this lambda computes. A stable id whose content is recomputed.
    output: NodeUid,

    /// Last-seen value version of each wired upstream node, so a change re-fires this lambda.
    #[dynamic(skip)]
    seen_deps: Transient<std::collections::HashMap<NodeUid, u64>>,
}

#[utils::dynamic_methods]
impl Lambda {
    /// Build a lambda into `ws`.
    pub fn new(ws: WorkspaceActionHandle) -> Lambda {
        let name = ws.insert_node(LabelEditable::new("Lambda".to_owned()));
        let editor = LambdaEditor::build(ws.clone());
        let args = LambdaArgs::build(ws.clone());
        let update_button = Button::build(ws.clone(), Label::new("Update".to_owned()));
        let output = ws.insert_node(Nothing).erase();
        Self {
            name,
            args,
            editor,
            update_button,
            output,
            seen_deps: Transient::default(),
        }
    }

    /// Recompute [`Lambda::output`]: flip it to a pending view, then run the active script + commit on a worker thread.
    fn run_update(&self, ctx: &DrawContext) {
        let workspace = ctx.node.workspace;

        // Cancel any in-flight computation first.
        workspace.cancel_all_tasks_for(ctx.node.id);

        let Some((source, lang)) = workspace.send_request(self.editor, ActiveScript) else {
            return;
        };

        // Resolve each argument to a `name = value` pair.
        let bindings = workspace
            .send_request(self.args, ArgBindings)
            .unwrap_or_default();
        let mut args: Vec<(String, String)> = Vec::new();
        for (name, target) in bindings {
            if !is_valid_ident(&name) {
                continue;
            }
            if let Some(target) = target
                && let Some(value) = workspace.send_request(target.erase(), GetText)
            {
                args.push((name, value));
            }
        }

        // Show the previous output under a pending marker while recomputing.
        let previous = workspace
            .get_node(self.output)
            .unwrap_or_else(|| Arc::new(Nothing));
        let new_pending = if previous.as_any_ref().is::<PendingLayout>() {
            // Child already displayed pending
            previous
        } else {
            Arc::new(PendingLayout {
                child: LayoutChild::Node(previous),
            })
        };
        workspace.action_handle().insert_node_at_dyn(
            self.output,
            Arc::new(PendingLayout {
                child: LayoutChild::Node(new_pending),
            }),
        );

        let output = self.output;
        let task = ComputeTask::new(ctx.node.id, move || {
            let (handle, actions) = WorkspaceActionHandle::buffered();
            match run_script(lang, &source, &handle, &args) {
                Ok(ScriptOutput::Nothing) => handle.insert_node_at_dyn(output, Arc::new(Nothing)),
                Ok(ScriptOutput::Node(node)) => handle.insert_node_at_dyn(output, node),
                Ok(ScriptOutput::Handle(uid)) => handle.commit_output(output, uid),
                Err(e) => {
                    handle.insert_node_at_dyn(output, Arc::new(ErrorLayout::message(e.to_string())))
                }
            }
            drop(handle);
            actions.try_iter().collect()
        });
        workspace.submit_task(task);
    }

    /// Poll wired nodes; returns `true` if any dependency's version has changed since last check.
    fn poll_dependencies(&self, ctx: &DrawContext) -> bool {
        let workspace = ctx.node.workspace;
        let bindings = workspace
            .send_request(self.args, ArgBindings)
            .unwrap_or_default();

        let mut changed = false;
        let mut seen = self
            .seen_deps
            .val_mut_or_else(std::collections::HashMap::new);
        for (_name, target) in bindings {
            let Some(target) = target else { continue };
            let uid = target.erase();
            // Don't sample a source that is recomputing; wait for it to settle.
            if workspace.send_request(uid, IsPending).unwrap_or(false) {
                continue;
            }
            let version = workspace.send_request(uid, ValueVersion).unwrap_or(0);
            if let Some(&prev) = seen.get(&uid)
                && prev != version
            {
                changed = true;
            }
            seen.insert(uid, version);
        }
        changed
    }
}

#[utils::dynamic_node]
impl Node for Lambda {
    fn type_name(&self) -> String {
        "Lambda".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const V_SECTIONS_GAP: f32 = 6.0;
        const OUTER_PADDING: f32 = 8.0;
        const FALLBACK_SIZE: Vector = Vector { x: 400.0, y: 240.0 };

        let constraints = ctx.constraints;
        let node_size = Vector {
            x: constraints
                .x
                .map(|a| a.provided_value())
                .unwrap_or(FALLBACK_SIZE.x),
            y: constraints
                .y
                .map(|a| a.provided_value())
                .unwrap_or(FALLBACK_SIZE.y),
        };
        let origin = constraints.pos;

        let body = VerticalLayout {
            children: vec![
                LayoutChild::from(self.name),
                LayoutChild::from(self.args),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::from(self.editor),
                LayoutChild::from(self.update_button),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Id(self.output),
            ],
            spacing: V_SECTIONS_GAP,
            // The output claims the remaining height
            fill_last: true,
        };
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(body)),
            padding: OUTER_PADDING,
            corner_radius: 4.0,
            fill_color: Color::WHITE,
            border_width: 1.0,
            border_color: Color::gray(170),
        };
        ctx.draw_node(
            &bordered,
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(node_size.x)),
                y: Some(AxisConstraint::Exactly(node_size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: constraints.should_clip,
            },
        );

        // Recompute when the update button is clicked, or when a dependency has changed.
        let clicked = ctx
            .node
            .workspace
            .send_request(self.update_button.erase(), WasClicked)
            .unwrap_or(false);
        let deps_changed = self.poll_dependencies(&ctx);
        if clicked || deps_changed {
            self.run_update(&ctx);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, node_size)),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.args.erase());
        ctx.workspace.delete_node(self.editor.erase());
        ctx.workspace.delete_node(self.update_button.erase());
        ctx.workspace.delete_node(self.output);
    }
}

defhandlers! { Lambda {
    extern_requests: [
        // Value probe: a lambda's value is its resolved output's value.
        GetText => (this, _q, ctx): String {
            ctx.workspace.send_request(this.output, GetText).unwrap_or_default()
        },
        // Change probe: a lambda's version is its output node's version.
        ValueVersion => (this, _q, ctx): u64 { ctx.workspace.node_version(this.output) },
        // A lambda is pending while its output is mid-recompute.
        IsPending => (this, _q, ctx): bool {
            ctx.workspace.send_request(this.output, IsPending).unwrap_or(false)
        },
    ],
}}
