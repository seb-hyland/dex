use dex_core::prelude::*;

use egui::{Id, LayerId, Order};
use utils::Transient;

use crate::layouts::desktops::{Desktops, PythonPrelude};
use crate::primitives::checkout;
use crate::scripting::{
    DataflowOutput, ScriptOutput, ScriptValue, ValueDelegate, is_valid_ident, resolve_arg,
    run_script,
};

use crate::{
    composites::button::Button,
    layouts::{
        Bordered, HorizontalLayout, LayoutChild, VerticalLayout,
        canvas::{
            layout::{AdoptCanvasNode, Canvas, Layer, RemoveCanvasItem},
            nodes::{CanvasItemDeletable, CanvasNode, shapes::SectionDivider},
        },
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
        ws.insert_node(Self::holding(ws.clone(), String::new()))
    }

    /// An editor already holding `source`, for a caller placing one under an id of its own.
    pub fn holding(ws: WorkspaceActionHandle, source: String) -> LambdaEditor {
        Self {
            python: ws.insert_node(CodeEditor::new(source, "python".to_owned())),
            edit_externally: Button::build(ws.clone(), Label::new("Edit in IDE".to_owned())),
        }
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

/// Whether `ancestor` owns `start`, at any depth.
fn encloses(ws: &Workspace, ancestor: NodeUid, start: NodeUid) -> bool {
    const MAX_DEPTH: usize = 64;
    let mut current = Some(start);
    for _ in 0..MAX_DEPTH {
        match current {
            Some(uid) if uid == ancestor => return true,
            Some(uid) => current = ws.owner_of(uid),
            None => return false,
        }
    }
    false
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
        let port = Self::empty(ws.clone());
        ws.insert_node(port)
    }

    /// An unwired port, for a caller placing one under an id of its own.
    pub fn empty(ws: WorkspaceActionHandle) -> ConnectionPort {
        Self {
            connected: None,
            drag_sensor: ws.insert_node(InteractionBox::sensing(false, false, true)),
            drag_pos: Transient::default(),
        }
    }
}

#[utils::dynamic_node]
impl Node for ConnectionPort {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Connection Port".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let wire_color = Color::rgba(176, 202, 224, 150);
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

        let wire_stroke = Stroke::new(1.5, wire_color);

        let clip = crate::layouts::canvas::layout::wire_clip(ctx.ui.ctx())
            .unwrap_or_else(|| ctx.ui.clip_rect());
        let wire_painter = ctx
            .ui
            .ctx()
            .layer_painter(LayerId::new(Order::Foreground, Id::new("lambda_wires")))
            .with_clip_rect(clip);
        // The outline goes over the nodes.
        let mark_painter = ctx
            .ui
            .ctx()
            .layer_painter(LayerId::new(
                Order::Foreground,
                Id::new("lambda_wire_marks"),
            ))
            .with_clip_rect(clip);

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
            // Anything the user can point at, at whatever depth it sits, except something this port lives inside.
            let target = ws
                .inspectable_at(pos)
                .filter(|&candidate| !encloses(ws, candidate, ctx.node.id));
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
        let arg = NodeUid::mint();
        let port = ConnectionPort::build(ws.clone());
        Self::build_with(ws, arg, port.erase(), "param_name".to_owned())
    }

    /**
        Build an argument named `name` under ids the caller chose.

    */
    pub fn build_with(
        ws: WorkspaceActionHandle,
        arg: NodeUid<LambdaArg>,
        port: NodeUid,
        name: String,
    ) -> NodeUid<LambdaArg> {
        let label = ws.insert_node(LabelEditable::new("label".to_owned()));
        let param_name = ws.insert_node(LabelEditable::new(name));
        ws.insert_node_at(
            port.cast::<ConnectionPort>(),
            ConnectionPort::empty(ws.clone()),
        );
        ws.insert_node_at(
            arg,
            Self {
                label,
                param_name,
                port: port.cast(),
            },
        );
        arg
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
        /*
            The same binding, plus the port that holds it.

            A reader wants the name and the source; a writer wants somewhere to
            send `SetConnection`. Handing back both means a script that is
            rewiring does not have to go looking for the port separately.
        */
        ArgInput => (this, _q, ctx): (String, NodeUid, Option<NodeUid>) {
            let name = ctx.workspace.send_request(this.param_name, GetText).unwrap_or_default();
            let target = ctx.workspace.send_request(this.port, ConnectedTarget).flatten();
            (name, this.port.erase(), target)
        },
    ],
}}

