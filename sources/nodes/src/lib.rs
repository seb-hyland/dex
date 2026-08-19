pub mod composites;
pub mod layouts;
pub mod primitives;

use dex_core::prelude::*;
use utils::Transient;

pub fn resolve_center_origin(
    ctx: &mut DrawContext,
    last_size: &Transient<Vector>,
    fallback: Vector,
) -> ScreenPos {
    match ctx.constraints.pos {
        PositionConstraint::TopLeft(tl) => tl,
        PositionConstraint::Center(_) => {
            let estimate = match *last_size.val() {
                Some(size) => size,
                None => {
                    // We're going to guess; don't draw this frame.
                    ctx.request_skip_frame();
                    fallback
                }
            };
            ctx.constraints.pos.to_top_left(estimate)
        }
    }
}
