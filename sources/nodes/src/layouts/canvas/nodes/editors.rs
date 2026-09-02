//! Bespoke on-canvas editors for nodes.

use dex_core::prelude::*;
use dex_core::theme;
use egui::{Color32, Stroke as EguiStroke};
use serde::{Deserialize, Serialize};
use utils::Transient;

use super::{CanvasItemBounds, CanvasNodeChild, NudgeCanvasItem};
use crate::scripting::ValueDelegate;
use crate::{
    composites::button::Button,
    layouts::{
        canvas::layout::{RemoveCanvasItem, SwapCanvasItem},
        inspector::{PlacementCommands, menu_button},
        vertical::VerticalLayout,
    },
    primitives::{
        checkbox::{Checkbox, IsChecked},
        color_picker::{ColorPicker, ColorSlot, drop_preview, repicked},
        interaction::{
            ContainsPointer, DragStartPos, InteractionBox, PointerPos, TakeClicked, WasClicked,
            WasDoubleClicked, WasDragged, WasHovered, WasRightClicked,
        },
        shapes::{
            Anchor, GetAnchors, GetFill, GetRadius, GetStroke, HasEndArrow, HasStartArrow,
            IsPathClosed, IsPathFilled, Path, SetAnchors, SetPathArrows, SetPathClosed,
            SetPathFill, SetPathFilled, SetPathStrokeColor, SetRadius,
        },
    },
};

fn chrome_stroke() -> EguiStroke {
    EguiStroke::new(1.0, Color32::from_gray(170))
}

/// A small round grip drawn at a draggable point.
fn paint_grip(ctx: &mut DrawContext, center: ScreenPos, hovered: bool) {
    ctx.ui.painter().circle(
        center.into(),
        4.5,
        Color32::WHITE,
        EguiStroke::new(1.5, Color32::from_gray(if hovered { 80 } else { 150 })),
    );
}

fn dist2(a: ScreenPos, b: ScreenPos) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}

