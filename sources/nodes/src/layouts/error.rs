use dex_core::prelude::*;
use dex_core::theme;

use crate::layouts::child::LayoutChild;
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::text::Label;

/// Wraps a node, marking it as an error result.
#[utils::dynamic_type]
#[utils::portable]
pub struct ErrorLayout {
    // `LayoutChild` isn't bindable; compose it from node handles/values instead.
    #[dynamic(skip)]
    pub child: LayoutChild,
}

#[utils::dynamic_methods]
impl ErrorLayout {
    /// An error result displaying `message`.
    pub fn message(message: String) -> Self {
        let mut label = Label::new(message);
        label.color = theme::DANGER;
        label.singleline = false; // wrap long error messages
        Self {
            child: LayoutChild::Node(Arc::new(label)),
        }
    }

    /// Mark `child` as an error result.
    pub fn new(child: LayoutChild) -> ErrorLayout {
        ErrorLayout { child }
    }
}

#[utils::dynamic_node]
impl Node for ErrorLayout {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Error".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let mut header = Label::new("Error occurred".to_owned());
        header.color = theme::DANGER;
        let body = VerticalLayout {
            children: vec![LayoutChild::Node(Arc::new(header)), self.child.clone()],
            spacing: 4.0,
            fill_last: false,
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&body, constraints)
    }
}

defhandlers! { ErrorLayout {} }
