use dex_core::prelude::*;
use dex_core::theme;
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
    /**
        A mirror of `target`. The first copy is taken on the next tick.

        `NodeUid::nil()` is allowed and means "nothing yet": a mirror is often
        built by something that does not know what it will be showing — an
        output row exists before the pin it stands for is wired to anything —
        and [`SetMirrorTarget`] points it once there is something to point at.
    */
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
            placeholder.color = theme::INK_FAINT;
            return ctx.draw_node(&placeholder, constraints);
        };
        ctx.draw_workspace_node(copy, constraints)
            .unwrap_or(DrawResult::Complete { region: None })
    }

    fn tick(&self, ctx: NodeContext) {
        // A mirror of nothing is waiting.
        if self.target == NodeUid::nil() {
            return;
        }
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
        /*
            Point this mirror somewhere else.

            The copy is dropped rather than kept until the next one is taken:
            what it holds is a picture of the *old* target, and showing that
            under a new name for a frame is worse than showing nothing.
        */
        SetMirrorTarget { target: NodeUid } => (this, s, ctx) {
            if this.target != s.target {
                if let Some(previous) = this.copy.take() {
                    ctx.workspace.delete_node(previous);
                }
                this.target = s.target;
                // Zero is what `tick` reads as "never synced", so the next one
                // takes a copy without needing to be told again.
                this.seen_version.set(0);
            }
        },
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