/// Place a fixed-size square sensor centred on a screen point.
fn place_sensor(ctx: &mut DrawContext, sensor: NodeUid, center: ScreenPos, size: f32) {
    let region = ScreenRegion::from_center_size(center, Vector::splat(size));
    ctx.draw_workspace_node(
        sensor,
        DrawConstraints {
            pos: region.min,
            x: Some(AxisConstraint::Exactly(size)),
            y: Some(AxisConstraint::Exactly(size)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: false,
        },
    );
}

/// Generic commands for top-level canvas elements.
fn placement_commands(ws: &Workspace, target: NodeUid) -> NodeUid<PlacementCommands> {
    let size = ws
        .send_request(target, CanvasItemBounds)
        .map(|bounds| bounds.size())
        .unwrap_or(Vector::splat(80.0));
    PlacementCommands::build_for_canvas_item(ws.action_handle(), target, size)
}

fn place_region(ctx: &mut DrawContext, sensor: NodeUid, region: ScreenRegion) {
    ctx.draw_workspace_node(
        sensor,
        DrawConstraints {
            pos: region.min,
            x: Some(AxisConstraint::Exactly(region.size().x)),
            y: Some(AxisConstraint::Exactly(region.size().y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: false,
        },
    );
}

// =====================================================================
// Circle: centre + radius
// =====================================================================

#[utils::dynamic_type]
#[utils::portable]
pub struct CircleEditor {
    child: NodeUid,
    center: Vector,
    pending_center: Transient<Vector>,
    pending_radius: Transient<f32>,
    proximity: NodeUid<InteractionBox>,
    body: NodeUid<InteractionBox>,
    radius_grip: NodeUid<InteractionBox>,
}

#[utils::dynamic_methods]
impl CircleEditor {
    const MIN_RADIUS: f32 = 6.0;

    pub fn build(
        ws: WorkspaceActionHandle,
        child: NodeUid,
        canvas_pos: Vector,
        size: Vector,
    ) -> NodeUid<CircleEditor> {
        ws.insert_node(Self {
            child,
            center: canvas_pos + size / 2.0,
            pending_center: Transient::default(),
            pending_radius: Transient::default(),
            proximity: ws.insert_node(InteractionBox::sensing(true, false, false)),
            body: ws.insert_node(InteractionBox::sensing(true, false, true)),
            radius_grip: ws.insert_node(InteractionBox::sensing(true, false, true)),
        })
    }

    fn radius(&self, ws: &Workspace) -> f32 {
        self.pending_radius
            .val()
            .or_else(|| ws.send_request(self.child, GetRadius))
            .unwrap_or(20.0)
    }
}

#[utils::dynamic_node]
impl Node for CircleEditor {
    fn type_name(&self, ctx: NodeContext) -> String {
        ctx.workspace
            .get_node(self.child)
            .filter(|_| self.child != ctx.id)
            .map(|c| {
                c.type_name(NodeContext {
                    id: self.child,
                    workspace: ctx.workspace,
                })
            })
            .unwrap_or_else(|| "A Circle".to_owned())
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const HANDLE_GRAB: f32 = 7.0;
        let ws = ctx.node.workspace;
        let canvas_origin = ctx.constraints.pos;

        let committed_radius = ws.send_request(self.child, GetRadius).unwrap_or(20.0);
        let center = self.pending_center.val().unwrap_or(self.center);
        let radius = self.radius(ws);
        let screen_center = canvas_origin + center;

        ctx.draw_workspace_node(
            self.child,
            DrawConstraints {
                pos: screen_center - Vector::splat(committed_radius),
                x: Some(AxisConstraint::Exactly(committed_radius * 2.0)),
                y: Some(AxisConstraint::Exactly(committed_radius * 2.0)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        let editing = self.pending_center.val().is_some() || self.pending_radius.val().is_some();
        place_region(
            &mut ctx,
            self.proximity.erase(),
            ScreenRegion::from_center_size(screen_center, Vector::splat((radius + 20.0) * 2.0)),
        );
        let near = editing
            || ws
                .send_request(self.proximity, ContainsPointer)
                .unwrap_or(false);

        let mut moved: Option<Vector> = None;
        let mut resized: Option<f32> = None;
        if near {
            place_region(
                &mut ctx,
                self.body.erase(),
                ScreenRegion::from_center_size(screen_center, Vector::splat(radius * 2.0)),
            );
            if let Some(delta) = ws.send_request(self.body, WasDragged).flatten() {
                moved = Some(center + delta);
            }
            place_sensor(
                &mut ctx,
                self.radius_grip.erase(),
                screen_center + Vector { x: radius, y: 0.0 },
                HANDLE_GRAB * 2.0,
            );
            if let Some(delta) = ws.send_request(self.radius_grip, WasDragged).flatten() {
                resized = Some((radius + delta.x).max(Self::MIN_RADIUS));
            }
        }

        // Commit on release. Bind the pending value to a local first so its read
        // guard drops before `val_mut()` takes the write lock (both = deadlock).
        match moved {
            Some(c) => self.pending_center.set(c),
            None => {
                let pending = *self.pending_center.val();
                if let Some(c) = pending {
                    *self.pending_center.val_mut() = None;
                    ctx.submit_action_for_self::<Self, _>(
                        SetCircleCenter { center: c },
                        "Moved circle",
                    );
                }
            }
        }
        match resized {
            Some(r) => self.pending_radius.set(r),
            None => {
                let pending = *self.pending_radius.val();
                if let Some(r) = pending {
                    *self.pending_radius.val_mut() = None;
                    ws.submit_action(self.child, "Resized circle", SetRadius { radius: r });
                }
            }
        }

        if near {
            ctx.ui.painter().circle(
                screen_center.into(),
                radius,
                Color32::TRANSPARENT,
                chrome_stroke(),
            );
            let grip_hovered = ws
                .send_request(self.radius_grip, WasHovered)
                .unwrap_or(false);
            paint_grip(
                &mut ctx,
                screen_center + Vector { x: radius, y: 0.0 },
                grip_hovered,
            );
            let body_hovered = ws.send_request(self.body, WasHovered).unwrap_or(false);
            paint_grip(&mut ctx, screen_center, body_hovered);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_center_size(
                screen_center,
                Vector::splat(radius * 2.0),
            )),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(CircleEditorMenu::build(ctx.workspace, ctx.id, self.child).erase())
    }

    fn deref_target(&self) -> Option<NodeUid> {
        Some(self.child)
    }

    fn on_delete(&self, ctx: NodeContext) {
        for s in [
            self.child,
            self.proximity.erase(),
            self.body.erase(),
            self.radius_grip.erase(),
        ] {
            ctx.workspace.delete_node(s);
        }
    }
}

defhandlers! { CircleEditor {
    actions: [
        SetCircleCenter { center: Vector } => (this, s) { this.center = s.center; },
    ],
    extern_actions: [
        NudgeCanvasItem => (this, s) { this.center = this.center + s.delta; },
    ],
    extern_requests: [
        CanvasItemBounds => (this, _q, ctx): ScreenRegion {
            let radius = this.radius(ctx.workspace);
            let center = (*this.pending_center.val()).unwrap_or(this.center);
            ScreenRegion::from_center_size(center.to_screen_pos(), Vector::splat(radius * 2.0))
        },
        CanvasNodeChild => (this, _q): NodeUid { this.child },
        ValueDelegate => (this, _q): Option<NodeUid> { Some(this.child) },
    ],
}}

/// The circle editor's inspector: convert to a path, or delete.
#[utils::portable]
pub struct CircleEditorMenu {
    #[uid_ref]
    target: NodeUid<CircleEditor>,
    child: NodeUid,
    column: NodeUid<VerticalLayout>,
    placement: NodeUid<PlacementCommands>,
    convert_button: NodeUid<Button>,
    delete_button: NodeUid<Button>,
}

impl CircleEditorMenu {
    fn build(ws: &Workspace, target: NodeUid, child: NodeUid) -> NodeUid<CircleEditorMenu> {
        let h = ws.action_handle();
        let placement = placement_commands(ws, target);
        let convert_button = menu_button(h.clone(), "Convert to polygon");
        let delete_button = menu_button(h.clone(), "Delete");
        let column = VerticalLayout::build(
            h,
            vec![
                placement.erase(),
                convert_button.erase(),
                delete_button.erase(),
            ],
            2.0,
        );
        ws.insert_node(Self {
            target: target.cast(),
            child,
            column,
            placement,
            convert_button,
            delete_button,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for CircleEditorMenu {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Circle Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);
        let ws = ctx.node.workspace;
        if ws
            .send_request(self.convert_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            // Swap the circle for a path tracing exactly the same outline, in
            // exactly the same place, keeping its colours: nothing moves, but
            // every point of it becomes editable.
            let radius = ws.send_request(self.child, GetRadius).unwrap_or(20.0);
            let fill = ws
                .send_request(self.child, GetFill)
                .unwrap_or(Path::default_fill());
            let stroke = ws
                .send_request(self.child, GetStroke)
                .unwrap_or(Stroke::NONE);
            if let Some(bounds) = ws.send_request(self.target, CanvasItemBounds) {
                let path = Path::circle(Vector::splat(radius), radius, fill, stroke);
                ws.submit_action(
                    ws.root(),
                    "Converted to polygon",
                    SwapCanvasItem {
                        old: self.target.erase(),
                        child: Arc::new(path),
                        pos: bounds.min.to_vector(),
                        size: bounds.size(),
                    },
                );
            }
        }
        if ws
            .send_request(self.delete_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            ws.submit_action(
                ws.root(),
                "Deleted canvas item",
                RemoveCanvasItem {
                    node: self.target.erase(),
                },
            );
        }
        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        ctx.workspace.delete_node(self.placement.erase());
        ctx.workspace.delete_node(self.convert_button.erase());
        ctx.workspace.delete_node(self.delete_button.erase());
    }
}

defhandlers! { CircleEditorMenu {} }

// =====================================================================
// Path: draggable anchors (polygon and line)
// =====================================================================

/// The local-space region spanned by `points`, empty if there are none.
fn bounds_of(points: impl IntoIterator<Item = Vector>) -> ScreenRegion {
    let mut points = points.into_iter();
    let Some(first) = points.next() else {
        return ScreenRegion::from_min_size(ScreenPos::zero(), Vector::ZERO);
    };
    let (mut min, mut max) = (first, first);
    for p in points {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    ScreenRegion::from_min_max(min.to_screen_pos(), max.to_screen_pos())
}

/// The local-space bounding region of an anchor outline (points only).
fn anchor_bounds(anchors: &[Anchor]) -> ScreenRegion {
    bounds_of(anchors.iter().map(|a| a.pos))
}

/// The region the editor's own controls occupy: the vertices plus any Bézier
/// control points, which stick out past the vertex bounds and must still be
/// inside the sensor that hit-tests them.
fn outline_bounds(anchors: &[Anchor]) -> ScreenRegion {
    bounds_of(anchors.iter().flat_map(|a| {
        [
            Some(a.pos),
            a.in_handle.map(|h| a.pos + h),
            a.out_handle.map(|h| a.pos + h),
        ]
        .into_iter()
        .flatten()
    }))
}

/// The point halfway along the segment from `a` to `b`, following the curve
/// when either end carries a handle so the marker sits on the drawn border
/// rather than on the straight chord between the vertices.
fn edge_midpoint(a: &Anchor, b: &Anchor) -> Vector {
    match (a.out_handle, b.in_handle) {
        (None, None) => (a.pos + b.pos) / 2.0,
        (out, inc) => {
            // A cubic Bézier at t = 0.5 is (p0 + 3c1 + 3c2 + p3) / 8.
            let c1 = a.pos + out.unwrap_or_default();
            let c2 = b.pos + inc.unwrap_or_default();
            Vector {
                x: (a.pos.x + 3.0 * c1.x + 3.0 * c2.x + b.pos.x) / 8.0,
                y: (a.pos.y + 3.0 * c1.y + 3.0 * c2.y + b.pos.y) / 8.0,
            }
        }
    }
}

/// `anchors` with the edge starting at `i` split in half at its midpoint.
///
/// A curved edge is subdivided (de Casteljau at t = 0.5), which rewrites the
/// neighbouring handles so the outline keeps exactly the shape it had; a
/// straight one just gains a corner.
fn split_edge(anchors: &[Anchor], i: usize) -> Vec<Anchor> {
    let n = anchors.len();
    let j = (i + 1) % n;
    let mut out = anchors.to_vec();
    let (a, b) = (&anchors[i], &anchors[j]);
    if a.out_handle.is_none() && b.in_handle.is_none() {
        out.insert(i + 1, Anchor::corner((a.pos + b.pos) / 2.0));
        return out;
    }
    let c1 = a.pos + a.out_handle.unwrap_or_default();
    let c2 = b.pos + b.in_handle.unwrap_or_default();
    let m0 = (a.pos + c1) / 2.0;
    let m1 = (c1 + c2) / 2.0;
    let m2 = (c2 + b.pos) / 2.0;
    let q0 = (m0 + m1) / 2.0;
    let q1 = (m1 + m2) / 2.0;
    let mid = (q0 + q1) / 2.0;
    out[i].out_handle = Some(m0 - a.pos);
    out[j].in_handle = Some(m2 - b.pos);
    out.insert(
        i + 1,
        Anchor {
            pos: mid,
            in_handle: Some(q0 - mid),
            out_handle: Some(q1 - mid),
        },
    );
    out
}

/// Return `anchors` with vertex `i` toggled between a straight corner and a
/// smooth (mirrored-handle) vertex tangent to its neighbours.
fn toggled_vertex(anchors: &[Anchor], i: usize) -> Vec<Anchor> {
    let n = anchors.len();
    let mut out = anchors.to_vec();
    let a = &anchors[i];
    if a.in_handle.is_some() || a.out_handle.is_some() {
        out[i].in_handle = None;
        out[i].out_handle = None;
    } else {
        let prev = anchors[(i + n - 1) % n].pos;
        let next = anchors[(i + 1) % n].pos;
        let dir = next - prev;
        let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
        let handle = if len > 1e-3 {
            let scale = (len / 3.0).min(80.0) / len;
            Vector {
                x: dir.x * scale,
                y: dir.y * scale,
            }
        } else {
            Vector { x: 24.0, y: 0.0 }
        };
        out[i].out_handle = Some(handle);
        out[i].in_handle = Some(Vector {
            x: -handle.x,
            y: -handle.y,
        });
    }
    out
}

fn nearest_vertex(anchors: &[Anchor], origin: ScreenPos, p: ScreenPos, r: f32) -> Option<usize> {
    let r2 = r * r;
    let mut best = None;
    let mut bd = r2;
    for (i, a) in anchors.iter().enumerate() {
        let d = dist2(origin + a.pos, p);
        if d <= bd {
            bd = d;
            best = Some(i);
        }
    }
    best
}

/// The edge whose midpoint is nearest `p`, by its starting vertex.
fn nearest_edge(
    anchors: &[Anchor],
    origin: ScreenPos,
    closed: bool,
    p: ScreenPos,
    r: f32,
) -> Option<usize> {
    let n = anchors.len();
    let edges = if closed { n } else { n.saturating_sub(1) };
    let r2 = r * r;
    let mut best = None;
    let mut bd = r2;
    for i in 0..edges {
        let mid = edge_midpoint(&anchors[i], &anchors[(i + 1) % n]);
        let d = dist2(origin + mid, p);
        if d <= bd {
            bd = d;
            best = Some(i);
        }
    }
    best
}

/// What a path drag is manipulating, decided on the first drag frame.
#[derive(Clone, Copy, Serialize, Deserialize)]
enum Grab {
    Nothing,
    Anchor(usize),
    In(usize),
    Out(usize),
}

#[utils::dynamic_type]
#[utils::portable]
pub struct PathEditor {
    child: NodeUid,
    pos: Vector,
    /// Line semantics (two ends, arrows, no add/remove) vs. polygon.
    is_line: bool,
    /// Point-edit mode. A closed polygon starts off (double-click to enter);
    /// an open path starts on, and a line is always editable whatever this
    /// says, since it has nothing else to edit.
    editable: bool,

    pending_pos: Transient<Vector>,
    pending_anchors: Transient<Vec<Anchor>>,
    grab: Transient<Grab>,

    proximity: NodeUid<InteractionBox>,
    move_sensor: NodeUid<InteractionBox>,
    /// Covers the shape; hit-testing picks vertices, handles, edges or move.
    body: NodeUid<InteractionBox>,
}

#[utils::dynamic_methods]
impl PathEditor {
    const VERT_GRAB: f32 = 9.0;
    const HANDLE_GRAB: f32 = 9.0;
    const EDGE_GRAB: f32 = 10.0;

    pub fn build(
        ws: WorkspaceActionHandle,
        child: NodeUid,
        pos: Vector,
        is_line: bool,
        editable: bool,
    ) -> NodeUid<PathEditor> {
        ws.insert_node(Self {
            child,
            pos,
            is_line,
            editable,
            pending_pos: Transient::default(),
            pending_anchors: Transient::default(),
            grab: Transient::default(),
            proximity: ws.insert_node(InteractionBox::sensing(true, false, false)),
            move_sensor: ws.insert_node(InteractionBox::sensing(true, false, true)),
            body: ws.insert_node(InteractionBox::sensing(true, true, true)),
        })
    }
}

#[utils::dynamic_node]
impl Node for PathEditor {
    fn type_name(&self, ctx: NodeContext) -> String {
        ctx.workspace
            .get_node(self.child)
            .filter(|_| self.child != ctx.id)
            .map(|c| {
                c.type_name(NodeContext {
                    id: self.child,
                    workspace: ctx.workspace,
                })
            })
            .unwrap_or_else(|| if self.is_line { "A Line" } else { "A Path" }.to_owned())
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let ws = ctx.node.workspace;
        let canvas_origin = ctx.constraints.pos;

        let base_anchors: Vec<Anchor> = ws.send_request(self.child, GetAnchors).unwrap_or_default();
        let closed = ws.send_request(self.child, IsPathClosed).unwrap_or(false);
        let anchors: Vec<Anchor> = (*self.pending_anchors.val())
            .clone()
            .unwrap_or_else(|| base_anchors.clone());
        let display_pos = self.pending_pos.val().unwrap_or(self.pos);
        let shape_origin = canvas_origin + display_pos;
        let editable = self.is_line || self.editable;

        // Draw the wrapped path at its committed anchors (child owns colours).
        let child_bounds = anchor_bounds(&base_anchors);
        ctx.draw_workspace_node(
            self.child,
            DrawConstraints {
                pos: shape_origin,
                x: Some(AxisConstraint::Exactly(child_bounds.size().x.max(1.0))),
                y: Some(AxisConstraint::Exactly(child_bounds.size().y.max(1.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        // Everything the editor draws and grabs, control points included.
        let local = outline_bounds(&anchors);
        let reveal =
            ScreenRegion::from_min_size(shape_origin + local.min.to_vector(), local.size())
                .expand(28.0);
        place_region(&mut ctx, self.proximity.erase(), reveal);
        let near = self.pending_pos.val().is_some()
            || self.pending_anchors.val().is_some()
            || ws
                .send_request(self.proximity, ContainsPointer)
                .unwrap_or(false);

        let mut any_drag = false;
        let mut working = anchors.clone();

        // Body sensor covers the outline (+margin), under the move handle.
        let body_region =
            ScreenRegion::from_min_size(shape_origin + local.min.to_vector(), local.size())
                .expand(8.0);
        place_region(&mut ctx, self.body.erase(), body_region);

        if let Some(delta) = ws.send_request(self.body, WasDragged).flatten() {
            any_drag = true;
            // Read the grab into a local first: the guard from `val()` would
            // otherwise live for the whole `match`, and `set()` below would
            // block on it forever.
            let existing_grab = *self.grab.val();
            let grab = match existing_grab {
                Some(g) => g,
                None => {
                    let press = ws.send_request(self.body, DragStartPos).flatten();
                    let g = press
                        .map(|p| {
                            if editable {
                                if let Some(i) =
                                    nearest_vertex(&anchors, shape_origin, p, Self::VERT_GRAB)
                                {
                                    return Grab::Anchor(i);
                                }
                                for (i, a) in anchors.iter().enumerate() {
                                    if let Some(o) = a.out_handle
                                        && dist2(shape_origin + a.pos + o, p)
                                            <= Self::HANDLE_GRAB * Self::HANDLE_GRAB
                                    {
                                        return Grab::Out(i);
                                    }
                                    if let Some(inh) = a.in_handle
                                        && dist2(shape_origin + a.pos + inh, p)
                                            <= Self::HANDLE_GRAB * Self::HANDLE_GRAB
                                    {
                                        return Grab::In(i);
                                    }
                                }
                            }
                            Grab::Nothing
                        })
                        .unwrap_or(Grab::Nothing);
                    self.grab.set(g);
                    g
                }
            };
            match grab {
                Grab::Nothing => {}
                Grab::Anchor(i) if i < working.len() => {
                    working[i].pos = working[i].pos + delta;
                    self.pending_anchors.set(working.clone());
                }
                Grab::Out(i) if i < working.len() => {
                    let nh = working[i].out_handle.unwrap_or(Vector::ZERO) + delta;
                    working[i].out_handle = Some(nh);
                    working[i].in_handle = Some(Vector { x: -nh.x, y: -nh.y });
                    self.pending_anchors.set(working.clone());
                }
                Grab::In(i) if i < working.len() => {
                    let nh = working[i].in_handle.unwrap_or(Vector::ZERO) + delta;
                    working[i].in_handle = Some(nh);
                    working[i].out_handle = Some(Vector { x: -nh.x, y: -nh.y });
                    self.pending_anchors.set(working.clone());
                }
                _ => {}
            }
        }

        // Move handle (always available).
        if near {
            let handle_center = shape_origin + local.min.to_vector() - Vector { x: 16.0, y: 16.0 };
            place_sensor(&mut ctx, self.move_sensor.erase(), handle_center, 16.0);
            if let Some(delta) = ws.send_request(self.move_sensor, WasDragged).flatten() {
                any_drag = true;
                self.pending_pos.set(display_pos + delta);
            }
        }

        // Double-click to start editing points; once editing, a double-click on
        // a vertex switches it between a corner and a smooth curve.
        if ws
            .send_request(self.body, WasDoubleClicked)
            .unwrap_or(false)
        {
            if !editable {
                ctx.submit_action_for_self::<Self, _>(
                    SetPathEditable { on: true },
                    "Edited points",
                );
            } else if let Some(p) = ws.send_request(self.body, PointerPos).flatten()
                && let Some(i) = nearest_vertex(&anchors, shape_origin, p, Self::VERT_GRAB)
            {
                ws.submit_action(
                    self.child,
                    "Converted vertex",
                    SetAnchors {
                        anchors: toggled_vertex(&anchors, i),
                    },
                );
            }
        }

        // Polygon-only: click an edge to add a vertex, right-click to delete one.
        if editable && !self.is_line {
            if ws.send_request(self.body, WasClicked).unwrap_or(false)
                && let Some(p) = ws.send_request(self.body, PointerPos).flatten()
                && nearest_vertex(&anchors, shape_origin, p, Self::VERT_GRAB).is_none()
                && let Some(edge) = nearest_edge(&anchors, shape_origin, closed, p, Self::EDGE_GRAB)
            {
                ws.submit_action(
                    self.child,
                    "Added vertex",
                    SetAnchors {
                        anchors: split_edge(&anchors, edge),
                    },
                );
            }
            if ws.send_request(self.body, WasRightClicked).unwrap_or(false)
                && let Some(p) = ws.send_request(self.body, PointerPos).flatten()
                && let Some(i) = nearest_vertex(&anchors, shape_origin, p, Self::VERT_GRAB)
            {
                let min = if closed { 3 } else { 2 };
                if anchors.len() > min {
                    let mut new = anchors.clone();
                    new.remove(i);
                    ws.submit_action(self.child, "Removed vertex", SetAnchors { anchors: new });
                }
            }
        }

        // Commit whatever was dragged once the drag ends.
        if !any_drag {
            let pending_anchors = (*self.pending_anchors.val()).clone();
            if let Some(pa) = pending_anchors {
                *self.pending_anchors.val_mut() = None;
                ws.submit_action(self.child, "Edited path", SetAnchors { anchors: pa });
            }
            let pending_pos = *self.pending_pos.val();
            if let Some(p) = pending_pos {
                *self.pending_pos.val_mut() = None;
                ctx.submit_action_for_self::<Self, _>(SetPathPos { pos: p }, "Moved path");
            }
            *self.grab.val_mut() = None;
        }

        // Decorations.
        if near {
            Path::unfilled(anchors.clone(), closed, Stroke::new(1.5, theme::INK_MUTED))
                .paint(ctx.ui.painter(), shape_origin);

            let hover = ws.send_request(self.body, PointerPos).flatten();
            if editable {
                // Edge midpoint markers (polygon only), signalling "click to add".
                if !self.is_line {
                    let n = anchors.len();
                    let edges = if closed { n } else { n.saturating_sub(1) };
                    for i in 0..edges {
                        let mid = shape_origin + edge_midpoint(&anchors[i], &anchors[(i + 1) % n]);
                        ctx.ui.painter().circle(
                            mid.into(),
                            3.0,
                            Color32::from_gray(245),
                            EguiStroke::new(1.0, Color32::from_gray(160)),
                        );
                    }
                }
                for (i, a) in anchors.iter().enumerate() {
                    let smooth = a.in_handle.is_some() || a.out_handle.is_some();
                    let apos = shape_origin + a.pos;
                    for h in [a.out_handle, a.in_handle].into_iter().flatten() {
                        let hc = apos + h;
                        ctx.ui.painter().line(
                            vec![apos.into(), hc.into()],
                            EguiStroke::new(1.0, Color32::from_gray(180)),
                        );
                        ctx.ui
                            .painter()
                            .circle_filled(hc.into(), 3.5, Color32::from_gray(120));
                    }
                    let hovered = hover.is_some_and(|p| {
                        dist2(apos, p) <= Self::VERT_GRAB * Self::VERT_GRAB
                            && nearest_vertex(&anchors, shape_origin, p, Self::VERT_GRAB) == Some(i)
                    });
                    ctx.ui.painter().circle(
                        apos.into(),
                        if hovered { 6.0 } else { 5.0 },
                        if smooth {
                            Color32::from_gray(90)
                        } else {
                            Color32::WHITE
                        },
                        EguiStroke::new(1.5, Color32::from_gray(80)),
                    );
                }
            }

            let handle_center = shape_origin + local.min.to_vector() - Vector { x: 16.0, y: 16.0 };
            let hovered = ws
                .send_request(self.move_sensor, WasHovered)
                .unwrap_or(false);
            ctx.ui.painter().circle(
                handle_center.into(),
                6.0,
                if hovered {
                    Color32::from_gray(210)
                } else {
                    Color32::from_gray(235)
                },
                chrome_stroke(),
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                shape_origin + local.min.to_vector(),
                local.size(),
            )),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(PathEditorMenu::build(ctx.workspace, ctx.id, self.child, self.is_line).erase())
    }

    fn deref_target(&self) -> Option<NodeUid> {
        Some(self.child)
    }

    fn on_delete(&self, ctx: NodeContext) {
        for s in [
            self.child,
            self.proximity.erase(),
            self.move_sensor.erase(),
            self.body.erase(),
        ] {
            ctx.workspace.delete_node(s);
        }
    }
}

defhandlers! { PathEditor {
    actions: [
        SetPathPos { pos: Vector } => (this, s) { this.pos = s.pos; },
        SetPathEditable { on: bool } => (this, s) { this.editable = s.on; },
    ],
    extern_actions: [
        NudgeCanvasItem => (this, s) { this.pos = this.pos + s.delta; },
    ],
    requests: [
        // Whether point-edit mode is on (read to seed the inspector's toggle).
        PathEditable => (this, _q): bool { this.editable },
        // The canvas-space origin the wrapped path's anchors are measured from,
        // so a replacement item can be placed exactly where this one sits.
        PathAnchorOrigin => (this, _q): Vector { (*this.pending_pos.val()).unwrap_or(this.pos) },
    ],
    extern_requests: [
        CanvasItemBounds => (this, _q, ctx): ScreenRegion {
            let anchors: Vec<Anchor> =
                ctx.workspace.send_request(this.child, GetAnchors).unwrap_or_default();
            let pos = (*this.pending_pos.val()).unwrap_or(this.pos);
            let local = outline_bounds(&anchors).expand(6.0);
            ScreenRegion::from_min_size((pos + local.min.to_vector()).to_screen_pos(), local.size())
        },
        CanvasNodeChild => (this, _q): NodeUid { this.child },
        ValueDelegate => (this, _q): Option<NodeUid> { Some(this.child) },
    ],
}}

/// The tick box's state, when it no longer matches the value it stands for.
///
/// The two are polled against each other rather than the box handing over a
/// one-shot toggle: nothing is consumed, so a frame that does not get to look
/// simply tries again on the next, and the value written is the one the box is
/// showing rather than whatever a queued action has yet to catch up to.
fn changed(ws: &Workspace, tick: NodeUid<Checkbox>, actual: bool) -> Option<bool> {
    let shown = ws.send_request(tick, IsChecked)?;
    (shown != actual).then_some(shown)
}

/// The path editor's inspector. Its controls differ for lines and polygons, so
/// it is rebuilt from the editor's current mode each time it opens.
#[utils::portable]
pub struct PathEditorMenu {
    #[uid_ref]
    target: NodeUid<PathEditor>,
    child: NodeUid,
    is_line: bool,
    column: NodeUid<VerticalLayout>,
    placement: NodeUid<PlacementCommands>,
    delete_button: NodeUid<Button>,
    border_picker: NodeUid<ColorPicker>,
    /// Arrowheads are drawn only on an open path, but offered on any path: a
    /// polygon can be opened, and should not have to be reopened to be armed.
    start_arrow_check: NodeUid<Checkbox>,
    end_arrow_check: NodeUid<Checkbox>,
    // Polygon controls.
    editable_check: Option<NodeUid<Checkbox>>,
    closed_check: Option<NodeUid<Checkbox>>,
    filled_check: Option<NodeUid<Checkbox>>,
    fill_picker: Option<NodeUid<ColorPicker>>,
    // Line controls.
    convert_button: Option<NodeUid<Button>>,
}

impl PathEditorMenu {
    fn build(
        ws: &Workspace,
        target: NodeUid,
        child: NodeUid,
        is_line: bool,
    ) -> NodeUid<PathEditorMenu> {
        let h = ws.action_handle();
        let placement = placement_commands(ws, target);
        let stroke = ws
            .send_request(child, GetStroke)
            .unwrap_or(Stroke::new(2.0, Color::BLACK));
        let border_picker = ColorPicker::build(h.clone(), "Border".to_owned(), stroke.color);
        let delete_button = menu_button(h.clone(), "Delete");

        let start = ws.send_request(child, HasStartArrow).unwrap_or(false);
        let end = ws.send_request(child, HasEndArrow).unwrap_or(false);
        let start_arrow_check = Checkbox::build(h.clone(), "Start arrow".to_owned(), start);
        let end_arrow_check = Checkbox::build(h.clone(), "End arrow".to_owned(), end);

        let mut rows: Vec<NodeUid> = vec![placement.erase()];
        let mut editable_check = None;
        let mut closed_check = None;
        let mut filled_check = None;
        let mut fill_picker = None;
        let mut convert_button = None;

        if is_line {
            let cv = menu_button(h.clone(), "Convert to polygon");
            rows.push(start_arrow_check.erase());
            rows.push(end_arrow_check.erase());
            rows.push(border_picker.erase());
            rows.push(cv.erase());
            convert_button = Some(cv);
        } else {
            let editable = ws.send_request(target, PathEditable).unwrap_or(false);
            let closed = ws.send_request(child, IsPathClosed).unwrap_or(false);
            let filled = ws.send_request(child, IsPathFilled).unwrap_or(false);
            let fill = ws.send_request(child, GetFill).unwrap_or(Color::WHITE);
            let ed = Checkbox::build(h.clone(), "Edit points".to_owned(), editable);
            let cl = Checkbox::build(h.clone(), "Closed".to_owned(), closed);
            let fi = Checkbox::build(h.clone(), "Filled".to_owned(), filled);
            let fp = ColorPicker::build(h.clone(), "Fill".to_owned(), fill);
            rows.push(ed.erase());
            rows.push(cl.erase());
            rows.push(fi.erase());
            rows.push(fp.erase());
            rows.push(border_picker.erase());
            rows.push(start_arrow_check.erase());
            rows.push(end_arrow_check.erase());
            editable_check = Some(ed);
            closed_check = Some(cl);
            filled_check = Some(fi);
            fill_picker = Some(fp);
        }
        rows.push(delete_button.erase());

        let column = VerticalLayout::build(h, rows, 3.0);
        ws.insert_node(Self {
            target: target.cast(),
            child,
            is_line,
            column,
            placement,
            delete_button,
            border_picker,
            editable_check,
            closed_check,
            filled_check,
            fill_picker,
            start_arrow_check,
            end_arrow_check,
            convert_button,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for PathEditorMenu {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Path Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);
        let ws = ctx.node.workspace;
        let child = self.child;

        // The picker shows its colour on the path itself while the gesture is
        // going, and is committed once for the whole drag when it ends.
        if let Some(stroke) = ws.send_request(child, GetStroke)
            && let Some(color) = repicked(
                ws,
                self.border_picker,
                child,
                ColorSlot::Stroke,
                stroke.color,
            )
        {
            ws.submit_action(child, "Set border colour", SetPathStrokeColor { color });
        }

        // Both ends travel in one action, so poll them together. A box that
        // cannot be read counts as agreeing, so an unanswered request never
        // leaves the two sides disagreeing forever.
        if let Some(has_start) = ws.send_request(child, HasStartArrow)
            && let Some(has_end) = ws.send_request(child, HasEndArrow)
        {
            let start = ws
                .send_request(self.start_arrow_check, IsChecked)
                .unwrap_or(has_start);
            let end = ws
                .send_request(self.end_arrow_check, IsChecked)
                .unwrap_or(has_end);
            if start != has_start || end != has_end {
                ws.submit_action(child, "Set arrows", SetPathArrows { start, end });
            }
        }
        if let Some(cv) = self.convert_button
            && ws.send_request(cv.erase(), TakeClicked).unwrap_or(false)
        {
            // Two points are a line however they are edited, so the swap
            // adds a third at the middle of the span. It sits on the line,
            // so nothing moves on screen — but the shape now edits as a
            // polygon, with that midpoint there to drag out. Closing and
            // filling it are left to the inspector, rather than inventing
            // an outline the user did not draw.
            let anchors: Vec<Anchor> = ws.send_request(child, GetAnchors).unwrap_or_default();
            let stroke = ws
                .send_request(child, GetStroke)
                .unwrap_or(Stroke::new(2.0, Color::BLACK));
            let pos = ws
                .send_request(self.target, PathAnchorOrigin)
                .unwrap_or(Vector::ZERO);
            let mut polygon = Path::open_through(split_edge(&anchors, 0), stroke);
            polygon.start_arrow = ws.send_request(child, HasStartArrow).unwrap_or(false);
            polygon.end_arrow = ws.send_request(child, HasEndArrow).unwrap_or(false);
            let size = anchor_bounds(&polygon.anchors).size();
            ws.submit_action(
                ws.root(),
                "Converted to polygon",
                SwapCanvasItem {
                    old: self.target.erase(),
                    child: Arc::new(polygon),
                    pos,
                    size,
                },
            );
        }

        if let Some(ed) = self.editable_check
            && let Some(editing) = ws.send_request(self.target, PathEditable)
            && let Some(on) = changed(ws, ed, editing)
        {
            ws.submit_action(
                self.target.erase(),
                "Toggled point editing",
                SetPathEditable { on },
            );
        }
        if let Some(cl) = self.closed_check
            && let Some(closed) = ws.send_request(child, IsPathClosed)
            && let Some(on) = changed(ws, cl, closed)
        {
            ws.submit_action(child, "Toggled closed", SetPathClosed { closed: on });
        }
        if let Some(fi) = self.filled_check
            && let Some(filled) = ws.send_request(child, IsPathFilled)
            && let Some(on) = changed(ws, fi, filled)
        {
            ws.submit_action(child, "Toggled filled", SetPathFilled { filled: on });
        }
        if let Some(fp) = self.fill_picker
            && let Some(fill) = ws.send_request(child, GetFill)
            && let Some(color) = repicked(ws, fp, child, ColorSlot::Fill, fill)
        {
            ws.submit_action(child, "Set fill colour", SetPathFill { color });
        }

        if ws
            .send_request(self.delete_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            ws.submit_action(
                ws.root(),
                "Deleted canvas item",
                RemoveCanvasItem {
                    node: self.target.erase(),
                },
            );
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        // Closing mid-gesture would otherwise leave a preview standing, with
        // nothing left running to clear it.
        for slot in [ColorSlot::Fill, ColorSlot::Stroke] {
            drop_preview(ctx.workspace, self.child, slot);
        }
        ctx.workspace.delete_node(self.column.erase());
        for c in [
            Some(self.placement.erase()),
            Some(self.delete_button.erase()),
            Some(self.border_picker.erase()),
            Some(self.start_arrow_check.erase()),
            Some(self.end_arrow_check.erase()),
            self.editable_check.map(|n| n.erase()),
            self.closed_check.map(|n| n.erase()),
            self.filled_check.map(|n| n.erase()),
            self.fill_picker.map(|n| n.erase()),
            self.convert_button.map(|n| n.erase()),
        ]
        .into_iter()
        .flatten()
        {
            ctx.workspace.delete_node(c);
        }
    }
}

defhandlers! { PathEditorMenu {} }

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vector {
        Vector { x, y }
    }

    /// A cubic Bézier evaluated at `t`, to check the editor's curve maths
    /// against something derived independently.
    fn bezier(p0: Vector, c1: Vector, c2: Vector, p3: Vector, t: f32) -> Vector {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        v(
            a * p0.x + b * c1.x + c * c2.x + d * p3.x,
            a * p0.y + b * c1.y + c * c2.y + d * p3.y,
        )
    }

    fn close(a: Vector, b: Vector) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn a_straight_edge_has_its_marker_on_the_chord() {
        let a = Anchor::corner(v(0.0, 0.0));
        let b = Anchor::corner(v(100.0, 40.0));
        assert!(close(edge_midpoint(&a, &b), v(50.0, 20.0)));
    }

    /// The marker for a curved edge belongs on the curve the border actually
    /// draws, not on the straight chord between its vertices.
    #[test]
    fn a_curved_edge_has_its_marker_on_the_curve() {
        let a = Anchor::smooth(v(0.0, 0.0), v(0.0, -80.0));
        let b = Anchor {
            pos: v(100.0, 0.0),
            in_handle: Some(v(0.0, -80.0)),
            out_handle: None,
        };
        let mid = edge_midpoint(&a, &b);
        let on_curve = bezier(
            a.pos,
            a.pos + a.out_handle.unwrap(),
            b.pos + b.in_handle.unwrap(),
            b.pos,
            0.5,
        );
        assert!(
            close(mid, on_curve),
            "({}, {}) is not on the curve",
            mid.x,
            mid.y
        );
        assert!(
            (mid.y + 60.0).abs() < 1e-3,
            "the curve bulges 60 clear of the chord, which is flat at y = 0"
        );
    }

    #[test]
    fn splitting_a_straight_edge_just_adds_a_corner() {
        let anchors = vec![
            Anchor::corner(v(0.0, 0.0)),
            Anchor::corner(v(100.0, 0.0)),
            Anchor::corner(v(100.0, 100.0)),
        ];
        let split = split_edge(&anchors, 0);
        assert_eq!(split.len(), 4);
        assert!(close(split[1].pos, v(50.0, 0.0)));
        assert!(split[1].in_handle.is_none() && split[1].out_handle.is_none());
    }

    /// Adding a point to a curved edge must not deform it: subdividing keeps
    /// every point of the original curve where it was.
    #[test]
    fn splitting_a_curved_edge_keeps_the_curve() {
        let a = Anchor::smooth(v(0.0, 0.0), v(40.0, -60.0));
        let b = Anchor {
            pos: v(100.0, 0.0),
            in_handle: Some(v(-40.0, -60.0)),
            out_handle: None,
        };
        let (c1, c2) = (a.pos + a.out_handle.unwrap(), b.pos + b.in_handle.unwrap());
        let split = split_edge(&[a.clone(), b.clone()], 0);
        assert_eq!(split.len(), 3);
        assert!(close(split[1].pos, edge_midpoint(&a, &b)));

        // The first half, sampled at t, must match the original at t / 2.
        let (h0, h1) = (&split[0], &split[1]);
        for step in 1..4 {
            let t = step as f32 / 4.0;
            let half = bezier(
                h0.pos,
                h0.pos + h0.out_handle.unwrap(),
                h1.pos + h1.in_handle.unwrap(),
                h1.pos,
                t,
            );
            let whole = bezier(a.pos, c1, c2, b.pos, t / 2.0);
            assert!(
                close(half, whole),
                "at t={t}: ({}, {}) != ({}, {})",
                half.x,
                half.y,
                whole.x,
                whole.y
            );
        }
    }
}
