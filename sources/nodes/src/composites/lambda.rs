use dex_core::prelude::*;
use dex_core::theme;

use egui::{Id, Pos2};
use utils::Transient;

use crate::argtypes::{ArgSpec, ArgType, TypeFault, arg_type_labels, check_arg_types, describe};
use crate::layouts::desktops::{Desktops, PythonPrelude};
use crate::primitives::checkout;
use crate::primitives::dropdown::{Dropdown, DropdownSelection};
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
        icon::Glyph,
        interaction::{DragPointerPos, InteractionBox, TakeClicked, WasDragReleased},
        nothing::Nothing,
        shapes::{Circle, Path, Rect},
        text::{CodeEditor, GetText, Label, LabelEditable, SetText, TakeExternalEditRequest},
    },
};

// ================================================================================
// LAMBDA EDITOR
// ================================================================================

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaEditor {
    python: NodeUid<CodeEditor>,
}

#[utils::dynamic_methods]
impl LambdaEditor {
    /// Build a lambda editor into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<LambdaEditor> {
        ws.insert_node(Self::holding(
            ws.clone(),
            "def transform():\n    return".to_owned(),
        ))
    }

    /// An editor already holding `source`, for a caller placing one under an id of its own.
    pub fn holding(ws: WorkspaceActionHandle, source: String) -> LambdaEditor {
        Self {
            python: ws.insert_node(CodeEditor::new(source, "python".to_owned())),
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
        // Inspectable, so the lens can reach the editor's own commands.
        ctx.draw_inspectable_node(self.python.erase(), constraints)
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
/// Every wire shares one layer and every mark another, so that a mark is drawn
/// over all the wires and not merely over its own.
const WIRE_LAYER: &str = "lambda_wires";
const MARK_LAYER: &str = "lambda_wire_marks";
/// A wire carrying the wrong kind of thing. Redder than the port it runs from,
/// and no more transparent than an ordinary wire, so the fault reads at a glance.
const FAULT_WIRE: Color = Color::rgba(200, 60, 60, 200);
/// The drop candidate under a live drag, distinct from a settled connection.
const CANDIDATE_COLOR: Color = Color {
    r: 40,
    g: 160,
    b: 110,
    a: 255,
};

/// Ring to show the target of a wire.
fn outline(ctx: &mut DrawContext, region: ScreenRegion, color: Color) {
    let region = ScreenRegion::from(egui::Rect::from(region).shrink(CONNECTION_MARK_INSET));
    ctx.draw_node(
        &Rect {
            size: region.size(),
            corner_radius: 4.0,
            fill_color: Color::TRANSPARENT,
            border: Stroke::new(2.0, color),
            stroke_kind: StrokeKind::Inside,
        },
        loose(region.min),
    );
}

/// Constraints for something drawn at `pos` at its own size, unclipped.
fn loose(pos: ScreenPos) -> DrawConstraints {
    DrawConstraints {
        pos,
        x: None,
        y: None,
        wrap: WrapConstraints::NotAllowed,
        should_clip: false,
    }
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
    /// Whether what is wired here is not what the argument asked for.
    faulty: bool,
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
            faulty: false,
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
        let (wire_color, port_color) = if self.faulty {
            (FAULT_WIRE, theme::DANGER)
        } else {
            (Color::rgba(176, 202, 224, 150), theme::ACCENT_STRONG)
        };

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

        /*
            The port sits inside whatever surface it belongs to, and on a zoomed canvas that surface's items
            are drawn on a transformed layer.
        */
        let to_global = ctx.ui.ctx().layer_transform_to_global(ctx.ui.layer_id());
        let to_screen = |p: ScreenPos| -> ScreenPos {
            to_global.map_or(p, |t| ScreenPos::from(t.mul_pos(Pos2::from(p))))
        };
        let screen_port_center = to_screen(port_center);

        // Clipped to the surface the wires belong to.
        let clip = crate::layouts::canvas::layout::wire_clip(ctx.ui.ctx())
            .unwrap_or_else(|| ctx.ui.clip_rect());

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
            .flatten()
            .map(to_screen);

        let ws = ctx.node.workspace;
        // Where the wire ends, and what to ring at the far end of it.
        let (end, mark) = if let Some(pos) = cur_drag_pos {
            // Update ongoing drag
            self.drag_pos.set(pos);
            // Say what would be wired up if the drag ended here.
            let candidate = ws
                .inspectable_at(pos)
                .and_then(|c| ws.inspectable_rect(c))
                .map(|rect| (rect, CANDIDATE_COLOR));
            (Some(pos), candidate)
        } else if let Some(target) = self.connected
            && let Some(rect) = ws.inspectable_rect(target)
        {
            // Stop at the target's edge rather than its middle.
            (
                Some(rect.edge_towards(screen_port_center)),
                Some((rect, wire_color)),
            )
        } else {
            (None, None)
        };

        if let Some(end) = end {
            ctx.overlay_in(Id::new(WIRE_LAYER), clip, |ctx| {
                ctx.draw_node(
                    &Path::span((end - screen_port_center).to_vector(), wire_stroke),
                    loose(screen_port_center),
                );
            });
        }
        if let Some((rect, color)) = mark {
            ctx.overlay_in(Id::new(MARK_LAYER), clip, |ctx| outline(ctx, rect, color));
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
            // A fresh wire has not been checked yet, so it does not carry the
            // last one's verdict until the check that follows says so.
            this.faulty = false;
        },
        // Mark, or clear, the argument's type fault. See [`ConnectionPort::faulty`].
        SetPortFault { faulty: bool } => (this, s) {
            this.faulty = s.faulty;
        },
    ],
    requests: [
        // The node this port is wired to, if any.
        ConnectedTarget => (this, _q): Option<NodeUid> { this.connected },
        // Whether this wire is carrying the wrong kind of thing.
        PortFaulty => (this, _q): bool { this.faulty },
    ],
}}

