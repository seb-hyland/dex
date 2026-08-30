pub mod shapes;

use dex_core::prelude::*;
use egui::{Color32, Stroke};
use utils::Transient;

use crate::composites::button::Button;
use crate::layouts::canvas::layout::RemoveCanvasItem;
use crate::layouts::inspector::PlacementCommands;
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::{
    interaction::{ContainsPointer, InteractionBox, TakeClicked, WasDragged, WasHovered},
    shapes::Rect,
    text::Label,
};
use crate::scripting::ValueDelegate;

#[utils::dynamic_type]
#[utils::portable]
pub struct CanvasNode {
    child: NodeUid,
    /// Top-left position of this node in canvas space and size.
    pub committed: ConstraintsTuple,
    /// Uncommitted preview accumulated during interaction.
    pending: Transient<ConstraintsTuple>,

    /// Hover sensor covering the node + margin.
    proximity: NodeUid<InteractionBox>,
    /// Drag/hover sensor for this item's move handle.
    move_sensor: NodeUid<InteractionBox>,
    /// The eight resize grips: tr, bl, br, top, bottom, left, right.
    grips: [NodeUid<InteractionBox>; 7],
}

#[utils::dynamic_methods]
impl CanvasNode {
    /// Build a canvas node (wrapping `child`) and its sensors into `ws`.
    pub fn build(
        ws: WorkspaceActionHandle,
        child: NodeUid,
        canvas_pos: Vector,
        size: Vector,
    ) -> NodeUid<CanvasNode> {
        let sensor = |kind| ws.insert_node(kind);

        ws.insert_node(Self {
            child,
            committed: ConstraintsTuple {
                pos: canvas_pos,
                size,
            },
            pending: Transient::default(),
            proximity: sensor(InteractionBox::sensing(true, false, false)),
            move_sensor: sensor(InteractionBox::sensing(true, false, true)),
            grips: std::array::from_fn(|_| sensor(InteractionBox::sensing(false, false, true))),
        })
    }
}

/// What a canvas item adds to the inspector.
#[utils::portable]
pub struct CanvasNodeInspector {
    #[uid_ref]
    target: NodeUid<CanvasNode>,
    column: NodeUid<VerticalLayout>,
    delete_button: NodeUid<Button>,
}

