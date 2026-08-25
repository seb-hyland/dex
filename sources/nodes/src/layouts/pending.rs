use dex_core::prelude::*;

use crate::layouts::child::LayoutChild;
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::text::Label;

/// Wraps a node, marking it as a pending (recomputing) result.
#[utils::dynamic_type]
#[utils::portable]
pub struct PendingLayout {
    // `LayoutChild` isn't bindable; compose it from node handles/values instead.
    #[dynamic(skip)]
    pub child: LayoutChild,
}

#[utils::dynamic_methods]
impl PendingLayout {
    /// Mark `child` as a pending (recomputing) result.
    pub fn new(child: LayoutChild) -> PendingLayout {
        PendingLayout { child }
    }
}

#[utils::dynamic_node]
impl Node for PendingLayout {
    fn type_name(&self) -> String {
        "Pending".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let mut label = Label::new("Pending…".to_owned());
        label.color = Color::gray(120);
        let body = VerticalLayout {
            children: vec![LayoutChild::Node(Arc::new(label)), self.child.clone()],
            spacing: 4.0,
            fill_last: false,
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&body, constraints)
    }
}

defhandlers! { PendingLayout {
    requests: [
        // Lets a lambda detect that its output is mid-recompute.
        IsPending => (this, _q): bool { true },
    ],
}}