/// The little × an argument row is polled against.
fn delete_arg_button(ws: WorkspaceActionHandle) -> NodeUid<Button> {
    let mut label = Label::new("×".to_owned());
    label.font = Font::proportional(11.0);
    label.color = Color::gray(120);
    Button::build_with(ws, label, |b| {
        b.padding = 1.0;
        b.border = dex_core::Stroke::NONE;
    })
}

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
        ws.insert_node(Self::empty(ws.clone()))
    }

    /// An empty args row, for a caller placing one under an id of its own.
    pub fn empty(ws: WorkspaceActionHandle) -> LambdaArgs {
        Self {
            args: Vec::new(),
            delete_buttons: Vec::new(),
            add_button: Button::build(ws.clone(), Label::new("+".to_owned())),
        }
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
            let delete_button = delete_arg_button(ctx.workspace.action_handle());
            this.args.push(arg);
            this.delete_buttons.push(delete_button);
        },
        /*
            Take an argument the caller already built, keeping its ids.

            [`AddArg`] mints everything itself, so the caller learns none of it
            until the queue drains — too late to wire the port in the same pass.
        */
        AddArgAt { arg: NodeUid } => (this, s, ctx) {
            let delete_button = delete_arg_button(ctx.workspace.action_handle());
            this.args.push(s.arg.cast());
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
        // Every input as `(name, port, source)`, in declaration order.
        DataflowInputs => (this, _q, ctx): Vec<(String, NodeUid, Option<NodeUid>)> {
            this.args
                .iter()
                .filter_map(|arg| ctx.workspace.send_request(*arg, ArgInput))
                .collect()
        },
        // The arguments themselves, for a caller that needs to address one.
        ArgNodes => (this, _q): Vec<NodeUid> {
            this.args.iter().map(|a| a.erase()).collect()
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
    /**
        Build a lambda named `name`, running `source`, under ids the caller chose.

        `args` is where its arguments go and `output` is what consumers wire to
        — the two ids a script has to know before the queue drains. Everything
        else is minted here, since nothing outside needs to address it.
    */
    pub fn new_with(
        ws: WorkspaceActionHandle,
        args: NodeUid<LambdaArgs>,
        output: NodeUid,
        name: String,
        source: String,
    ) -> Lambda {
        ws.insert_node_at(args, LambdaArgs::empty(ws.clone()));
        ws.insert_node_at_dyn(output, Arc::new(Nothing));
        Self {
            name: ws.insert_node(LabelEditable::new(name)),
            args,
            editor: ws.insert_node(LambdaEditor::holding(ws.clone(), source)),
            update_button: Button::build(ws.clone(), Label::new("Update".to_owned())),
            edit_externally: Button::build(ws.clone(), Label::new("Edit in IDE".to_owned())),
            output,
            seen_deps: Transient::default(),
            checkout: Transient::default(),
        }
    }

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

        // The graph as it stands now, so the script can look up what its
        // arguments point at once it is off on a worker thread.
        let graph = GraphSnapshot::capture(workspace);

        let output = self.output;
        let task = ComputeTask::new(ctx.id, move || {
            let (handle, actions) = WorkspaceActionHandle::buffered();
            match run_script(&source, &py_prelude, &handle, &args, graph) {
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
        /*
            The display name, which is what identifies an operator.

            A lambda's meaning lives in its script, which nothing can read
            symbolically, so a walker recognises `Add` from `Mult` by what the
            user called it.
        */
        LambdaName => (this, _q, ctx): String {
            ctx.workspace.send_request(this.name, GetText).unwrap_or_default()
        },
        // The editor, so a caller can read or rewrite the script.
        LambdaEditorNode => (this, _q): NodeUid { this.editor.erase() },
        // The name label, so a caller can rename what it built.
        LambdaNameNode => (this, _q): NodeUid { this.name.erase() },
        // The arguments row, so a caller can add to or read it.
        LambdaArgsNode => (this, _q): NodeUid { this.args.erase() },
    ],
    extern_requests: [
        // A lambda's value is the output slot it recomputes, which is already
        // drawn as something the user can point at.
        DataflowOutput => (this, _q): Option<NodeUid> { Some(this.output) },
        // Forwarded to the editor, so the script is reachable from the lambda.
        ActiveScript => (this, _q, ctx): String {
            ctx.workspace.send_request(this.editor, ActiveScript).unwrap_or_default()
        },
        // Forwarded to the arguments row, so a lambda describes its own inputs.
        DataflowInputs => (this, _q, ctx): Vec<(String, NodeUid, Option<NodeUid>)> {
            ctx.workspace.send_request(this.args, DataflowInputs).unwrap_or_default()
        },
    ],
}}

// ================================================================================
// LAMBDA CANVAS
// ================================================================================

/// The tint that marks a parameter apart from the nodes it feeds.
const PARAM_FILL: Color = Color {
    r: 227,
    g: 238,
    b: 250,
    a: 255,
};
const PARAM_BORDER: Color = Color {
    r: 90,
    g: 140,
    b: 195,
    a: 255,
};

/**
    One input parameter of a [`ComputeCanvas`].

    A pin does not hold a copy of its argument — it *points* at whatever the
    lambda's argument is wired to and delegates its value there. Copying meant
    rendering the value to text at the boundary, so every parameter arrived
    inside the canvas as a string: `2 ** x` raised a `TypeError`, and worse,
    `a + b` silently concatenated. Delegating keeps the type intact, tables and
    all, and there is nothing to keep in sync.
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct ComputeParam {
    pub name: String,
    /// What the lambda's argument is wired to. Outside this canvas, hence a
    /// reference rather than a child.
    #[uid_ref]
    source: Option<NodeUid>,
}

#[utils::dynamic_methods]
impl ComputeParam {
    pub fn build(ws: WorkspaceActionHandle, name: String) -> NodeUid<ComputeParam> {
        ws.insert_node(Self { name, source: None })
    }
}

#[utils::dynamic_node]
impl Node for ComputeParam {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Compute Parameter".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let shown = self
            .source
            .map(|source| resolve_arg(ctx.node.workspace, source).value.display())
            .unwrap_or_default();
        let text = if shown.is_empty() {
            self.name.clone()
        } else {
            format!("{}: {}", self.name, shown)
        };
        let pill = Bordered {
            child: LayoutChild::Node(Arc::new(Label::new(text))),
            padding: 5.0,
            corner_radius: 4.0,
            fill_color: PARAM_FILL,
            border_width: 1.5,
            border_color: PARAM_BORDER,
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&pill, constraints)
    }
}

defhandlers! { ComputeParam {
    actions: [
        // Point this pin at a fresh name and source.
        SetParam { name: String, source: Option<NodeUid> } => (this, s) {
            this.name = s.name;
            this.source = s.source;
        },
    ],
    requests: [
        ParamEntry => (this, _q): (String, Option<NodeUid>) {
            (this.name.clone(), this.source)
        },
    ],
    extern_requests: [
        // The pin is worth exactly what the argument is wired to.
        ValueDelegate => (this, _q): Option<NodeUid> { this.source },
        // A pin exists because the lambda has that argument; removing it here
        // would only have the next sync put it back.
        CanvasItemDeletable => (this, _q): bool { false },
    ],
}}

/// Where the pin for argument `slot` lands, and how big it is.
const PARAM_SLOT_W: f32 = 100.0;
const PARAM_SIZE: Vector = Vector { x: 90.0, y: 32.0 };

fn param_slot(slot: usize) -> Vector {
    Vector {
        x: slot as f32 * PARAM_SLOT_W,
        y: 0.0,
    }
}

/// Put a pin for argument `slot` onto `canvas` as an item, under ids of its own.
fn build_param_item(ws: &Workspace, canvas: NodeUid<Canvas>, slot: usize, name: String) -> NodeUid {
    let item = NodeUid::mint();
    place_param_item(ws, canvas, item, slot, name, None);
    item
}

/// Build the pin `item` wraps, and the item, and put it on `canvas`.
///
/// The item is what the surface draws and what a wire points at; the pin inside
/// carries the name and the delegation. Adopted rather than placed, because
/// `PlaceOnCanvas` would centre it and the slot position is the point.
fn place_param_item(
    ws: &Workspace,
    canvas: NodeUid<Canvas>,
    item: NodeUid,
    slot: usize,
    name: String,
    source: Option<NodeUid>,
) {
    let handle = ws.action_handle();
    let pin = handle.insert_node(ComputeParam { name, source });
    CanvasNode::build_at(handle, item, pin.erase(), param_slot(slot), PARAM_SIZE);
    ws.submit_action(
        canvas,
        "Show a parameter pin",
        AdoptCanvasNode {
            node: item,
            layer: Layer::Midground,
        },
    );
}

#[utils::dynamic_type]
#[utils::portable]
pub struct ComputeCanvas {
    canvas: NodeUid<Canvas>,
    /**
        The parameter pins, in argument order — as *canvas items*.

        An item is what a wire can point at and what can be dragged, so the pins
        are held by their items rather than bare. The surface owns them, so
        these are references.
    */
    #[uid_ref]
    #[dynamic(skip)]
    params: Vec<NodeUid>,
    output_port: NodeUid<ConnectionPort>,
}

#[utils::dynamic_methods]
impl ComputeCanvas {
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<ComputeCanvas> {
        let uid = NodeUid::mint();
        let output_port = ConnectionPort::build(ws.clone());
        Self::build_with(ws, uid, output_port.erase())
    }

    /**
        Build a compute canvas under ids the caller chose.

        `output_port` is the one piece a caller has to know up front: it is what
        decides the lambda's result, and there is no way to ask for it before
        the queue drains. The inner surface needs no id of its own — a canvas
        action sent here dereferences down to it.
    */
    pub fn build_with(
        ws: WorkspaceActionHandle,
        uid: NodeUid<ComputeCanvas>,
        output_port: NodeUid,
    ) -> NodeUid<ComputeCanvas> {
        ws.insert_node_at(
            output_port.cast::<ConnectionPort>(),
            ConnectionPort::empty(ws.clone()),
        );
        ws.insert_node_at(
            uid,
            Self {
                canvas: Canvas::build(ws.clone()),
                params: Vec::new(),
                output_port: output_port.cast(),
            },
        );
        uid
    }
}

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

        // The pins are items on the surface below, not a strip up here: they
        // pan with the graph they feed, so a wire to one stays short.
        let canvas_y = origin.y;
        let canvas_h = (avail_h - CC_OUT_H).max(0.0);
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
        /*
            Reconcile the pins to `entries`, preserving item ids by index so
            existing connections survive.

            A new pin lands in the next slot along the top row and can be
            dragged from there; one that is no longer an argument is taken off
            the surface.
        */
        SyncParams { entries: Vec<(String, Option<NodeUid>)> } => (this, s, ctx) {
            while this.params.len() < s.entries.len() {
                let slot = this.params.len();
                this.params.push(build_param_item(ctx.workspace, this.canvas, slot, String::new()));
            }
            while this.params.len() > s.entries.len() {
                if let Some(item) = this.params.pop() {
                    ctx.workspace.submit_action(
                        this.canvas,
                        "Drop a parameter pin",
                        RemoveCanvasItem { node: item },
                    );
                }
            }
            for (item, (name, source)) in this.params.iter().zip(&s.entries) {
                // Only write (and so bump the version) a pin that changed, or
                // every tick re-fires whatever reads it.
                let cur = ctx.workspace.send_request(*item, ParamEntry).unwrap_or_default();
                if &cur.0 != name || &cur.1 != source {
                    ctx.workspace.submit_action(
                        *item,
                        "Sync compute param",
                        SetParam { name: name.clone(), source: *source },
                    );
                }
            }
        },
        /*
            Set the pins outright, under ids the caller chose.

            [`SyncParams`] mints a pin for each new entry, which a script cannot
            then wire to — the queue has not drained, so there is nothing to look
            up. Naming them is what lets a body be built and connected in one
            pass. Pins left unnamed here are dropped.
        */
        SyncParamsAt { entries: Vec<(NodeUid, String, Option<NodeUid>)> } => (this, s, ctx) {
            let kept: Vec<NodeUid> = s.entries.iter().map(|(uid, _n, _s)| *uid).collect();
            for old in &this.params {
                if !kept.contains(old) {
                    ctx.workspace.submit_action(
                        this.canvas,
                        "Drop a parameter pin",
                        RemoveCanvasItem { node: *old },
                    );
                }
            }
            this.params = s.entries
                .into_iter()
                .enumerate()
                .map(|(slot, (item, name, source))| {
                    place_param_item(ctx.workspace, this.canvas, item, slot, name, source);
                    item
                })
                .collect();
        },
    ],
    requests: [
        // The current (name, value) of each param pin, in order.
        ParamEntries => (this, _q, ctx): Vec<(String, Option<NodeUid>)> {
            this.params
                .iter()
                .map(|p| ctx.workspace.send_request(*p, ParamEntry).unwrap_or_default())
                .collect()
        },
        // The inner node the output pin is wired to.
        OutputConnected => (this, _q, ctx): Option<NodeUid> {
            ctx.workspace.send_request(this.output_port, ConnectedTarget).flatten()
        },
        /*
            The parameter pins themselves, in order.

            [`ParamEntries`] says what they hold; this says where they are. A
            walker that descends into a canvas lambda needs the pins as ids, so
            it can recognise one when the inner graph points at it.
        */
        ParamPins => (this, _q): Vec<NodeUid> { this.params.clone() },
        // The output pin, so a caller can rewire what this canvas produces.
        OutputPin => (this, _q): NodeUid { this.output_port.erase() },
        // The surface the items sit on.
        InnerCanvas => (this, _q): NodeUid { this.canvas.erase() },
    ],
}}