/**
    The type an argument's row is set in.

    An editable label comes up a step larger than a plain one, and the row is
    made of both — so everything in it is told which of the two it is, rather
    than half of it being a size smaller by default.
*/
const ARG_FONT: Font = Font::proportional(theme::TEXT_LG);

#[utils::dynamic_type]
#[utils::portable]
pub struct LambdaArg {
    label: NodeUid<LabelEditable>,
    param_name: NodeUid<LabelEditable>,
    port: NodeUid<ConnectionPort>,
    /// What this argument will accept, picked from a list.
    kind_picker: NodeUid<Dropdown>,
    /// What the two written kinds are finished with: a type name, or a test.
    detail: NodeUid<LabelEditable>,
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
        // Every field in the row is a word in a sentence, so an empty one shows
        // a rule to write on rather than a caret's worth of blank square.
        let field = |text: String| {
            let mut label = LabelEditable::new(text);
            label.underline_when_empty = true;
            ws.insert_node(label)
        };
        let label = field("label".to_owned());
        let param_name = field(name);
        /*
            A declaration reads as part of the argument's own line, not as a
            control docked beside it: the row's own type and ink, no frame, no
            padding, and only as wide as the word it is showing.
        */
        let kind_picker = ws.insert_node({
            let mut picker = Dropdown::new(arg_type_labels());
            picker.font = ARG_FONT;
            picker.boxed = false;
            picker.shrink_to_text = true;
            picker
        });
        let detail = field(String::new());
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
                kind_picker,
                detail,
            },
        );
        arg
    }
}

impl LambdaArg {
    /// What this argument is currently declared to be.
    fn kind(&self, ws: &Workspace) -> ArgType {
        ArgType::at(
            ws.send_request(self.kind_picker, DropdownSelection)
                .unwrap_or(0),
        )
    }
}

#[utils::dynamic_node]
impl Node for LambdaArg {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda Argument".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const ARG_LABEL_GAP: f32 = theme::SPACE_SM;
        const PORT_GAP: f32 = theme::SPACE_LG;
        /// The declaration is set close to the brackets holding it.
        const DECL_GAP: f32 = 0.0;

