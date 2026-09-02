use dex_core::prelude::*;
use utils::Transient;

use crate::primitives::text::Label;

/// A live view of another node.
#[utils::dynamic_type]
#[utils::portable]
pub struct Mirror {
    /// The node being mirrored.
    #[uid_ref]
    target: NodeUid,

    /// The copy actually drawn.
    copy: Option<NodeUid>,

    seen_version: Transient<u64>,
}

#[utils::dynamic_methods]
impl Mirror {
    /// A mirror of `target`. The first copy is taken on the next tick.
    pub fn new(target: NodeUid) -> Mirror {
        Mirror {
            target,
            copy: None,
            seen_version: Transient::default(),
        }
    }

    /// The node this mirror follows.
    pub fn target(&self) -> NodeUid {
        self.target
    }
}

#[utils::dynamic_node]
impl Node for Mirror {
    fn type_name(&self, ctx: NodeContext) -> String {
        let reflection_type_name = ctx
            .workspace
            .get_node(self.target)
            .map(|t| t.type_name(ctx));
        match reflection_type_name {
            Some(n) => format!("A Mirror displaying {n}"),
            None => "A Mirror".into(),
        }
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let Some(copy) = self.copy else {
            // No copy yet: the first tick has not run.
            let mut placeholder = Label::new("Nothing to mirror".to_owned());
            placeholder.color = Color::gray(140);
            return ctx.draw_node(&placeholder, constraints);
        };
        ctx.draw_workspace_node(copy, constraints)
            .unwrap_or(DrawResult::Complete { region: None })
    }

    fn tick(&self, ctx: NodeContext) {
        let version = ctx.workspace.version_of(self.target);
        let seen_version = *self.seen_version.val_or_else(|| 0);

        if seen_version == 0 || version != seen_version || self.copy.is_none() {
            ctx.workspace.submit_action(
                ctx.id.cast::<Mirror>(),
                "Refreshed mirror",
                Resync { version },
            );
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        if let Some(copy) = self.copy {
            ctx.workspace.delete_node(copy);
        }
    }
}

defhandlers! { Mirror {
    actions: [
        Resync { version: u64 } => (this, a, ctx) {
            let ws = ctx.workspace.action_handle();
            if let Some(previous) = this.copy.take() {
                ws.delete_node(previous);
            }
            this.copy = Some(ws.deep_clone(this.target));
            this.seen_version.set(a.version);
        },
    ],
    requests: [
        // The node this mirror follows.
        MirrorTarget => (this, _q): NodeUid { this.target },
    ],
    extern_requests: [
        // A mirror stands in for what it mirrors when a value is resolved, so a
        // lambda wired to a mirror reads through to the real leaf.
        crate::scripting::ValueDelegate => (this, _q): Option<NodeUid> { Some(this.target) },
    ],
}}
