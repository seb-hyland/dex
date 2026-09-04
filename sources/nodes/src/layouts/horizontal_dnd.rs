use dex_core::prelude::*;
use utils::Transient;

use crate::layouts::dnd::{self, Axis, PendingReorder, Strip};
use crate::layouts::scroll::draw_scrolled;
use crate::primitives::interaction::InteractionBox;

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

    pub scrollable: bool,

    /// Cached draw sizes from last frame
    sizes: Transient<Vec<Vector>>,
    /// Pending reorder while a child is dragged
    pending: Transient<PendingReorder>,
}

#[utils::dynamic_methods]
impl HorizontalDnD {
    /// Build the container into `ws`, minting one drag sensor per child.
    pub fn build(
        ws: WorkspaceActionHandle,
        children: Vec<NodeUid>,
        spacing: f32,
        scrollable: bool,
    ) -> NodeUid<HorizontalDnD> {
        let sensors = children
            .iter()
            .map(|_| ws.insert_node(dnd::slot_sensor()))
            .collect();
        ws.insert_node(Self {
            children,
            sensors,
            spacing,
            scrollable,
            sizes: Transient::default(),
            pending: Transient::default(),
        })
    }
}

impl HorizontalDnD {
    /// The strip that draws (and reorders) this container's children.
    fn strip(&self) -> Strip<'_> {
        Strip {
            axis: Axis::Horizontal,
            children: &self.children,
            sensors: &self.sensors,
            spacing: self.spacing,
            sizes: &self.sizes,
            pending: &self.pending,
        }
    }
}

#[utils::dynamic_node]
impl Node for HorizontalDnD {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Horizontal Drag-and-Drop".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let (result, commit) = if self.scrollable {
            let id = egui::Id::new(ctx.node.id);
            draw_scrolled(&mut ctx, [true, false], id, |sub| self.strip().draw(sub))
        } else {
            self.strip().draw(&mut ctx)
        };

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

defhandlers! { HorizontalDnD {
    actions: [
        AddChild { child: NodeUid } => (this, a, ctx) {
            this.children.push(a.child);
            // Mint a matching slot sensor.
            this.sensors.push(ctx.workspace.insert_node(::dex_nodes::layouts::dnd::slot_sensor()));
        },
        RemoveChild { child: NodeUid } => (this, a, ctx) {
            ::dex_nodes::layouts::dnd::remove_child(ctx, &mut this.children, &mut this.sensors, a.child);
        },
        Reorder { from: usize, to: usize } => (this, s) {
            ::dex_nodes::layouts::dnd::reorder(&mut this.children, s.from, s.to);
        },
    ],
    requests: [
        ChildCount => (this, _q): usize { this.children.len() },
        // The children in display order, for a caller that needs to pick a neighbour.
        Children => (this, _q): Vec<NodeUid> { this.children.clone() },
    ],
}}
