//! Editable numbers.
//!
//! Each is a [`LabelEditable`] wearing a type: it draws and edits exactly like
//! one, but only text that parses is allowed to become the value. A script sees
//! the number rather than its rendering, the way a label is seen as its string.

use dex_core::prelude::*;

use crate::primitives::text::{GetText, LabelEditable, SetText};

/**
    Define a node wrapping [`LabelEditable`] that only holds text parsing as `$ty`.

    The field is drawn *as this node*, so the `SetText` it commits on focus loss
    arrives here to be checked rather than landing on the field itself.
*/
macro_rules! number_node {
    ($(#[$meta:meta])* $name:ident, $ty:ty, $label:literal) => {
        $(#[$meta])*
        #[utils::dynamic_type]
        #[utils::portable]
        pub struct $name {
            /// The committed value. The field's text is only ever a rendering of it.
            pub value: $ty,
            /// The editor this node wraps.
            field: LabelEditable,
        }

        #[utils::dynamic_methods]
        impl $name {
            pub fn new(value: $ty) -> Self {
                Self {
                    value,
                    field: LabelEditable::new(value.to_string()),
                }
            }
        }

        #[utils::dynamic_node]
        impl Node for $name {
            fn type_name(&self, _ctx: NodeContext) -> String {
                $label.to_owned()
            }

            fn draw(&self, mut ctx: DrawContext) -> DrawResult {
                let constraints = ctx.constraints;
                // As self, so the field's commit comes back here.
                ctx.draw_child_as_self(&self.field, constraints)
            }

            fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
                // It looks like a label, so it takes a label's styling controls.
                self.field.build_inspector(ctx)
            }
        }

        defhandlers! { $name {
            extern_actions: [
                /*
                    The field's edit, arriving here because it drew as this node.

                    Text that does not parse is dropped and the field snaps back
                    to the value it had, so this node never holds a non-number.
                */
                SetText => (this, s) {
                    if let Ok(parsed) = s.value.trim().parse::<$ty>() {
                        this.value = parsed;
                    }
                    this.field.set_text(this.value.to_string());
                },
            ],
            extern_requests: [
                // The value, not whatever is half-typed into the field.
                GetText => (this, _q): String { this.value.to_string() },
            ],
        }}
    };
}

number_node! {
    /// A whole number, edited as text.
    Integer, i64, "An Integer"
}

number_node! {
    /// A decimal number, edited as text.
    Float, f64, "A Float"
}
