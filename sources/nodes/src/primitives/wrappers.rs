use dex_core::prelude::*;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient, match_dyn};

use crate::layouts::canvas::MoveChild;
use crate::primitives::interaction::{InteractionBox, WasDragged};
use crate::primitives::shapes::Rect;

/**
   A wrapper that draws a child element at a fixed [`size`](Self::size) and exposes draggable edges and corners for resizing
*/
#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Resizable {
    pub size: Vector,
    pub child: NodeUid,
    pending: Transient<Vector>,
}

impl Resizable {
    pub fn new(size: Vector, child: NodeUid) -> Self {
        Self {
            size,
            child,
            pending: Transient::default(),
        }
    }
}

struct ResizeHandle {
    /// Stable key used to derive this handle's [`LocalId`]
    key: &'static str,
    origin: ScreenPos,
    size: Vector,
    /// Mapping of horizontal drag delta onto width (-1, 0, or +1)
    h_mul: f32,
    /// Mapping of vertical drag delta onto height (-1, 0, or +1)
    v_mul: f32,
}

#[typetag::serde]
impl Node for Resizable {
    fn type_name(&self) -> String {
        "Resizable".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const HANDLE_THICKNESS: f32 = 8.0;
        /// Smallest size the box can be resized to.
        const MIN_SIZE: f32 = 24.0;

        let size = self.size;
        let origin = ctx.constraints.pos.to_top_left(size);
        let region = ScreenRegion::from_min_size(origin, size);

        // Draw child ----------------------------------------
        let child_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(origin),
            x: Some(AxisConstraint::Exactly(size.x)),
            y: Some(AxisConstraint::Exactly(size.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };
        ctx.draw_workspace_node(self.child, child_constraints);

        // Draw and poll resize handles ----------------------------------------
        let edge_w = (size.x - 2.0 * HANDLE_THICKNESS).max(0.0);
        let edge_h = (size.y - 2.0 * HANDLE_THICKNESS).max(0.0);
        let right = origin.x + size.x - HANDLE_THICKNESS;
        let bottom = origin.y + size.y - HANDLE_THICKNESS;
        let handles = [
            // Corners
            ResizeHandle {
                key: "tl",
                origin,
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: HANDLE_THICKNESS,
                },
                h_mul: -1.0,
                v_mul: -1.0,
            },
            ResizeHandle {
                key: "tr",
                origin: ScreenPos {
                    x: right,
                    y: origin.y,
                },
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: HANDLE_THICKNESS,
                },
                h_mul: 1.0,
                v_mul: -1.0,
            },
            ResizeHandle {
                key: "bl",
                origin: ScreenPos {
                    x: origin.x,
                    y: bottom,
                },
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: HANDLE_THICKNESS,
                },
                h_mul: -1.0,
                v_mul: 1.0,
            },
            ResizeHandle {
                key: "br",
                origin: ScreenPos {
                    x: right,
                    y: bottom,
                },
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: HANDLE_THICKNESS,
                },
                h_mul: 1.0,
                v_mul: 1.0,
            },
            // Edges (spanning between the corners)
            ResizeHandle {
                key: "top",
                origin: ScreenPos {
                    x: origin.x + HANDLE_THICKNESS,
                    y: origin.y,
                },
                size: Vector {
                    x: edge_w,
                    y: HANDLE_THICKNESS,
                },
                h_mul: 0.0,
                v_mul: -1.0,
            },
            ResizeHandle {
                key: "bottom",
                origin: ScreenPos {
                    x: origin.x + HANDLE_THICKNESS,
                    y: bottom,
                },
                size: Vector {
                    x: edge_w,
                    y: HANDLE_THICKNESS,
                },
                h_mul: 0.0,
                v_mul: 1.0,
            },
            ResizeHandle {
                key: "left",
                origin: ScreenPos {
                    x: origin.x,
                    y: origin.y + HANDLE_THICKNESS,
                },
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: edge_h,
                },
                h_mul: -1.0,
                v_mul: 0.0,
            },
            ResizeHandle {
                key: "right",
                origin: ScreenPos {
                    x: right,
                    y: origin.y + HANDLE_THICKNESS,
                },
                size: Vector {
                    x: HANDLE_THICKNESS,
                    y: edge_h,
                },
                h_mul: 1.0,
                v_mul: 0.0,
            },
        ];

        let mut active_handle: Option<(f32, f32, Vector)> = None;
        for handle in handles {
            let mut sensor = InteractionBox::default();
            sensor.senses_drags = true;
            let handle_constraints = DrawConstraints {
                pos: PositionConstraint::TopLeft(handle.origin),
                x: Some(AxisConstraint::Exactly(handle.size.x)),
                y: Some(AxisConstraint::Exactly(handle.size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            };
            ctx.draw_node(
                &sensor,
                LocalId::from_cons(ctx.id, handle.key),
                handle_constraints,
            );

            if let Some(delta) = sensor.request_typed(Box::new(WasDragged)).flatten() {
                active_handle = Some((handle.h_mul, handle.v_mul, delta));
            }
        }

        match active_handle {
            Some((h_mul, v_mul, delta)) => {
                // A handle is being dragged!

                // Grow from the active drag (or the in-progress pending size).
                let base = (*self.pending.val()).unwrap_or(size);
                let new_size = Vector {
                    x: (base.x + h_mul * delta.x).max(MIN_SIZE),
                    y: (base.y + v_mul * delta.y).max(MIN_SIZE),
                };
                self.pending.set(new_size);

                // Preview the prospective new size
                let ghost = Rect {
                    size: new_size,
                    corner_radius: 0.0,
                    fill_color: Color32::from_rgba_unmultiplied(120, 160, 255, 48),
                    border: Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 160, 255, 160)),
                };
                let ghost_constraints = DrawConstraints {
                    pos: PositionConstraint::TopLeft(origin),
                    x: Some(AxisConstraint::Exactly(new_size.x)),
                    y: Some(AxisConstraint::Exactly(new_size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                };
                ctx.draw_node(
                    &ghost,
                    LocalId::from_cons(ctx.id, "ghost"),
                    ghost_constraints,
                );
            }
            None => {
                // No active drag

                // If a drag was in progress last frame, commit the release state
                let pending = *self.pending.val();
                if let Some(new_size) = pending {
                    *self.pending.val_mut() = None;
                    if let Id::Workspace(uid) = ctx.id {
                        ctx.workspace.submit_action(Action {
                            dest: Some(uid),
                            description: "Resized element".into(),
                            body: Box::new(SetSize { size: new_size }),
                        });
                    }
                }
            }
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {
        match_dyn! { r,
            s: SetSize => self.size = s.size,
            _ => {}
        }
    }
}

impl Requestable for Resizable {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SetSize {
    pub size: Vector,
}

#[typetag::serde]
impl ActionBody for SetSize {}

/**
   A wrapper to make a child of a [`CanvasLayout`](crate::layouts::canvas::CanvasLayout) draggable to move it.
*/
#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Movable {
    /// The canvas that owns this element.
    pub parent: NodeUid,
    pub child: NodeUid,
    pending: Transient<Vector>,
}

impl Movable {
    pub fn new(parent: NodeUid, child: NodeUid) -> Self {
        Self {
            parent,
            child,
            pending: Transient::default(),
        }
    }
}

#[typetag::serde]
impl Node for Movable {
    fn type_name(&self) -> String {
        "Movable".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let last_child_region = ctx.last_frame_location_of(self.child);
        let region = last_child_region.unwrap_or_else(|| {
            let origin = ctx.constraints.pos.to_top_left(Vector::splat(0.0));
            ScreenRegion::from_min_size(origin, Vector::splat(0.0))
        });

        const OFFSET: Vector = Vector { x: 3.0, y: 0.0 };
        let handle_origin = region.min - OFFSET;
        let size = region.size();

        // Draw the drag handle ----------------------------------------
        const HANDLE_SIZE: f32 = 14.0;
        let handle_size = Vector::splat(HANDLE_SIZE);
        let handle_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(handle_origin),
            x: Some(AxisConstraint::Exactly(handle_size.x)),
            y: Some(AxisConstraint::Exactly(handle_size.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: false,
        };
        let handle = Rect {
            size: handle_size,
            corner_radius: 3.0,
            fill_color: Color32::from_rgba_unmultiplied(120, 160, 255, 210),
            border: Stroke::new(1.0, Color32::from_rgb(90, 120, 220)),
        };
        ctx.draw_node(
            &handle,
            LocalId::from_cons(ctx.id, "handle"),
            handle_constraints,
        );

        let mut sensor = InteractionBox::default();
        sensor.senses_drags = true;
        ctx.draw_node(
            &sensor,
            LocalId::from_cons(ctx.id, "handle sensor"),
            handle_constraints,
        );
        let drag = sensor.request_typed(Box::new(WasDragged)).flatten();

        match drag {
            Some(delta) => {
                // This node is being dragged!

                let accumulated = (*self.pending.val()).unwrap_or(Vector::splat(0.0)) + delta;
                self.pending.set(accumulated);

                // Draw the child at preview location
                let child_constraints = DrawConstraints {
                    pos: PositionConstraint::TopLeft(region.min + accumulated),
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly(size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                };
                ctx.draw_workspace_node(self.child, child_constraints);
            }
            None => {
                // No active drag

                let mut child_pos;

                // If a drag just finished, inform the parent canvas
                if let Some(delta) = *self.pending.val() {
                    *self.pending.val_mut() = None;
                    if let Id::Workspace(uid) = ctx.id {
                        ctx.workspace.submit_action(Action {
                            dest: Some(self.parent),
                            description: "Moved element".into(),
                            body: Box::new(MoveChild { child: uid, delta }),
                        });
                    }

                    // For this frame, still draw at preview location
                    child_pos = region.min + delta;
                } else {
                    child_pos = region.min;
                }

                // Draw the child
                let child_constraints = DrawConstraints {
                    pos: PositionConstraint::TopLeft(child_pos),
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly(size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                };
                ctx.draw_workspace_node(self.child, child_constraints);
            }
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for Movable {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
