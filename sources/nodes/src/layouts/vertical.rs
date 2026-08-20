use dex_core::prelude::*;

/// Lay out `children` top-to-bottom, `spacing` apart.
pub fn vertical_layout(
    ctx: &mut DrawContext,
    children: &[NodeUid],
    spacing: f32,
    constraints: DrawConstraints,
) -> DrawResult {
    let avail_w = constraints
        .x
        .map(|x_ax| x_ax.provided_value())
        .unwrap_or(f32::INFINITY);
    let avail_h = constraints
        .y
        .map(|y_ax| y_ax.provided_value())
        .unwrap_or(f32::INFINITY);

    let origin = constraints.pos;

    let mut consumed_height = 0.0;
    let mut max_width = 0.0_f32;
    let mut is_first = true;

    for child in children.iter().copied() {
        let y_offset = if is_first {
            // No spacing on top of first child
            consumed_height
        } else {
            consumed_height + spacing
        };

        if y_offset >= avail_h {
            // No vertical space left
            break;
        }

        let child_constraints = DrawConstraints {
            pos: origin
                + Vector {
                    x: 0.0,
                    y: y_offset,
                },
            x: Some(AxisConstraint::AtMost(avail_w)),
            y: Some(AxisConstraint::AtMost(avail_h - y_offset)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: constraints.should_clip,
        };

        if let Some(region) = ctx
            .draw_workspace_node(child, child_constraints)
            .and_then(|res| res.region())
        {
            let size = region.size();
            consumed_height = y_offset + size.y;
            max_width = max_width.max(size.x);
            is_first = false;
        }
    }

    DrawResult::Complete {
        region: Some(ScreenRegion::from_min_size(
            origin,
            Vector {
                x: max_width,
                y: consumed_height,
            },
        )),
    }
}
