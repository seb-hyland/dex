use dex_core::prelude::*;

use crate::layouts::child::LayoutChild;

/// Lay out `children` top-to-bottom, `spacing` apart.
#[utils::dynamic_type]
#[utils::portable]
pub struct VerticalLayout {
    // Composed from node handles by `build`; `LayoutChild` isn't bindable.
    #[dynamic(skip)]
    pub children: Vec<LayoutChild>,
    pub spacing: f32,
    /// Give the last child all remaining vertical space, and always report the full available height.
    pub fill_last: bool,
}

#[utils::dynamic_methods]
impl VerticalLayout {
    /// Build a column of workspace-node `children` into `ws`.
    pub fn build(
        ws: WorkspaceActionHandle,
        children: Vec<NodeUid>,
        spacing: f32,
    ) -> NodeUid<VerticalLayout> {
        ws.insert_node(Self {
            children: children.into_iter().map(LayoutChild::Id).collect(),
            spacing,
            fill_last: false,
        })
    }

    /// A column composed from dynamic children (values or node handles).
    pub fn new(children: Vec<LayoutChild>, spacing: f32) -> VerticalLayout {
        VerticalLayout {
            children,
            spacing,
            fill_last: false,
        }
    }
}

#[utils::dynamic_node]
impl Node for VerticalLayout {
    fn type_name(&self) -> String {
        "Vertical Layout".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx
            .constraints
            .x
            .map(|x_ax| x_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = ctx
            .constraints
            .y
            .map(|y_ax| y_ax.provided_value())
            .unwrap_or(f32::INFINITY);

        let mut consumed_height = 0.0;
        let mut max_width = 0.0_f32;
        let mut is_first = true;

        // Filling the last child only makes sense with a bounded height to fill.
        let fill_last = self.fill_last && avail_h.is_finite();
        let last_index = self.children.len().saturating_sub(1);

        for (i, child) in self.children.iter().enumerate() {
            let y_offset = if is_first {
                // No spacing on top of first child
                consumed_height
            } else {
                consumed_height + self.spacing
            };

            if y_offset >= avail_h {
                // No vertical space left
                break;
            }

            let remaining = avail_h - y_offset;
            let is_last = i == last_index;
            let y_constraint = if is_last && fill_last {
                // Hand the last child every remaining pixel.
                AxisConstraint::Exactly(remaining)
            } else {
                AxisConstraint::AtMost(remaining)
            };

            let child_constraints = DrawConstraints {
                pos: ctx.constraints.pos
                    + Vector {
                        x: 0.0,
                        y: y_offset,
                    },
                x: Some(AxisConstraint::AtMost(avail_w)),
                y: Some(y_constraint),
                wrap: WrapConstraints::NotAllowed,
                should_clip: ctx.constraints.should_clip,
            };

            if let Some(region) = child.draw(&mut ctx, child_constraints).region() {
                let size = region.size();
                consumed_height = y_offset + size.y;
                max_width = max_width.max(size.x);
                is_first = false;
            }
        }

        // Reserve the whole height when filling.
        if fill_last {
            consumed_height = avail_h;
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                ctx.constraints.pos,
                Vector {
                    x: max_width,
                    y: consumed_height,
                },
            )),
        }
    }
}

defhandlers! { VerticalLayout {} }
