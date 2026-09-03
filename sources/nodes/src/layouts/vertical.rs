use dex_core::prelude::*;

use crate::layouts::child::{LayoutChild, Sizing};

/// Lay out `children` top-to-bottom, `spacing` apart.
#[utils::dynamic_type]
#[utils::portable]
pub struct VerticalLayout {
    // Composed from node handles by `build`; `LayoutChild` isn't bindable.
    #[dynamic(skip)]
    pub children: Vec<LayoutChild>,
    pub spacing: f32,
    /// How much height each child asks for, by index.
    /// Anything unmentioned is considered [`Sizing::Fixed`]
    pub sizing: Vec<Sizing>,
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
            sizing: Vec::new(),
        })
    }

    /// A column composed from dynamic children (values or node handles).
    pub fn new(children: Vec<LayoutChild>, spacing: f32) -> VerticalLayout {
        VerticalLayout {
            children,
            spacing,
            sizing: Vec::new(),
        }
    }

    /// A column whose last child takes everything the others leave.
    pub fn filling_last(children: Vec<LayoutChild>, spacing: f32) -> VerticalLayout {
        let sizing = Sizing::fill_last(children.len());
        Self::sized(children, spacing, sizing)
    }

    /// A column in which each child takes the height `sizing` gives it.
    pub fn sized(children: Vec<LayoutChild>, spacing: f32, sizing: Vec<Sizing>) -> VerticalLayout {
        VerticalLayout {
            children,
            spacing,
            sizing,
        }
    }
}

impl VerticalLayout {
    /// What the child at `index` asks for. Unmentioned children are fixed.
    fn sizing_of(&self, index: usize) -> Sizing {
        self.sizing.get(index).copied().unwrap_or_default()
    }

    /// Whether any child wants a share of the leftovers.
    fn has_fill(&self, avail_y: f32) -> bool {
        avail_y.is_finite() && (0..self.children.len()).any(|i| self.sizing_of(i) == Sizing::Fill)
    }

    /// The constraints for a child sitting `offset` below the top.
    fn child_constraints(
        &self,
        origin: ScreenPos,
        offset: f32,
        avail: Vector,
        height: Option<f32>,
        should_clip: bool,
    ) -> DrawConstraints {
        DrawConstraints {
            pos: origin + Vector { x: 0.0, y: offset },
            x: Some(AxisConstraint::AtMost(avail.x)),
            y: Some(match height {
                Some(h) => AxisConstraint::Exactly(h),
                None => AxisConstraint::AtMost((avail.y - offset).max(0.0)),
            }),
            wrap: WrapConstraints::NotAllowed,
            should_clip,
        }
    }
}

#[utils::dynamic_node]
impl Node for VerticalLayout {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Vertical Layout".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail = ctx.constraints.available();
        let origin = ctx.constraints.pos;
        let clip = ctx.constraints.should_clip;
        let filling = self.has_fill(avail.y);

        // What every fixed child comes to, measured up front.
        let fixed_heights: Vec<f32> = if filling {
            (0..self.children.len())
                .map(|i| match self.sizing_of(i) {
                    Sizing::Fill => 0.0,
                    Sizing::Fixed => self.children[i]
                        .measure(
                            &mut ctx,
                            self.child_constraints(origin, 0.0, avail, None, clip),
                        )
                        .region()
                        .map_or(0.0, |r| r.size().y),
                })
                .collect()
        } else {
            Vec::new()
        };

        let mut y = 0.0;
        let mut max_width = 0.0_f32;
        let mut drew_any = false;

        for (i, child) in self.children.iter().enumerate() {
            let offset = if drew_any { y + self.spacing } else { y };
            if offset >= avail.y {
                // No vertical space left.
                break;
            }

            let height = match self.sizing_of(i) {
                Sizing::Fill if filling => {
                    // Everything below this child that is already spoken for:
                    // the fixed heights, and a gap before each of them.
                    let below: f32 = (i + 1..self.children.len())
                        .map(|j| fixed_heights[j] + self.spacing)
                        .sum();
                    // Shared equally with the fills that have yet to be drawn,
                    // so one that takes less than its share leaves more.
                    let fills_left = (i..self.children.len())
                        .filter(|j| self.sizing_of(*j) == Sizing::Fill)
                        .count()
                        .max(1) as f32;
                    Some(((avail.y - offset - below) / fills_left).max(0.0))
                }
                _ => None,
            };

            let constraints = self.child_constraints(origin, offset, avail, height, clip);
            if let Some(region) = child.draw(&mut ctx, constraints).region() {
                let size = region.size();
                // A filling child occupies what it was given, even if it drew less.
                y = offset + height.unwrap_or(size.y).max(size.y);
                max_width = max_width.max(size.x);
                drew_any = true;
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: max_width,
                    // A column that fills has claimed the whole height.
                    y: if filling { avail.y } else { y },
                },
            )),
        }
    }
}

defhandlers! { VerticalLayout {} }