impl CanvasNodeInspector {
    fn build(
        ctx: NodeContext,
        target: NodeUid<CanvasNode>,
        child: NodeUid,
        size: Vector,
    ) -> NodeUid<CanvasNodeInspector> {
        let delete_button = Button::build(
            ctx.workspace.action_handle(),
            Label::new("Delete".to_owned()),
        );
        let placement =
            PlacementCommands::build(ctx.workspace.action_handle(), target.erase(), size);
        let child_ctx = NodeContext {
            id: child,
            workspace: ctx.workspace,
        };
        let target_inspector = ctx
            .workspace
            .get_node(child)
            .filter(|_| child != target.erase())
            .and_then(|child_node| child_node.build_inspector(child_ctx));
        let column = VerticalLayout::build(
            ctx.workspace.action_handle(),
            [
                Some(placement.erase()),
                Some(delete_button.erase()),
                target_inspector,
            ]
            .into_iter()
            .flatten()
            .collect(),
            2.0,
        );
        ctx.workspace.insert_node(Self {
            target,
            column,
            delete_button,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for CanvasNodeInspector {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Item Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        // Taken, so a command fires once: these rows stop being drawn the
        // moment the menu closes, and a plain read would repeat the last click.
        let taken = |button: NodeUid<Button>| {
            ws.send_request(button.erase(), TakeClicked)
                .unwrap_or(false)
        };
        let root = ws.root();
        let target = self.target;

        if taken(self.delete_button) {
            ws.submit_action(
                root,
                "Deleted canvas node",
                RemoveCanvasItem { node: target },
            );
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        ctx.workspace.delete_node(self.delete_button.erase());
    }
}

defhandlers! { CanvasNodeInspector {} }

#[utils::dynamic_node]
impl Node for CanvasNode {
    /// An item is named for what it frames, not for the frame.
    fn type_name(&self, ctx: NodeContext) -> String {
        let child_ctx = NodeContext {
            id: self.child,
            workspace: ctx.workspace,
        };
        ctx.workspace
            .get_node(self.child)
            // A child naming itself after this node would loop forever.
            .filter(|_| self.child != ctx.id)
            .map(|child| child.type_name(child_ctx))
            .unwrap_or_else(|| "A Canvas Node".to_owned())
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const GRAB_RADIUS: f32 = 8.0;
        const GRIP_RADIUS: f32 = 4.0;
        const MIN_SIZE: f32 = 24.0;
        const HANDLE_SIZE: Vector = Vector { x: 12.0, y: 18.0 };
        // Beyond the inspector's lens, which sits at 22.
        const HANDLE_OFFSET: f32 = 22.0;
        const VISIBILITY_MARGIN: f32 = 16.0;

        // Muted palette, matching the discreet canvas affordances.
        let outline_stroke = Stroke::new(1.0, Color32::from_gray(200));
        let grip_fill = Color32::WHITE;
        let grip_stroke = Stroke::new(1.0, Color32::from_gray(165));
        let ghost_fill = Color32::from_rgba_unmultiplied(130, 130, 130, 26);
        let ghost_stroke = Stroke::new(1.0, Color32::from_gray(150));

        let canvas_origin = ctx.constraints.pos;

        let display = (*self.pending.val()).unwrap_or(self.committed);
        let display_tl = canvas_origin + display.pos;
        let handle_offset = Vector {
            x: HANDLE_OFFSET,
            y: 0.0,
        };

        ctx.draw_workspace_node(
            self.child,
            DrawConstraints {
                pos: display_tl,
                x: Some(AxisConstraint::Exactly(display.size.x)),
                y: Some(AxisConstraint::Exactly(display.size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

        // Reveal handles only when the cursor is near the node or a drag is already underway.
        let near_bounds = ScreenRegion::from_min_size(display_tl, display.size).union(
            ScreenRegion::from_min_size(display_tl - handle_offset, HANDLE_SIZE),
        );
        ctx.draw_workspace_node(
            self.proximity.erase(),
            DrawConstraints {
                pos: near_bounds.min - Vector::splat(VISIBILITY_MARGIN),
                x: Some(AxisConstraint::Exactly(
                    near_bounds.size().x + 2.0 * VISIBILITY_MARGIN,
                )),
                y: Some(AxisConstraint::Exactly(
                    near_bounds.size().y + 2.0 * VISIBILITY_MARGIN,
                )),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        let near = ctx
            .node
            .workspace
            .send_request(self.proximity, ContainsPointer)
            .unwrap_or(false)
            || self.pending.val().is_some();

        // Poll the resize grips and the move handle; only one can be dragging at a time.
        let mut active: Option<ConstraintsTuple> = None;
        let mut handle_hovered = false;
        if near {
            let (w, h) = (display.size.x, display.size.y);
            let r = GRAB_RADIUS;
            let d = 2.0 * GRAB_RADIUS;
            let edge_w = (w - d).max(0.0);
            let edge_h = (h - d).max(0.0);
            // (key, local top-left, sensor size, size multiplier)
            let grips = [
                (
                    "tr",
                    Vector { x: w - r, y: -r },
                    Vector { x: d, y: d },
                    Vector { x: 1.0, y: -1.0 },
                ),
                (
                    "bl",
                    Vector { x: -r, y: h - r },
                    Vector { x: d, y: d },
                    Vector { x: -1.0, y: 1.0 },
                ),
                (
                    "br",
                    Vector { x: w - r, y: h - r },
                    Vector { x: d, y: d },
                    Vector { x: 1.0, y: 1.0 },
                ),
                (
                    "top",
                    Vector { x: r, y: -r },
                    Vector { x: edge_w, y: d },
                    Vector { x: 0.0, y: -1.0 },
                ),
                (
                    "bottom",
                    Vector { x: r, y: h - r },
                    Vector { x: edge_w, y: d },
                    Vector { x: 0.0, y: 1.0 },
                ),
                (
                    "left",
                    Vector { x: -r, y: r },
                    Vector { x: d, y: edge_h },
                    Vector { x: -1.0, y: 0.0 },
                ),
                (
                    "right",
                    Vector { x: w - r, y: r },
                    Vector { x: d, y: edge_h },
                    Vector { x: 1.0, y: 0.0 },
                ),
            ];
            for (i, (_key, local, sensor_size, size_mul)) in grips.into_iter().enumerate() {
                let grip = self.grips[i];
                ctx.draw_workspace_node(
                    grip.erase(),
                    DrawConstraints {
                        pos: display_tl + local,
                        x: Some(AxisConstraint::Exactly(sensor_size.x)),
                        y: Some(AxisConstraint::Exactly(sensor_size.y)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: false,
                    },
                );
                if let Some(delta) = ctx.node.workspace.send_request(grip, WasDragged).flatten() {
                    active = Some(display.apply_resize(size_mul, delta, MIN_SIZE));
                }
            }

            // Move handle, in the margin beyond the inspector's lens.
            ctx.draw_workspace_node(
                self.move_sensor.erase(),
                DrawConstraints {
                    pos: display_tl - handle_offset,
                    x: Some(AxisConstraint::Exactly(HANDLE_SIZE.x)),
                    y: Some(AxisConstraint::Exactly(HANDLE_SIZE.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
            handle_hovered = ctx
                .node
                .workspace
                .send_request(self.move_sensor, WasHovered)
                .unwrap_or(false);
            if let Some(delta) = ctx
                .node
                .workspace
                .send_request(self.move_sensor, WasDragged)
                .flatten()
            {
                active = Some(ConstraintsTuple {
                    pos: display.pos + delta,
                    size: display.size,
                });
            }
        }

        let is_dragging = active.is_some();
        match active {
            Some(new) => self.pending.set(new),
            None => {
                let pending = *self.pending.val();
                if let Some(final_layout) = pending {
                    *self.pending.val_mut() = None;
                    ctx.submit_action_for_self::<Self, _>(
                        SetLayout {
                            canvas_pos: final_layout.pos,
                            size: final_layout.size,
                        },
                        "Moved/resized node",
                    );
                }
            }
        }

        // Decorations (only while near): bounds outline, grips, and the handle.
        if near {
            // Filled ghost while dragging, thin outline while merely hovering.
            let bounds = Rect {
                size: display.size,
                corner_radius: 0.0,
                fill_color: if is_dragging {
                    ghost_fill
                } else {
                    Color32::TRANSPARENT
                }
                .into(),
                border: if is_dragging {
                    ghost_stroke
                } else {
                    outline_stroke
                }
                .into(),
                stroke_kind: StrokeKind::Middle,
            };
            bounds.paint(ctx.ui.painter(), display_tl);

            // Grip circles centered *on* the preview rect's edges and corners.
            let edge_center = |mx: f32, my: f32| ScreenPos {
                x: display_tl.x
                    + if mx < 0.0 {
                        0.0
                    } else if mx > 0.0 {
                        display.size.x
                    } else {
                        display.size.x / 2.0
                    },
                y: display_tl.y
                    + if my < 0.0 {
                        0.0
                    } else if my > 0.0 {
                        display.size.y
                    } else {
                        display.size.y / 2.0
                    },
            };
            let grip_positions = [
                (1.0, -1.0),
                (-1.0, 1.0),
                (1.0, 1.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-1.0, 0.0),
                (1.0, 0.0),
            ];
            for (mx, my) in grip_positions {
                ctx.ui.painter().circle(
                    edge_center(mx, my).into(),
                    GRIP_RADIUS,
                    grip_fill,
                    grip_stroke,
                );
            }

            // Move handle.
            let handle_tl = display_tl - handle_offset;
            Rect {
                size: HANDLE_SIZE,
                corner_radius: 3.0,
                fill_color: if handle_hovered {
                    Color32::from_gray(224)
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, 12)
                }
                .into(),
                border: Stroke::NONE.into(),
                stroke_kind: StrokeKind::Middle,
            }
            .paint(ctx.ui.painter(), handle_tl);

            let dot_color = if handle_hovered {
                Color32::from_gray(100)
            } else {
                Color32::from_gray(150)
            };
            for cx in [HANDLE_SIZE.x * 0.34, HANDLE_SIZE.x * 0.66] {
                for cy in [
                    HANDLE_SIZE.y * 0.26,
                    HANDLE_SIZE.y * 0.5,
                    HANDLE_SIZE.y * 0.74,
                ] {
                    let centre = handle_tl + Vector { x: cx, y: cy };
                    ctx.ui
                        .painter()
                        .circle_filled(centre.into(), 1.4, dot_color);
                }
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(display_tl, display.size)),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(
            CanvasNodeInspector::build(ctx, ctx.id.cast(), self.child, self.committed.size).erase(),
        )
    }

    fn deref_target(&self) -> Option<NodeUid> {
        // Messages we do not understand fall through to the wrapped child.
        Some(self.child)
    }

    fn on_delete(&self, ctx: NodeContext) {
        // Deleting the node deletes the child it owns, plus its sensors.
        ctx.workspace.delete_node(self.child);
        ctx.workspace.delete_node(self.proximity.erase());
        ctx.workspace.delete_node(self.move_sensor.erase());
        for grip in self.grips {
            ctx.workspace.delete_node(grip.erase());
        }
    }
}

#[derive(Copy)]
#[utils::dynamic_type(new)]
#[utils::portable]
pub struct ConstraintsTuple {
    /// Top-left position in canvas space.
    pub pos: Vector,
    pub size: Vector,
}

impl ConstraintsTuple {
    fn apply_resize(
        self,
        // -1, 0, or +1
        dir_multiplier: Vector,
        dir_delta: Vector,
        min: f32,
    ) -> Self {
        let new_w = (self.size.x + dir_multiplier.x * dir_delta.x).max(min);
        let new_h = (self.size.y + dir_multiplier.y * dir_delta.y).max(min);
        let new_x = if dir_multiplier.x < 0.0 {
            self.pos.x + (self.size.x - new_w)
        } else {
            self.pos.x
        };
        let new_y = if dir_multiplier.y < 0.0 {
            self.pos.y + (self.size.y - new_h)
        } else {
            self.pos.y
        };
        Self {
            pos: Vector { x: new_x, y: new_y },
            size: Vector { x: new_w, y: new_h },
        }
    }
}

defhandlers! { CanvasNode {
    actions: [
        SetLayout { canvas_pos: Vector, size: Vector } => (this, s) {
            this.committed.pos = s.canvas_pos;
            this.committed.size = s.size;
        },
    ],
    requests: [
        // The node's current rendered layout (position + size) in canvas space.
        CanvasNodeConstraints => (this, _q): ConstraintsTuple {
            (*this.pending.val()).unwrap_or(this.committed)
        },
        // The workspace node this canvas node wraps.
        CanvasNodeChild => (this, _q): NodeUid { this.child },
    ],
    extern_requests: [
        // A canvas node represents its wrapped child for value resolution.
        ValueDelegate => (this, _q): Option<NodeUid> { Some(this.child) },
    ],
}}
