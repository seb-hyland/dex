use dex_core::prelude::*;

/// A node that draws nothing.
#[utils::portable(noop_reset)]
pub struct Nothing;

#[utils::dynamic_node(skip)]
impl Node for Nothing {
    fn type_name(&self) -> String {
        "Nothing".into()
    }

    fn draw(&self, _ctx: DrawContext) -> DrawResult {
        DrawResult::Complete { region: None }
    }
}

defhandlers! { Nothing {} }
