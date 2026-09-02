use dex_core::prelude::*;
use utils::Transient;

use crate::layouts::dnd::{self, Axis, PendingReorder, Strip};
use crate::layouts::horizontal_dnd::{AddChild, ChildCount, Children, RemoveChild, Reorder};
use crate::primitives::interaction::InteractionBox;

/// A vertical container whose child nodes can be reordered by drag-and-drop.
#[utils::dynamic_type]
#[utils::portable]
pub struct VerticalDnD {
    children: Vec<NodeUid>,
    sensors: Vec<NodeUid<InteractionBox>>,
    pub spacing: f32,

    /// Cached draw sizes from last frame
    sizes: Transient<Vec<Vector>>,
    /// Pending reorder while a child is dragged
    pending: Transient<PendingReorder>,
}

#[utils::dynamic_methods]
impl VerticalDnD {
    /// Build the container into `ws`, minting one drag sensor per child.
    pub fn build(
        ws: WorkspaceActionHandle,
        children: Vec<NodeUid>,
        spacing: f32,
    ) -> NodeUid<VerticalDnD> {
        let sensors = children
            .iter()
            .map(|_| ws.insert_node(dnd::slot_sensor()))
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
impl Node for VerticalDnD {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Vertical Drag-and-Drop".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let (result, commit) = Strip {
            axis: Axis::Vertical,
            children: &self.children,
            sensors: &self.sensors,
            spacing: self.spacing,
            sizes: &self.sizes,
            pending: &self.pending,
        }
        .draw(&mut ctx);

        if let Some(p) = commit {
            ctx.submit_action_for_self::<Self, _>(
                Reorder {
                    from: p.from,
                    to: p.to,
                },
                "Reordered children",
            );
        }
        result
    }

    fn on_delete(&self, ctx: NodeContext) {
        dnd::delete_all(ctx, &self.children, &self.sensors);
    }
}

defhandlers! { VerticalDnD {
    extern_actions: [
        AddChild => (this, a, ctx) {
            this.children.push(a.child);
            // Mint a matching slot sensor.
            this.sensors.push(ctx.workspace.insert_node(::dex_nodes::layouts::dnd::slot_sensor()));
        },
        RemoveChild => (this, a, ctx) {
            ::dex_nodes::layouts::dnd::remove_child(ctx, &mut this.children, &mut this.sensors, a.child);
        },
        Reorder => (this, s) {
            ::dex_nodes::layouts::dnd::reorder(&mut this.children, s.from, s.to);
        },
    ],
    extern_requests: [
        ChildCount => (this, _q): usize { this.children.len() },
        // The children in display order, for a caller that needs to pick a neighbour.
        Children => (this, _q): Vec<NodeUid> { this.children.clone() },
    ],
}}
