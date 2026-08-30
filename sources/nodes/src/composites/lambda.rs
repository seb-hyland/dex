use dex_core::prelude::*;

use egui::{Id, LayerId, Order};
use utils::Transient;

use crate::layouts::desktops::{Desktops, PythonPrelude};
use crate::primitives::checkout;
use crate::scripting::{
    ScriptOutput, ScriptValue, ValueDelegate, is_valid_ident, resolve_arg, run_script,
};

use crate::{
    composites::button::Button,
    layouts::{
        Bordered, HorizontalLayout, LayoutChild, VerticalLayout,
        canvas::{layout::Canvas, nodes::shapes::SectionDivider},
        error::ErrorLayout,
        pending::PendingLayout,
    },
    primitives::{
        interaction::{DragPointerPos, InteractionBox, WasClicked, WasDragReleased},
        nothing::Nothing,
        shapes::{Circle, Path},
        text::{CodeEditor, GetText, Label, LabelEditable, SetText},
    },
};

// ================================================================================
// LAMBDA EDITOR
// ================================================================================

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaEditor {
    python: NodeUid<CodeEditor>,
    edit_externally: NodeUid<Button>,
}

#[utils::dynamic_methods]
impl LambdaEditor {
    /// Build a lambda editor into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<LambdaEditor> {
        let python = ws.insert_node(CodeEditor::new(String::new(), "python".to_owned()));
        let edit_externally = Button::build(ws.clone(), Label::new("Edit in IDE".to_owned()));

        ws.insert_node(Self {
            python,
            edit_externally,
        })
    }
}

#[utils::dynamic_node]
impl Node for LambdaEditor {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda Editor".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        ctx.draw_workspace_node(self.python.erase(), constraints)
            .unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.python.erase());
    }
}

defhandlers! { LambdaEditor {
    requests: [
        // The editor's current source.
        ActiveScript => (this, _q, ctx): String {
            ctx.workspace.send_request(this.python, GetText).unwrap_or_default()
        },
        // The underlying code editor, so its owner can configure it.
        ActiveEditor => (this, _q): NodeUid { this.python.erase() },
    ],
}}

/// Marks the node a wire runs to, drawn over it.
const CONNECTION_MARK_INSET: f32 = 1.0;
/// The drop candidate under a live drag, distinct from a settled connection.
const CANDIDATE_COLOR: Color = Color {
    r: 40,
    g: 160,
    b: 110,
    a: 255,
};

/// Ring to show the target of a wire.
fn outline(painter: &egui::Painter, region: ScreenRegion, color: Color) {
    painter.rect_stroke(
        egui::Rect::from(region).shrink(CONNECTION_MARK_INSET),
        4.0,
        egui::Stroke::new(2.0, egui::Color32::from(color)),
        egui::StrokeKind::Inside,
    );
}