        let kind = self.kind(ctx.node.workspace);
        // The punctuation between the row's parts is the row's own text, not
        // chrome around it: one size and one colour all the way across.
        let word = |text: &str| {
            let mut label = Label::new(text.to_owned());
            label.font = ARG_FONT;
            LayoutChild::Node(Arc::new(label))
        };
        // `(any)`, or `(some pyarrow.Table)`: the kind, and what finishes it.
        let declaration = HorizontalLayout {
            children: [
                Some(word("(")),
                Some(LayoutChild::from(self.kind_picker)),
                kind.takes_detail().then(|| LayoutChild::from(self.detail)),
                Some(word(")")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            spacing: DECL_GAP,
            allow_wrap: false,
        };
        let fields = HorizontalLayout {
            children: vec![
                LayoutChild::from(self.label),
                word(":"),
                LayoutChild::from(self.param_name),
                LayoutChild::Node(Arc::new(declaration)),
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
        ctx.workspace.delete_node(self.kind_picker.erase());
        ctx.workspace.delete_node(self.detail.erase());
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
        /*
            What this argument is called, which port carries it, and what it was
            declared to be: the kind, and whatever finishes it.

            Separate from [`ArgInput`] rather than folded into it, because that
            one is the dataflow protocol a script walks and this is a claim
            about types that only the checker reads.
        */
        ArgDeclaration => (this, _q, ctx): (String, NodeUid, ArgType, String) {
            let ws = ctx.workspace;
            let name = ws.send_request(this.param_name, GetText).unwrap_or_default();
            let detail = ws.send_request(this.detail, GetText).unwrap_or_default();
            (name, this.port.erase(), this.kind(ws), detail)
        },
        // The dropdown that says what this argument accepts.
        ArgKindPicker => (this, _q): NodeUid { this.kind_picker.erase() },
    ],
}}

/// The lambda's run control: a play glyph and the word for it.
fn run_button(ws: WorkspaceActionHandle) -> NodeUid<Button> {
    Button::build_with(ws, Label::new("Run".to_owned()), |b| {
        b.icon = Some(Glyph::Play);
        b.icon_gap = theme::SPACE_SM;
    })
}

/// The little × an argument row is polled against.
fn delete_arg_button(ws: WorkspaceActionHandle) -> NodeUid<Button> {
    let mut label = Label::new(String::new());
    label.font = Font::proportional(theme::TEXT_XS);
    label.color = theme::INK_MUTED;
    Button::build_with(ws, label, |b| {
        b.icon = Some(Glyph::Cross);
        b.padding = theme::SPACE_XS;
        b.padding_x = 0.0;
        b.corner_radius = theme::RADIUS_SM;
        b.border = Stroke::NONE;
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
            add_button: Button::build_icon(ws.clone(), Glyph::Plus),
        }
    }
}

#[utils::dynamic_node]
impl Node for LambdaArgs {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Lambda Arguments".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const V_ROW_GAP: f32 = theme::SPACE_XS;
        const ARG_DELETE_GAP: f32 = theme::SPACE_SM;

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

        let layout = VerticalLayout::new(rows, V_ROW_GAP);
        let region = ctx
            .draw_node(&layout, constraints)
            .region()
            .unwrap_or_else(|| ScreenRegion::from_min_size(constraints.pos, Vector::ZERO));

        // Poll the (now-drawn) buttons and dispatch add/delete.
        for i in 0..self.delete_buttons.len() {
            if ctx
                .node
                .workspace
                .send_request(self.delete_buttons[i].erase(), TakeClicked)
                .unwrap_or(false)
            {
                ctx.submit_action_for_self::<Self, _>(DeleteArg { index: i }, "Delete argument");
            }
        }
        if ctx
            .node
            .workspace
            .send_request(self.add_button.erase(), TakeClicked)
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
        // Every argument's declaration, in order. See [`ArgDeclaration`].
        ArgDeclarations => (this, _q, ctx): Vec<(String, NodeUid, ArgType, String)> {
            this.args
                .iter()
                .filter_map(|arg| ctx.workspace.send_request(*arg, ArgDeclaration))
                .collect()
        },
        // The arguments themselves, for a caller that needs to address one.
        ArgNodes => (this, _q): Vec<NodeUid> {
            this.args.iter().map(|a| a.erase()).collect()
        },
    ],
}}

/**
    What each argument is called, what it is wired to, and what it was declared
    to be.

    Enough that any change worth re-checking on shows up as a difference, and
    nothing that changes on its own from one frame to the next.
*/
type ArgShape = Vec<(String, Option<NodeUid>, ArgType, String)>;

/// The same, and what each argument was worth when it was last looked at.
type CheckedArgs = Vec<(String, Option<NodeUid>, ArgType, String, u64)>;

/// What `port` was declared to be, from a list of declarations.
fn declaration_for(
    declarations: &[(String, NodeUid, ArgType, String)],
    port: NodeUid,
) -> (ArgType, String) {
    declarations
        .iter()
        .find(|(_, declared, _, _)| *declared == port)
        .map(|(_, _, kind, detail)| (*kind, detail.clone()))
        .unwrap_or((ArgType::Any, String::new()))
}

