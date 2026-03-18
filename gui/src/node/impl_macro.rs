#[macro_export]
macro_rules! impl_NodeDynamics {
    (for $type_name:ty where variants = { $($variant:ident),+ }) => {
        impl NodeDynamics for $type_name {
            fn draw(&mut self, ctx: &mut DrawContext<'_>) -> DrawInteraction {
                match self {
                    $(
                        Self::$variant(inner) => inner.draw(ctx),
                    )*
                }
            }

            fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2 {
                match self {
                    $(
                        Self::$variant(inner) => inner.size(ctx),
                    )*
                }
            }
        }
    };
}