/// A draggable connection knob for a lambda argument.
#[utils::dynamic_type]
#[utils::portable]
pub struct ConnectionPort {
    /// Wired to a node that lives elsewhere on the canvas.
    #[uid_ref]
    connected: Option<NodeUid>,
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
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Connection Port".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let wire_color = Color::rgb(70, 130, 180);
        let port_color = Color::rgb(50, 110, 160);

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
                Color::WHITE
            },
        };
        inner_circle.paint(ctx.ui.painter(), port_center);

        let wire_stroke = Stroke::new(2.0, wire_color);
        let wire_painter = ctx
            .ui
            .ctx()
            .layer_painter(LayerId::new(Order::Foreground, Id::new("lambda_wires")));
        // The outline goes over the nodes.
        let mark_painter = ctx.ui.ctx().layer_painter(LayerId::new(
            Order::Foreground,
            Id::new("lambda_wire_marks"),
        ));

        // Poll the drag sensor.
        ctx.draw_workspace_node(
            self.drag_sensor.erase(),
            DrawConstraints {
                pos: ctx.constraints.pos,
                x: Some(AxisConstraint::Exactly(outer_radius * 2.0)),
                y: Some(AxisConstraint::Exactly(outer_radius * 2.0)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        let cur_drag_pos: Option<ScreenPos> = ctx
            .node
            .workspace
            .send_request(self.drag_sensor, DragPointerPos {})
            .flatten();

        let ws = ctx.node.workspace;
        if let Some(pos) = cur_drag_pos {
            // Update ongoing drag
            self.drag_pos.set(pos);
            Path::span((pos - port_center).to_vector(), wire_stroke)
                .paint(&wire_painter, port_center);
            // Say what would be wired up if the drag ended here.
            if let Some(rect) = ws.inspectable_at(pos).and_then(|c| ws.inspectable_rect(c)) {
                outline(&mark_painter, rect, CANDIDATE_COLOR);
            }
        } else if let Some(target) = self.connected
            && let Some(rect) = ws.inspectable_rect(target)
        {
            let target_anchor = ScreenPos {
                x: (rect.min.x + rect.max.x) * 0.5,
                y: (rect.min.y + rect.max.y) * 0.5,
            };
            Path::span((target_anchor - port_center).to_vector(), wire_stroke)
                .paint(&wire_painter, port_center);

            outline(&mark_painter, rect, wire_color);
        }

        let drag_released = ctx
            .node
            .workspace
            .send_request(self.drag_sensor, WasDragReleased {})
            .unwrap_or(false);
        if drag_released && let Some(pos) = *self.drag_pos.val() {
            // Anything the user can point at, at whatever depth it sits.
            let target = ws.inspectable_at(pos);
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
        SetConnection { target: Option<NodeUid> } => (this, s) {
            this.connected = s.target;
        },
    ],
    requests: [
        // The node this port is wired to, if any.
        ConnectedTarget => (this, _q): Option<NodeUid> { this.connected },
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
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda Argument".into()
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
        // This argument's parameter name and the node it is wired to.
        ArgBinding => (this, _q, ctx): (String, Option<NodeUid>) {
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
    fn type_name(&self, _ctx: NodeContext) -> String {
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
        // Each argument's parameter name and wired node, in order.
        ArgBindings => (this, _q, ctx): Vec<(String, Option<NodeUid>)> {
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

    /// Where the script is checked out for external editing.
    #[dynamic(skip)]
    checkout: Transient<checkout::Checkout>,

    /// Opens the script in the user's editor.
    edit_externally: NodeUid<Button>,
}

#[utils::dynamic_methods]
impl Lambda {
    /// Build a lambda into `ws`.
    pub fn new(ws: WorkspaceActionHandle) -> Lambda {
        let name = ws.insert_node(LabelEditable::new("Lambda".to_owned()));
        let editor = LambdaEditor::build(ws.clone());
        let args = LambdaArgs::build(ws.clone());
        let update_button = Button::build(ws.clone(), Label::new("Update".to_owned()));
        let edit_externally = Button::build(ws.clone(), Label::new("Edit in IDE".to_owned()));
        let output = ws.insert_node(Nothing).erase();
        Self {
            name,
            args,
            editor,
            update_button,
            edit_externally,
            output,
            seen_deps: Transient::default(),
            checkout: Transient::default(),
        }
    }

    /// Recompute [`Lambda::output`]: flip it to a pending view, then run the active script + commit on a worker thread.
    fn run_update(&self, ctx: NodeContext) {
        let workspace = ctx.workspace;

        // Cancel any in-flight computation first.
        workspace.cancel_all_tasks_for(ctx.id);

        let Some(source) = workspace.send_request(self.editor, ActiveScript) else {
            return;
        };

        // Resolve each argument to a typed `name = value` pair.
        let bindings = workspace
            .send_request(self.args, ArgBindings)
            .unwrap_or_default();
        let mut args: Vec<(String, ScriptValue)> = Vec::new();
        for (name, target) in bindings {
            if !is_valid_ident(&name) {
                continue;
            }
            if let Some(target) = target {
                let value = resolve_arg(workspace, target).value;
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

        let py_prelude = ctx
            .workspace
            .send_request(ctx.workspace.root().cast::<Desktops>(), PythonPrelude)
            .unwrap_or_default();

        let output = self.output;
        let task = ComputeTask::new(ctx.id, move || {
            let (handle, actions) = WorkspaceActionHandle::buffered();
            match run_script(&source, &py_prelude, &handle, &args) {
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

    /// The wired arguments as `(name, python type)`, for a checkout's header.
    fn script_globals(&self, ctx: NodeContext) -> Vec<(String, String)> {
        let workspace = ctx.workspace;
        workspace
            .send_request(self.args, ArgBindings)
            .unwrap_or_default()
            .into_iter()
            .filter(|(name, _)| is_valid_ident(name))
            .map(|(name, target)| {
                let ty = target
                    .map(|t| resolve_arg(workspace, t).value.python_type())
                    .unwrap_or("None");
                (name, ty.to_owned())
            })
            .collect()
    }

    /// Check the script out to a file and open it in the user's editor.
    #[dynamic(skip)] // takes a borrowed context
    pub fn edit_externally(&self, ctx: NodeContext) {
        let source = ctx
            .workspace
            .send_request(self.editor, ActiveScript)
            .unwrap_or_default();
        match checkout::open(&ctx.id.key(), &source, &self.script_globals(ctx)) {
            Ok(open) => self.checkout.set(open),
            Err(e) => eprintln!("could not check the script out: {e}"),
        }
    }

    /**
        Pull in edits made to a checked-out file.
        The file wins while it is checked out.
    */
    fn poll_checkout(&self, ctx: NodeContext) {
        let Some(current) = self.checkout.val().clone() else {
            return;
        };
        let Some(pulled) = checkout::poll(&current) else {
            return;
        };
        self.checkout.set(pulled.checkout);

        let Some(editor) = ctx.workspace.send_request(self.editor, ActiveEditor) else {
            return;
        };
        ctx.workspace.submit_action(
            editor.cast::<CodeEditor>(),
            "Pulled external edits",
            SetText {
                value: pulled.source,
            },
        );
    }

    /// Poll wired nodes; returns `true` if any dependency's version has changed since last check.
    fn poll_dependencies(&self, ctx: NodeContext) -> bool {
        let workspace = ctx.workspace;
        let bindings = workspace
            .send_request(self.args, ArgBindings)
            .unwrap_or_default();

        let mut changed = false;
        let mut seen = self
            .seen_deps
            .val_mut_or_else(std::collections::HashMap::new);
        for (_name, target) in bindings {
            let Some(uid) = target else { continue };
            let resolved = resolve_arg(workspace, uid);
            // Don't sample a source that is recomputing; wait for it to settle.
            if resolved.pending {
                continue;
            }
            if let Some(&prev) = seen.get(&uid)
                && prev != resolved.version
            {
                changed = true;
            }
            seen.insert(uid, resolved.version);
        }
        changed
    }
}

#[utils::dynamic_node]
impl Node for Lambda {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda".into()
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
                LayoutChild::from(self.edit_externally),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Inspectable(self.output),
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

        // Recompute when the update button is clicked (dependency-driven reruns
        // happen in `tick`, so they fire even when this lambda isn't drawn).
        if ctx
            .node
            .workspace
            .send_request(self.update_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            self.run_update(ctx.node);
        }

        if ctx
            .node
            .workspace
            .send_request(self.edit_externally.erase(), WasClicked)
            .unwrap_or(false)
        {
            self.edit_externally(ctx.node);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, node_size)),
        }
    }

    fn tick(&self, ctx: NodeContext) {
        self.poll_checkout(ctx);
        // Re-fire when a wired dependency's value changed.
        if self.poll_dependencies(ctx) {
            self.run_update(ctx);
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.args.erase());
        ctx.workspace.delete_node(self.editor.erase());
        ctx.workspace.delete_node(self.update_button.erase());
        ctx.workspace.delete_node(self.edit_externally.erase());
        ctx.workspace.delete_node(self.output);
    }
}

defhandlers! { Lambda {
    requests: [
        LambdaOutput => (this, _q): NodeUid { this.output },
    ],
    extern_requests: [
        // Forwarded to the editor, so the script is reachable from the lambda.
        ActiveScript => (this, _q, ctx): String {
            ctx.workspace.send_request(this.editor, ActiveScript).unwrap_or_default()
        },
    ],
}}

// ================================================================================
// LAMBDA CANVAS
// ================================================================================

/// One input parameter of a [`ComputeCanvas`].
#[utils::dynamic_type]
#[utils::portable]
pub struct ComputeParam {
    pub name: String,
    pub value: String,
}

#[utils::dynamic_methods]
impl ComputeParam {
    pub fn build(ws: WorkspaceActionHandle, name: String) -> NodeUid<ComputeParam> {
        ws.insert_node(Self {
            name,
            value: String::new(),
        })
    }
}

#[utils::dynamic_node]
impl Node for ComputeParam {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Compute Parameter".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let text = if self.value.is_empty() {
            self.name.clone()
        } else {
            format!("{}: {}", self.name, self.value)
        };
        let pill = Bordered {
            child: LayoutChild::Node(Arc::new(Label::new(text))),
            padding: 5.0,
            corner_radius: 4.0,
            fill_color: Color::gray(238),
            border_width: 1.0,
            border_color: Color::gray(160),
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&pill, constraints)
    }
}

// A value leaf: `scripting::node_to_value` reads its `value`.
defhandlers! { ComputeParam {
    actions: [
        // Push a fresh name + value.
        SetParam { name: String, value: String } => (this, s) {
            this.name = s.name;
            this.value = s.value;
        },
    ],
    requests: [
        ParamEntry => (this, _q): (String, String) { (this.name.clone(), this.value.clone()) },
    ],
}}

#[utils::dynamic_type]
#[utils::portable]
pub struct ComputeCanvas {
    canvas: NodeUid<Canvas>,
    #[dynamic(skip)]
    params: Vec<NodeUid<ComputeParam>>,
    output_port: NodeUid<ConnectionPort>,
}

#[utils::dynamic_methods]
impl ComputeCanvas {
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<ComputeCanvas> {
        let canvas = Canvas::build(ws.clone());
        let output_port = ConnectionPort::build(ws.clone());
        ws.insert_node(Self {
            canvas,
            params: Vec::new(),
            output_port,
        })
    }
}

const CC_PARAM_H: f32 = 40.0;
const CC_OUT_H: f32 = 34.0;
const CC_GAP: f32 = 8.0;

#[utils::dynamic_node]
impl Node for ComputeCanvas {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Compute Canvas".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value()).unwrap_or(0.0);
        let avail_h = ctx.constraints.y.map(|a| a.provided_value()).unwrap_or(0.0);
        let origin = ctx.constraints.pos;

        // Parameter row along the top. Drawn as things the user can point at.
        let mut x = origin.x + CC_GAP;
        for &param in &self.params {
            let res = ctx.draw_inspectable_node(
                param.erase(),
                DrawConstraints {
                    pos: ScreenPos {
                        x,
                        y: origin.y + CC_GAP,
                    },
                    x: Some(AxisConstraint::AtMost((avail_w - CC_GAP).max(0.0))),
                    y: Some(AxisConstraint::AtMost(CC_PARAM_H - CC_GAP)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
            if let Some(region) = res.and_then(|r| r.region()) {
                x += region.size().x + CC_GAP;
            }
        }

        // Inner canvas fills the middle.
        let canvas_y = origin.y + CC_PARAM_H;
        let canvas_h = (avail_h - CC_PARAM_H - CC_OUT_H).max(0.0);
        ctx.draw_workspace_node(
            self.canvas.erase(),
            DrawConstraints {
                pos: ScreenPos {
                    x: origin.x,
                    y: canvas_y,
                },
                x: Some(AxisConstraint::Exactly(avail_w)),
                y: Some(AxisConstraint::Exactly(canvas_h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

        // Output pin along the bottom: a label plus the outgoing port.
        let out_y = origin.y + avail_h - CC_OUT_H + CC_GAP;
        let label = Label::new("output →".to_owned());
        ctx.draw_node(
            &label,
            DrawConstraints {
                pos: ScreenPos {
                    x: origin.x + CC_GAP,
                    y: out_y,
                },
                x: Some(AxisConstraint::AtMost(120.0)),
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        ctx.draw_workspace_node(
            self.output_port.erase(),
            DrawConstraints {
                pos: ScreenPos {
                    x: origin.x + CC_GAP + 90.0,
                    y: out_y,
                },
                x: Some(AxisConstraint::Exactly(8.0)),
                y: Some(AxisConstraint::Exactly(8.0)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: avail_w,
                    y: avail_h,
                },
            )),
        }
    }

    fn deref_target(&self) -> Option<NodeUid> {
        // Unhandled messages fall through to the inner canvas.
        Some(self.canvas.erase())
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.canvas.erase());
        ctx.workspace.delete_node(self.output_port.erase());
        for p in &self.params {
            ctx.workspace.delete_node(p.erase());
        }
    }
}

defhandlers! { ComputeCanvas {
    actions: [
        // Reconcile the param pins to `entries` (name, value), preserving pin ids by index so existing connections survive.
        SyncParams { entries: Vec<(String, String)> } => (this, s, ctx) {
            while this.params.len() < s.entries.len() {
                let p = ComputeParam::build(ctx.workspace.action_handle(), String::new());
                this.params.push(p);
            }
            while this.params.len() > s.entries.len() {
                if let Some(p) = this.params.pop() {
                    ctx.workspace.delete_node(p.erase());
                }
            }
            for (p, (name, value)) in this.params.iter().zip(&s.entries) {
                // Only write (and thus bump the version) a param that changed to prevent unnecessary re-firing.
                let cur = ctx.workspace.send_request(*p, ParamEntry).unwrap_or_default();
                if &cur.0 != name || &cur.1 != value {
                    ctx.workspace.submit_action(
                        *p,
                        "Sync compute param",
                        SetParam { name: name.clone(), value: value.clone() },
                    );
                }
            }
        },
    ],
    requests: [
        // The current (name, value) of each param pin, in order.
        ParamEntries => (this, _q, ctx): Vec<(String, String)> {
            this.params
                .iter()
                .map(|p| ctx.workspace.send_request(*p, ParamEntry).unwrap_or_default())
                .collect()
        },
        // The inner node the output pin is wired to.
        OutputConnected => (this, _q, ctx): Option<NodeUid> {
            ctx.workspace.send_request(this.output_port, ConnectedTarget).flatten()
        },
    ],
}}

/// A lambda whose body is a [`ComputeCanvas`].
#[utils::dynamic_type]
#[utils::portable]
pub struct CanvasLambda {
    name: NodeUid<LabelEditable>,
    args: NodeUid<LambdaArgs>,
    open_button: NodeUid<Button>,
    compute_canvas: NodeUid<ComputeCanvas>,
}

#[utils::dynamic_methods]
impl CanvasLambda {
    pub fn new(ws: WorkspaceActionHandle) -> CanvasLambda {
        let name = ws.insert_node(LabelEditable::new("Canvas Lambda".to_owned()));
        let args = LambdaArgs::build(ws.clone());
        let open_button = Button::build(ws.clone(), Label::new("Open".to_owned()));
        let compute_canvas = ComputeCanvas::build(ws.clone());
        Self {
            name,
            args,
            open_button,
            compute_canvas,
        }
    }

    /// The node the canvas's output pin is wired to, if any.
    fn output_node(&self, ctx: NodeContext) -> Option<NodeUid> {
        ctx.workspace
            .send_request(self.compute_canvas, OutputConnected)
            .flatten()
    }
}

#[utils::dynamic_node]
impl Node for CanvasLambda {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Lambda".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const V_SECTIONS_GAP: f32 = 6.0;
        const OUTER_PADDING: f32 = 8.0;
        const FALLBACK_SIZE: Vector = Vector { x: 260.0, y: 200.0 };

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

        // Preview the current output value.
        let out_value = self
            .output_node(ctx.node)
            .map(|n| resolve_arg(ctx.node.workspace, n).value)
            .map(|v| v.display())
            .unwrap_or_else(|| "(no output)".to_owned());

        let body = VerticalLayout {
            children: vec![
                LayoutChild::from(self.name),
                LayoutChild::from(self.args),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::from(self.open_button),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Node(Arc::new(Label::new(out_value))),
            ],
            spacing: V_SECTIONS_GAP,
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

        // Open the compute canvas fullscreen on the workspace root's override stack.
        if ctx
            .node
            .workspace
            .send_request(self.open_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            let root = ctx.node.workspace.root();
            ctx.node.workspace.submit_action(
                root.cast::<crate::layouts::desktops::Desktops>(),
                "Open compute canvas",
                crate::layouts::desktops::PushOverride {
                    node: self.compute_canvas.erase(),
                },
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, node_size)),
        }
    }

    fn tick(&self, ctx: NodeContext) {
        // Push resolved arg values into the canvas's param pins.
        let ws = ctx.workspace;
        let bindings = ws.send_request(self.args, ArgBindings).unwrap_or_default();
        let current = ws
            .send_request(self.compute_canvas, ParamEntries)
            .unwrap_or_default();

        let desired: Vec<(String, String)> = bindings
            .iter()
            .enumerate()
            .map(|(i, (name, target))| {
                let keep = || current.get(i).map(|(_, v)| v.clone()).unwrap_or_default();
                let value = match target {
                    Some(t) => {
                        let resolved = resolve_arg(ws, *t);
                        // Hold the last value while the source is recomputing.
                        if resolved.pending {
                            keep()
                        } else {
                            resolved.value.display()
                        }
                    }
                    None => keep(),
                };
                (name.clone(), value)
            })
            .collect();

        if desired != current {
            ws.submit_action(
                self.compute_canvas,
                "Sync params",
                SyncParams { entries: desired },
            );
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.args.erase());
        ctx.workspace.delete_node(self.open_button.erase());
        ctx.workspace.delete_node(self.compute_canvas.erase());
    }
}

defhandlers! { CanvasLambda {
    extern_requests: [
        // A canvas lambda represents whatever its output pin is wired to.
        ValueDelegate => (this, _q, ctx): Option<NodeUid> {
            ctx.workspace.send_request(this.compute_canvas, OutputConnected).flatten()
        },
    ],
}}