/**
    Every argument of `args`, resolved and paired with its declaration.

    Unwired ones included, holding nothing. They seed no global and a script
    never sees them — but "nothing is wired to it" is the most useful thing a
    check can say about an argument that was asked to be a table, and it can
    only say it if it is told the argument is there.
*/
fn arg_specs(ws: &Workspace, args: NodeUid<LambdaArgs>) -> Vec<ArgSpec> {
    let declarations = ws.send_request(args, ArgDeclarations).unwrap_or_default();
    ws.send_request(args, DataflowInputs)
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _, _)| is_valid_ident(name))
        .map(|(name, port, target)| {
            let (kind, detail) = declaration_for(&declarations, port);
            ArgSpec {
                name,
                port,
                kind,
                detail,
                value: target.map(|target| resolve_arg(ws, target).value),
            }
        })
        .collect()
}

/// Say on every one of `specs` whether it is among `faults`, so a wire that has
/// come right is cleared in the same pass that reddens one that has not.
fn report_faults(handle: &WorkspaceActionHandle, specs: &[ArgSpec], faults: &[TypeFault]) {
    for spec in specs {
        handle.submit_action(
            spec.port.cast::<ConnectionPort>(),
            "Marked the argument's type",
            SetPortFault {
                faulty: faults.iter().any(|fault| fault.port == spec.port),
            },
        );
    }
}

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

    /**
        The arguments' shape as last seen: what each is called, what it is wired
        to, and what it was declared to be.

        Watched separately from the values, because a change here is a change to
        what this lambda *is* — a rewiring, a rename, a new declaration — and
        none of it moves a version on anything upstream.
    */
    #[dynamic(skip)]
    seen_shape: Transient<ArgShape>,

    /// Where the script is checked out for external editing.
    #[dynamic(skip)]
    checkout: Transient<checkout::Checkout>,
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
            update_button: run_button(ws.clone()),
            output,
            seen_deps: Transient::default(),
            seen_shape: Transient::default(),
            checkout: Transient::default(),
        }
    }

    /// Build a lambda into `ws`.
    pub fn new(ws: WorkspaceActionHandle) -> Lambda {
        let name = ws.insert_node(LabelEditable::new("Lambda".to_owned()));
        let editor = LambdaEditor::build(ws.clone());
        let args = LambdaArgs::build(ws.clone());
        let update_button = run_button(ws.clone());
        let output = ws.insert_node(Nothing).erase();
        Self {
            name,
            args,
            editor,
            update_button,
            output,
            seen_deps: Transient::default(),
            seen_shape: Transient::default(),
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

        // Each argument, resolved, and carrying what it was declared to be.
        let specs = arg_specs(workspace, self.args);
        // Only the wired ones reach the script: an unwired argument binds no
        // name, and the check above says so before the script can trip over it.
        let args: Vec<(String, ScriptValue)> = specs
            .iter()
            .filter_map(|spec| Some((spec.name.clone(), spec.value.clone()?)))
            .collect();

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
            /*
                The arguments are checked before the script is handed them, so a
                mistyped one is reported as itself rather than as whatever the
                script made of it four lines in — and the wire carrying it is
                reddened, which says which *node* is wrong.
            */
            let faults = check_arg_types(&py_prelude, &specs);
            report_faults(&handle, &specs, &faults);
            if !faults.is_empty() {
                handle
                    .insert_node_at_dyn(output, Arc::new(ErrorLayout::message(describe(&faults))));
            } else {
                match run_script(&source, &py_prelude, &handle, &args, graph) {
                    Ok(ScriptOutput::Nothing) => {
                        handle.insert_node_at_dyn(output, Arc::new(Nothing))
                    }
                    Ok(ScriptOutput::Node(node)) => handle.insert_node_at_dyn(output, node),
                    Ok(ScriptOutput::Handle(uid)) => handle.commit_output(output, uid),
                    Err(e) => handle
                        .insert_node_at_dyn(output, Arc::new(ErrorLayout::message(e.to_string()))),
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
        let Some(mut current) = self.checkout.val().clone() else {
            return;
        };
        // The environment may have been changed since this was checked out.
        if let Some(refreshed) = checkout::refresh_config(&current) {
            current = refreshed;
            self.checkout.set(current.clone());
        }
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

    /**
        Whether the arguments' shape has changed since this was last asked.

        Nothing to compare against on the first tick, so a lambda that has never
        run is not made to run merely by being looked at.
    */
    fn poll_arg_shape(&self, ctx: NodeContext) -> bool {
        let ws = ctx.workspace;
        let declarations = ws
            .send_request(self.args, ArgDeclarations)
            .unwrap_or_default();
        let shape: ArgShape = ws
            .send_request(self.args, DataflowInputs)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, port, target)| {
                let (kind, detail) = declaration_for(&declarations, port);
                (name, target, kind, detail)
            })
            .collect();
        let previous = self.seen_shape.val().clone();
        self.seen_shape.set(shape.clone());
        previous.is_some_and(|previous| previous != shape)
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
        const V_SECTIONS_GAP: f32 = theme::SPACE_MD;
        const OUTER_PADDING: f32 = theme::SPACE_LG;
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

        // The output claims whatever the rest of the node leaves.
        let body = VerticalLayout::filling_last(
            vec![
                LayoutChild::from(self.name),
                LayoutChild::from(self.args),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::from(self.editor),
                LayoutChild::from(self.update_button),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Inspectable(self.output),
            ],
            V_SECTIONS_GAP,
        );
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(body)),
            padding: OUTER_PADDING,
            corner_radius: theme::RADIUS_LG,
            fill_color: theme::SURFACE,
            border_width: theme::HAIRLINE,
            border_color: theme::LINE,
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
            .send_request(self.update_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            self.run_update(ctx.node);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, node_size)),
        }
    }

    fn tick(&self, ctx: NodeContext) {
        self.poll_checkout(ctx);
        // The command sits in the code editor's own inspector; only its owner
        // can honour it, so the request is taken here.
        if let Some(editor) = ctx.workspace.send_request(self.editor, ActiveEditor)
            && ctx
                .workspace
                .send_request(editor, TakeExternalEditRequest)
                .unwrap_or(false)
        {
            self.edit_externally(ctx);
        }
        // Re-fire when a wired dependency's value changed, or when the
        // arguments themselves did. Both are polled, never short-circuited:
        // each keeps its own record, and a skipped poll is a missed change.
        let values_moved = self.poll_dependencies(ctx);
        let shape_changed = self.poll_arg_shape(ctx);
        if values_moved || shape_changed {
            self.run_update(ctx);
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

/// The tint that marks a parameter apart from the nodes it feeds. Pale on
/// both counts: a pin is a label, not a control asking to be pressed.
const PARAM_FILL: Color = Color::rgb(237, 244, 251);
const PARAM_BORDER: Color = theme::ACCENT_MUTED;

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
            padding: theme::SPACE_MD,
            corner_radius: theme::RADIUS_MD,
            fill_color: PARAM_FILL,
            border_width: theme::HAIRLINE,
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

        // A rule divides the surface from the strip below it, so the output
        // reads as this canvas's own footer rather than as something adrift on
        // it.
        let rule_y = origin.y + avail_h - CC_OUT_H;
        Rect {
            size: Vector {
                x: avail_w,
                y: theme::HAIRLINE,
            },
            corner_radius: 0.0,
            fill_color: theme::LINE,
            border: Stroke::NONE,
            stroke_kind: StrokeKind::Inside,
        }
        .paint(
            ctx.ui.painter(),
            ScreenPos {
                x: origin.x,
                y: rule_y,
            },
        );

        // Output pin along the bottom.
        let out_y = rule_y + CC_GAP;
        let mut label = Label::new("output".to_owned());
        label.color = theme::INK_MUTED;
        let label_res = ctx.draw_node(
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
        let label_size = label_res.region().map(|r| r.size()).unwrap_or_default();
        const PORT: f32 = 8.0;
        ctx.draw_workspace_node(
            self.output_port.erase(),
            DrawConstraints {
                pos: ScreenPos {
                    x: origin.x + CC_GAP + label_size.x + theme::SPACE_LG,
                    // Centred against the label's line.
                    y: out_y + (label_size.y - PORT) * 0.5,
                },
                x: Some(AxisConstraint::Exactly(PORT)),
                y: Some(AxisConstraint::Exactly(PORT)),
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
    /**
        What is wrong with the lambda's arguments, if anything.

        A canvas lambda has no script to fail, so this is where a failed
        declaration lands: the result reads as an error rather than as a value,
        and stands for nothing, which fails the checks downstream of it in turn.
    */
    fault: Option<String>,
}

#[utils::dynamic_methods]
impl OutputProxy {
    pub fn build(
        ws: WorkspaceActionHandle,
        canvas: NodeUid<ComputeCanvas>,
    ) -> NodeUid<OutputProxy> {
        ws.insert_node(Self {
            canvas,
            fault: None,
        })
    }
}

#[utils::dynamic_node]
impl Node for OutputProxy {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Lambda Output".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        if let Some(fault) = &self.fault {
            return ctx.draw_node(&ErrorLayout::message(fault.clone()), constraints);
        }
        let ws = ctx.node.workspace;
        let text = ws
            .send_request(self.canvas, OutputConnected)
            .flatten()
            .map(|node| resolve_arg(ws, node).value.display())
            .unwrap_or_else(|| "(no output)".to_owned());
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
    actions: [
        // Say what is wrong with the lambda's arguments, or that nothing is.
        SetOutputFault { message: Option<String> } => (this, s) {
            this.fault = s.message;
        },
    ],
    extern_requests: [
        // The proxy is worth exactly what the pin is wired to — unless the
        // arguments never made it in, in which case it is worth nothing.
        ValueDelegate => (this, _q, ctx): Option<NodeUid> {
            match this.fault {
                Some(_) => None,
                None => ctx.workspace.send_request(this.canvas, OutputConnected).flatten(),
            }
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

    /**
        The arguments as last checked: their shape, and what each was worth.

        A canvas lambda runs no script, so nothing else would ever notice a
        declaration going unmet. This is what keeps the check to the frames
        where something actually changed.
    */
    #[dynamic(skip)]
    seen_args: Transient<CheckedArgs>,
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
            seen_args: Transient::default(),
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
            seen_args: Transient::default(),
        }
    }
}

impl CanvasLambda {
    /**
        Re-check the arguments whenever anything about them changes.

        Still a worker's job even with no script to run: two of the eight kinds
        need the interpreter, and the prelude with it, so what the check found
        comes back as actions rather than as a stalled frame.
    */
    fn poll_arg_types(&self, ctx: NodeContext) {
        let ws = ctx.workspace;
        let declarations = ws
            .send_request(self.args, ArgDeclarations)
            .unwrap_or_default();
        let mut signature: CheckedArgs = Vec::new();
        for (name, port, target) in ws
            .send_request(self.args, DataflowInputs)
            .unwrap_or_default()
        {
            let (kind, detail) = declaration_for(&declarations, port);
            let resolved = target.map(|t| resolve_arg(ws, t));
            // A source still recomputing is not worth checking; wait for it to
            // settle rather than complain about the gap it leaves behind.
            if resolved.as_ref().is_some_and(|r| r.pending) {
                return;
            }
            let version = resolved.map(|r| r.version).unwrap_or(0);
            signature.push((name, target, kind, detail, version));
        }
        if self.seen_args.val().as_ref() == Some(&signature) {
            return;
        }
        self.seen_args.set(signature);

        let specs = arg_specs(ws, self.args);
        let prelude = ws
            .send_request(ws.root().cast::<Desktops>(), PythonPrelude)
            .unwrap_or_default();
        let output = self.output;
        ws.cancel_all_tasks_for(ctx.id);
        ws.submit_task(ComputeTask::new(ctx.id, move || {
            let (handle, actions) = WorkspaceActionHandle::buffered();
            let faults = check_arg_types(&prelude, &specs);
            report_faults(&handle, &specs, &faults);
            handle.submit_action(
                output,
                "Reported the arguments' types",
                SetOutputFault {
                    message: (!faults.is_empty()).then(|| describe(&faults)),
                },
            );
            drop(handle);
            actions.try_iter().collect()
        }));
    }
}

#[utils::dynamic_node]
impl Node for CanvasLambda {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Lambda".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const V_SECTIONS_GAP: f32 = theme::SPACE_MD;
        const OUTER_PADDING: f32 = theme::SPACE_LG;
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

        // The output claims whatever the rest of the node leaves.
        let body = VerticalLayout::filling_last(
            vec![
                LayoutChild::from(self.name),
                LayoutChild::from(self.args),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::from(self.open_button),
                LayoutChild::Node(Arc::new(SectionDivider)),
                LayoutChild::Inspectable(self.output.erase()),
            ],
            V_SECTIONS_GAP,
        );
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(body)),
            padding: OUTER_PADDING,
            corner_radius: theme::RADIUS_LG,
            fill_color: theme::SURFACE,
            border_width: theme::HAIRLINE,
            border_color: theme::LINE,
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
            .send_request(self.open_button.erase(), TakeClicked)
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

        self.poll_arg_types(ctx);
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