/**
    Stands in for a canvas lambda's result.

    A wire needs something to point at. Without this the only target a canvas
    lambda offers is the whole lambda, so consuming its value and referring to
    the lambda itself are the same gesture — and a script handed one as an
    argument gets the value, never the lambda. Giving the result its own id
    separates the two, and mirrors how [`Lambda`] already draws its output.
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct OutputProxy {
    /// Whose output pin decides what this stands for. A reference: the lambda
    /// owns both this and the canvas.
    #[uid_ref]
    canvas: NodeUid<ComputeCanvas>,
}

#[utils::dynamic_methods]
impl OutputProxy {
    pub fn build(
        ws: WorkspaceActionHandle,
        canvas: NodeUid<ComputeCanvas>,
    ) -> NodeUid<OutputProxy> {
        ws.insert_node(Self { canvas })
    }
}

#[utils::dynamic_node]
impl Node for OutputProxy {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda Output".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let ws = ctx.node.workspace;
        let text = ws
            .send_request(self.canvas, OutputConnected)
            .flatten()
            .map(|node| resolve_arg(ws, node).value.display())
            .unwrap_or_else(|| "(no output)".to_owned());
        let constraints = ctx.constraints;
        ctx.draw_node(&Label::new(text), constraints)
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        // The proxy is a stand-in, so it offers what it stands for.
        let target = ctx
            .workspace
            .send_request(self.canvas, OutputConnected)
            .flatten()?;
        let target_ctx = NodeContext {
            id: target,
            workspace: ctx.workspace,
        };
        ctx.workspace
            .get_node(target)
            .and_then(|node| node.build_inspector(target_ctx))
    }
}

defhandlers! { OutputProxy {
    extern_requests: [
        // The proxy is worth exactly what the pin is wired to.
        ValueDelegate => (this, _q, ctx): Option<NodeUid> {
            ctx.workspace.send_request(this.canvas, OutputConnected).flatten()
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
    /// What a consumer wires to, so binding the lambda and binding its result
    /// are different gestures.
    output: NodeUid<OutputProxy>,
}

#[utils::dynamic_methods]
impl CanvasLambda {
    /**
        Build a canvas lambda named `name` under ids the caller chose.

        The three a builder script needs: `args` to hang parameters on,
        `compute_canvas` to place the body on, and `output_port` to say which
        node is the result.
    */
    pub fn new_with(
        ws: WorkspaceActionHandle,
        args: NodeUid<LambdaArgs>,
        compute_canvas: NodeUid<ComputeCanvas>,
        output_port: NodeUid,
        name: String,
    ) -> CanvasLambda {
        ws.insert_node_at(args, LambdaArgs::empty(ws.clone()));
        ComputeCanvas::build_with(ws.clone(), compute_canvas, output_port);
        Self {
            name: ws.insert_node(LabelEditable::new(name)),
            args,
            open_button: Button::build(ws.clone(), Label::new("Open".to_owned())),
            compute_canvas,
            output: OutputProxy::build(ws.clone(), compute_canvas),
        }
    }

    pub fn new(ws: WorkspaceActionHandle) -> CanvasLambda {
        let name = ws.insert_node(LabelEditable::new("Canvas Lambda".to_owned()));
        let args = LambdaArgs::build(ws.clone());
        let open_button = Button::build(ws.clone(), Label::new("Open".to_owned()));
        let compute_canvas = ComputeCanvas::build(ws.clone());
        let output = OutputProxy::build(ws.clone(), compute_canvas);
        Self {
            name,
            args,
            open_button,
            compute_canvas,
            output,
        }
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

        let body = VerticalLayout {
            children: vec![
                LayoutChild::from(self.name),
                LayoutChild::from(self.args),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::from(self.open_button),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Inspectable(self.output.erase()),
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

        // The pin points at the argument's source rather than holding a copy,
        // so there is no value to render and nothing to hold on to while the
        // source recomputes — `resolve_arg` sees through it either way.
        let desired: Vec<(String, Option<NodeUid>)> = bindings
            .iter()
            .map(|(name, target)| (name.clone(), *target))
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
        ctx.workspace.delete_node(self.output.erase());
    }
}

defhandlers! { CanvasLambda {
    requests: [
        /*
            The node the inner output pin is wired to: this lambda's body.

            Where a symbolic walk starts, and where it resumes when it descends
            into a nested canvas lambda.
        */
        LambdaBody => (this, _q, ctx): Option<NodeUid> {
            ctx.workspace.send_request(this.compute_canvas, OutputConnected).flatten()
        },
        // The canvas holding the body.
        ComputeCanvasNode => (this, _q): NodeUid { this.compute_canvas.erase() },
    ],
    extern_requests: [
        // Deliberately *not* `ValueDelegate`: the lambda is itself, and its
        // result is the proxy. See [`OutputProxy`].
        DataflowOutput => (this, _q): Option<NodeUid> { Some(this.output.erase()) },
        LambdaName => (this, _q, ctx): String {
            ctx.workspace.send_request(this.name, GetText).unwrap_or_default()
        },
        DataflowInputs => (this, _q, ctx): Vec<(String, NodeUid, Option<NodeUid>)> {
            ctx.workspace.send_request(this.args, DataflowInputs).unwrap_or_default()
        },
        LambdaArgsNode => (this, _q): NodeUid { this.args.erase() },
        LambdaNameNode => (this, _q): NodeUid { this.name.erase() },
        // Forwarded so a walker need not fetch the canvas to reach the pins.
        ParamPins => (this, _q, ctx): Vec<NodeUid> {
            ctx.workspace.send_request(this.compute_canvas, ParamPins).unwrap_or_default()
        },
        OutputPin => (this, _q, ctx): NodeUid {
            ctx.workspace.send_request(this.compute_canvas, OutputPin)
                .unwrap_or_else(NodeUid::nil)
        },
    ],
}}
