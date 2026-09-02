/*!
    The body shared by [`HorizontalDnD`](super::horizontal_dnd::HorizontalDnD)
    and [`VerticalDnD`](super::vertical_dnd::VerticalDnD).

    The two strips differ only in which coordinate runs along the strip and
    which runs across it, so everything else — the cached sizes, the drop-slot
    arithmetic, the preview and the commit — lives here once and is read
    through an [`Axis`].
*/

use dex_core::prelude::*;
use dex_core::theme;
use utils::Transient;

use crate::primitives::{
    interaction::{DragPointerPos, InteractionBox, WasDragReleased},
    shapes::{Path, Rect},
};

/// Which way a strip runs.
#[derive(Clone, Copy)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// The component that runs along the strip.
    fn along(self, v: Vector) -> f32 {
        match self {
            Self::Horizontal => v.x,
            Self::Vertical => v.y,
        }
    }

    /// The component that runs across it.
    fn across(self, v: Vector) -> f32 {
        match self {
            Self::Horizontal => v.y,
            Self::Vertical => v.x,
        }
    }

    /// A vector from its along- and across-components.
    fn vector(self, along: f32, across: f32) -> Vector {
        match self {
            Self::Horizontal => Vector::new(along, across),
            Self::Vertical => Vector::new(across, along),
        }
    }
}

/// An in-progress drag: the child at index `from` is previewed at slot `to`.
#[derive(Clone, Copy)]
pub(crate) struct PendingReorder {
    pub from: usize,
    pub to: usize,
}

/// The mutable state a strip keeps between frames.
pub(crate) struct Strip<'a> {
    pub axis: Axis,
    pub children: &'a [NodeUid],
    pub sensors: &'a [NodeUid<InteractionBox>],
    pub spacing: f32,
    /// Last frame's child sizes, needed to place this frame's drop slots.
    pub sizes: &'a Transient<Vec<Vector>>,
    pub pending: &'a Transient<PendingReorder>,
}

