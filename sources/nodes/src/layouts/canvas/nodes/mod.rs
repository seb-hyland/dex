pub mod shapes;

use dex_core::prelude::*;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::primitives::{
    interaction::{ContainsPointer, InteractionBox, WasDragged, WasHovered},
    shapes::Rect,
};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CanvasNode {
    pub child: NodeUid,
    /// Top-left position of this node in canvas space and size.
    pub committed: ConstraintsTuple,
    /// Uncommitted preview accumulated during interaction.
    pending: Transient<ConstraintsTuple>,

    /// Hover sensor covering the node + margin.
    proximity: NodeUid<InteractionBox>,
    /// Drag/hover sensor for the move handle.
    move_sensor: NodeUid<InteractionBox>,
    /// The eight resize grips: tl, tr, bl, br, top, bottom, left, right.
    grips: [NodeUid<InteractionBox>; 8],
}

impl CanvasNode {
    /// Build a canvas node (wrapping `child`) and its sensors into `ws`.
    pub fn build(
        ws: &Workspace,
        child: NodeUid,
        canvas_pos: Vector,
        size: Vector,
    ) -> NodeUid<CanvasNode> {
        let sensor = |kind| ws.insert_node(Box::new(kind));
        ws.insert_node(Box::new(Self {
            child,
            committed: ConstraintsTuple {
                pos: canvas_pos,
                size,
            },
            pending: Transient::default(),
            proximity: sensor(InteractionBox::sensing(true, false, false)),
            move_sensor: sensor(InteractionBox::sensing(true, false, true)),
            grips: std::array::from_fn(|_| sensor(InteractionBox::sensing(false, false, true))),
        }))
    }
}

#[typetag::serde]
impl Node for CanvasNode {
    fn type_name(&self) -> String {
        "Canvas Node".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const GRAB_RADIUS: f32 = 8.0;
        const GRIP_RADIUS: f32 = 4.0;
        const MIN_SIZE: f32 = 24.0;
        const HANDLE_SIZE: Vector = Vector { x: 12.0, y: 18.0 };
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
            self.proximity,
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
            let edge_w = (w - 2.0 * GRAB_RADIUS).max(0.0);
            let edge_h = (h - 2.0 * GRAB_RADIUS).max(0.0);
            // (key, local top-left, sensor size, size multiplier)
            let grips = [
                (
                    "tl",
                    Vector { x: 0.0, y: 0.0 },
                    Vector {
                        x: GRAB_RADIUS,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: -1.0, y: -1.0 },
                ),
                (
                    "tr",
                    Vector {
                        x: w - GRAB_RADIUS,
                        y: 0.0,
                    },
                    Vector {
                        x: GRAB_RADIUS,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: 1.0, y: -1.0 },
                ),
                (
                    "bl",
                    Vector {
                        x: 0.0,
                        y: h - GRAB_RADIUS,
                    },
                    Vector {
                        x: GRAB_RADIUS,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: -1.0, y: 1.0 },
                ),
                (
                    "br",
                    Vector {
                        x: w - GRAB_RADIUS,
                        y: h - GRAB_RADIUS,
                    },
                    Vector {
                        x: GRAB_RADIUS,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: 1.0, y: 1.0 },
                ),
                (
                    "top",
                    Vector {
                        x: GRAB_RADIUS,
                        y: 0.0,
                    },
                    Vector {
                        x: edge_w,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: 0.0, y: -1.0 },
                ),
                (
                    "bottom",
                    Vector {
                        x: GRAB_RADIUS,
                        y: h - GRAB_RADIUS,
                    },
                    Vector {
                        x: edge_w,
                        y: GRAB_RADIUS,
                    },
                    Vector { x: 0.0, y: 1.0 },
                ),
                (
                    "left",
                    Vector {
                        x: 0.0,
                        y: GRAB_RADIUS,
                    },
                    Vector {
                        x: GRAB_RADIUS,
                        y: edge_h,
                    },
                    Vector { x: -1.0, y: 0.0 },
                ),
                (
                    "right",
                    Vector {
                        x: w - GRAB_RADIUS,
                        y: GRAB_RADIUS,
                    },
                    Vector {
                        x: GRAB_RADIUS,
                        y: edge_h,
                    },
                    Vector { x: 1.0, y: 0.0 },
                ),
            ];
            for (i, (_key, local, sensor_size, size_mul)) in grips.into_iter().enumerate() {
                let grip = self.grips[i];
                ctx.draw_workspace_node(
                    grip,
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

            // Move handle sensor (offset to the left of the node).
            ctx.draw_workspace_node(
                self.move_sensor,
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
                },
                border: if is_dragging {
                    ghost_stroke
                } else {
                    outline_stroke
                },
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
                (-1.0, -1.0),
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

            // Move handle plate + dot grid, following the preview top-left.
            let handle_tl = display_tl - handle_offset;
            let plate = Rect {
                size: HANDLE_SIZE,
                corner_radius: 3.0,
                fill_color: if handle_hovered {
                    Color32::from_gray(224)
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, 10)
                },
                border: Stroke::NONE,
            };
            plate.paint(ctx.ui.painter(), handle_tl);
            let dot_color = if handle_hovered {
                Color32::from_gray(110)
            } else {
                Color32::from_gray(150)
            };
            let cols = [HANDLE_SIZE.x * 0.34, HANDLE_SIZE.x * 0.66];
            let rows = [
                HANDLE_SIZE.y * 0.26,
                HANDLE_SIZE.y * 0.5,
                HANDLE_SIZE.y * 0.74,
            ];
            for cx in cols {
                for cy in rows {
                    let center = handle_tl + Vector { x: cx, y: cy };
                    ctx.ui
                        .painter()
                        .circle_filled(center.into(), 1.4, dot_color);
                }
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(display_tl, display.size)),
        }
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

#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
pub struct ConstraintsTuple {
    pos: Vector,
    size: Vector,
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
}}
