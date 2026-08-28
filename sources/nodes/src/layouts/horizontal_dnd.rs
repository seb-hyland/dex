use dex_core::prelude::*;
use utils::Transient;

use crate::primitives::{
    interaction::{DragPointerPos, InteractionBox, WasDragReleased},
    shapes::{Line, Rect},
};

/**
   A horizontal container whose child nodes can be reordered by drag-and-drop.

   This is a self-contained node that owns its children and the drag-and-drop state.
   Use [`AddChild`]/[`RemoveChild`] to mutate the list.
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct HorizontalDnD {
    children: Vec<NodeUid>,
    sensors: Vec<NodeUid<InteractionBox>>,
    pub spacing: f32,

    /// Cached draw sizes from last frame
    sizes: Transient<Vec<Vector>>,
    /// Pending reorder while a child is dragged
    pending: Transient<PendingReorder>,
}

/// An in-progress drag: the child at index `from` is previewed at slot `to`
#[derive(Clone, Copy)]
struct PendingReorder {
    from: usize,
    to: usize,
}

#[utils::dynamic_methods]
impl HorizontalDnD {
    /// Build the container into `ws`, minting one drag sensor per child.
    pub fn build(
        ws: WorkspaceActionHandle,
        children: Vec<NodeUid>,
        spacing: f32,
    ) -> NodeUid<HorizontalDnD> {
        let sensors = children
            .iter()
            .map(|_| ws.insert_node(InteractionBox::sensing(false, false, true)))
            .collect();
        ws.insert_node(Self {
            children,
            sensors,
            spacing,
            sizes: Transient::default(),
            pending: Transient::default(),
        })
    }
}

#[utils::dynamic_node]
impl Node for HorizontalDnD {
    fn type_name(&self) -> String {
        "Horizontal Drag-and-Drop".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = ctx
            .constraints
            .y
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);

        let origin = ctx.constraints.pos;

        let n = self.children.len();

        // Render from the pending drag ----------------------------------------
        let pending = *self.pending.val();

        // Determine last frame's sizes for draw ----------------------------------------
        // (necessary to determine drop spot of in-progress drag)
        let last_sizes: Option<Vec<Vector>> = self.sizes.val().clone();
        let last_sizes = last_sizes.filter(|s| s.len() == n);
        let natural_centers = last_sizes.as_ref().map(|sizes| {
            let mut centers = Vec::with_capacity(n);
            let mut x = origin.x;
            for s in sizes {
                centers.push(x + s.x * 0.5);
                x += s.x + self.spacing;
            }
            centers
        });

        let insertion_index = |px: f32, exclude: usize| -> Option<usize> {
            let centers = natural_centers.as_ref()?;
            Some((0..n).filter(|&i| i != exclude && centers[i] < px).count())
        };
        let draw_order: Vec<usize> = match pending {
            Some(p) => {
                // Order with pending drag previewed
                let mut order: Vec<usize> = (0..n).filter(|&i| i != p.from).collect();
                order.insert(p.to.min(order.len()), p.from);
                order
            }
            None => (0..n).collect(),
        };

        // Actually draw ----------------------------------------
        let mut slot_rect: Vec<Option<ScreenRegion>> = vec![None; n];
        let mut new_sizes: Vec<Vector> = vec![Vector::splat(0.0); n];
        let mut max_h = 0.0_f32;
        let mut cursor_x = 0.0;
        let mut this_dragged: Option<(usize, ScreenPos)> = None;
        let mut this_released: Option<usize> = None;

        for &idx in &draw_order {
            let child_pos = origin
                + Vector {
                    x: cursor_x,
                    y: 0.0,
                };

            // Drag sensor under child
            if let Some(size) = last_sizes.as_ref().and_then(|s| s.get(idx).copied())
                && let Some(&sensor) = self.sensors.get(idx)
            {
                ctx.draw_workspace_node(
                    sensor,
                    DrawConstraints {
                        pos: child_pos,
                        x: Some(AxisConstraint::Exactly(size.x)),
                        y: Some(AxisConstraint::Exactly(size.y)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: false,
                    },
                );
                if let Some(pos) = ctx
                    .node
                    .workspace
                    .send_request(sensor, DragPointerPos)
                    .flatten()
                {
                    this_dragged = Some((idx, pos));
                }
                if ctx
                    .node
                    .workspace
                    .send_request(sensor, WasDragReleased)
                    .unwrap_or(false)
                {
                    this_released = Some(idx);
                }
            }

            let Some(region) = ctx
                .draw_workspace_node(
                    self.children[idx],
                    DrawConstraints {
                        pos: child_pos,
                        x: Some(AxisConstraint::AtMost((avail_w - cursor_x).max(0.0))),
                        y: Some(AxisConstraint::AtMost(avail_h)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: ctx.constraints.should_clip,
                    },
                )
                .and_then(|r| r.region())
            else {
                continue;
            };

            let size = region.size();
            slot_rect[idx] = Some(region);
            new_sizes[idx] = size;
            cursor_x += size.x + self.spacing;
            max_h = max_h.max(size.y);
        }

        self.sizes.set(new_sizes);

        // Preview highlight + insertion line
        if let Some(p) = pending
            && let Some(region) = slot_rect[p.from]
        {
            let size = region.size();
            Rect {
                size,
                corner_radius: 4.0,
                fill_color: Color::rgba(70, 130, 180, 28),
                border: Stroke::NONE,
                stroke_kind: StrokeKind::Middle,
            }
            .paint(ctx.ui.painter(), region.min);

            Line {
                span: Vector { x: 0.0, y: max_h },
                stroke: Stroke::new(2.0, Color::gray(120)),
            }
            .paint(
                ctx.ui.painter(),
                ScreenPos {
                    x: region.min.x - self.spacing * 0.5,
                    y: origin.y,
                },
            );
        }

        // Refresh pending drag and commit on drop ----------------------------------------
        let new_pending = this_dragged.map(|(from, pos)| PendingReorder {
            from,
            to: insertion_index(pos.x, from).unwrap_or(from),
        });
        if this_released.is_some()
            && let Some(p) = pending
            && p.to != p.from
        {
            ctx.submit_action_for_self::<Self, _>(
                Reorder {
                    from: p.from,
                    to: p.to,
                },
                "Reordered children",
            );
        }
        match new_pending {
            Some(p) => self.pending.set(p),
            None => *self.pending.val_mut() = None,
        }

        let size = Vector {
            x: (cursor_x - self.spacing).max(0.0),
            y: max_h,
        };

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        for child in &self.children {
            ctx.workspace.delete_node(*child);
        }
        for sensor in &self.sensors {
            ctx.workspace.delete_node(sensor.erase());
        }
    }
}

defhandlers! { HorizontalDnD {
    actions: [
        AddChild { child: NodeUid } => (this, a, ctx) {
            this.children.push(a.child);
            // Mint a matching slot sensor.
            let sensor = ctx.workspace.insert_node(InteractionBox::sensing(false, false, true));
            this.sensors.push(sensor);
        },
        RemoveChild { child: NodeUid } => (this, a, ctx) {
            if let Some(pos) = this.children.iter().position(|c| *c == a.child) {
                this.children.remove(pos);
                let sensor = this.sensors.remove(pos);
                ctx.workspace.delete_node(sensor.erase());
            }
        },
        Reorder { from: usize, to: usize } => (this, s) {
            if s.from < this.children.len() && s.to < this.children.len() {
                let child = this.children.remove(s.from);
                this.children.insert(s.to, child);
            }
        },
    ],
    requests: [
        ChildCount => (this, _q): usize { this.children.len() },
    ],
}}