impl Strip<'_> {
    /**
        Draw the strip, returning the region it took and the reorder to commit.

        The reorder is handed back rather than submitted because the action is
        addressed to the concrete node type, which only the caller knows.
    */
    pub fn draw(&self, ctx: &mut DrawContext) -> (DrawResult, Option<PendingReorder>) {
        let axis = self.axis;
        let avail = Vector::new(
            ctx.constraints
                .x
                .map_or(f32::INFINITY, |a| a.provided_value()),
            ctx.constraints
                .y
                .map_or(f32::INFINITY, |a| a.provided_value()),
        );
        let origin = ctx.constraints.pos;
        let n = self.children.len();

        let pending = *self.pending.val();

        // Where the children sat when nothing was being dragged.
        let last_sizes = self.sizes.val().clone().filter(|s| s.len() == n);
        let natural_centers = last_sizes.as_ref().map(|sizes| {
            let mut cursor = axis.along(origin.to_vector());
            sizes
                .iter()
                .map(|s| {
                    let centre = cursor + axis.along(*s) * 0.5;
                    cursor += axis.along(*s) + self.spacing;
                    centre
                })
                .collect::<Vec<_>>()
        });
        let insertion_index = |along: f32, exclude: usize| -> Option<usize> {
            let centers = natural_centers.as_ref()?;
            Some(
                (0..n)
                    .filter(|&i| i != exclude && centers[i] < along)
                    .count(),
            )
        };

        // The dragged child is drawn in the slot it would land in.
        let draw_order: Vec<usize> = match pending {
            Some(p) => {
                let mut order: Vec<usize> = (0..n).filter(|&i| i != p.from).collect();
                order.insert(p.to.min(order.len()), p.from);
                order
            }
            None => (0..n).collect(),
        };

        let mut slot_rect: Vec<Option<ScreenRegion>> = vec![None; n];
        let mut new_sizes: Vec<Vector> = vec![Vector::ZERO; n];
        let mut extent_across = 0.0_f32;
        let mut cursor = 0.0;
        let mut dragged: Option<(usize, ScreenPos)> = None;
        let mut released: Option<usize> = None;

        for &idx in &draw_order {
            let child_pos = origin + axis.vector(cursor, 0.0);

            // The drag sensor goes under the child, sized from last frame.
            if let Some(size) = last_sizes.as_ref().and_then(|s| s.get(idx).copied())
                && let Some(&sensor) = self.sensors.get(idx)
            {
                ctx.draw_workspace_node(
                    sensor.erase(),
                    DrawConstraints {
                        pos: child_pos,
                        x: Some(AxisConstraint::Exactly(size.x)),
                        y: Some(AxisConstraint::Exactly(size.y)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: false,
                    },
                );
                let ws = ctx.node.workspace;
                if let Some(pos) = ws.send_request(sensor, DragPointerPos).flatten() {
                    dragged = Some((idx, pos));
                }
                if ws.send_request(sensor, WasDragReleased).unwrap_or(false) {
                    released = Some(idx);
                }
            }

            // What is left of the strip's length, and all of its breadth.
            let remaining = axis.vector((axis.along(avail) - cursor).max(0.0), axis.across(avail));
            let Some(region) = ctx
                .draw_inspectable_node(
                    self.children[idx],
                    DrawConstraints {
                        pos: child_pos,
                        x: Some(AxisConstraint::AtMost(remaining.x)),
                        y: Some(AxisConstraint::AtMost(remaining.y)),
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
            cursor += axis.along(size) + self.spacing;
            extent_across = extent_across.max(axis.across(size));
        }

        self.sizes.set(new_sizes);

        // The dragged child is tinted, with a rule at the seam it would fall into.
        if let Some(p) = pending
            && let Some(region) = slot_rect[p.from]
        {
            Rect {
                size: region.size(),
                corner_radius: theme::RADIUS_MD,
                fill_color: theme::ACCENT_SOFT,
                border: Stroke::NONE,
                stroke_kind: StrokeKind::Middle,
            }
            .paint(ctx.ui.painter(), region.min);

            let seam = axis.along(region.min.to_vector()) - self.spacing * 0.5;
            Path::span(
                axis.vector(0.0, extent_across),
                Stroke::new(2.0, theme::ACCENT),
            )
            .paint(
                ctx.ui.painter(),
                (axis.vector(seam, axis.across(origin.to_vector()))).to_screen_pos(),
            );
        }

        let commit = (released.is_some())
            .then_some(pending)
            .flatten()
            .filter(|p| p.to != p.from);

        match dragged.map(|(from, pos)| PendingReorder {
            from,
            to: insertion_index(axis.along(pos.to_vector()), from).unwrap_or(from),
        }) {
            Some(p) => self.pending.set(p),
            None => *self.pending.val_mut() = None,
        }

        let size = axis.vector((cursor - self.spacing).max(0.0), extent_across);
        (
            DrawResult::Complete {
                region: Some(ScreenRegion::from_min_size(origin, size)),
            },
            commit,
        )
    }
}

/// The sensor a strip puts under each child: drags only.
pub(crate) fn slot_sensor() -> InteractionBox {
    InteractionBox::sensing(false, false, true)
}

/// Delete a strip's children and their sensors.
pub(crate) fn delete_all(
    ctx: NodeContext,
    children: &[NodeUid],
    sensors: &[NodeUid<InteractionBox>],
) {
    for child in children {
        ctx.workspace.delete_node(*child);
    }
    for sensor in sensors {
        ctx.workspace.delete_node(sensor.erase());
    }
}

/// Drop `child` and its sensor from a strip.
pub(crate) fn remove_child(
    ctx: NodeContext,
    children: &mut Vec<NodeUid>,
    sensors: &mut Vec<NodeUid<InteractionBox>>,
    child: NodeUid,
) {
    if let Some(pos) = children.iter().position(|c| *c == child) {
        children.remove(pos);
        ctx.workspace.delete_node(sensors.remove(pos).erase());
    }
}

/// Move the child at `from` to index `to`, if both are in range.
pub(crate) fn reorder(children: &mut Vec<NodeUid>, from: usize, to: usize) {
    if from < children.len() && to < children.len() {
        let child = children.remove(from);
        children.insert(to, child);
    }
}
