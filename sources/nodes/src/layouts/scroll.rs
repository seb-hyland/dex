use dex_core::prelude::*;

use crate::layouts::child::LayoutChild;

/// Wraps a child in a scroll area, clamping it to the given viewport and scrolling its overflow.
#[utils::dynamic_type]
#[utils::portable]
pub struct ScrollLayout {
    #[dynamic(skip)]
    pub child: LayoutChild,
    pub horizontal: bool,
    pub vertical: bool,
}

#[utils::dynamic_methods]
impl ScrollLayout {
    pub fn vertical(child: LayoutChild) -> ScrollLayout {
        ScrollLayout {
            child,
            horizontal: false,
            vertical: true,
        }
    }

    pub fn horizontal(child: LayoutChild) -> ScrollLayout {
        ScrollLayout {
            child,
            horizontal: true,
            vertical: false,
        }
    }

    pub fn both(child: LayoutChild) -> ScrollLayout {
        ScrollLayout {
            child,
            horizontal: true,
            vertical: true,
        }
    }
}

#[utils::dynamic_node]
impl Node for ScrollLayout {
    fn type_name(&self) -> String {
        "Scroll Layout".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value()).unwrap_or(0.0);
        let avail_h = ctx.constraints.y.map(|a| a.provided_value()).unwrap_or(0.0);
        let origin = ctx.constraints.pos;
        let viewport = egui::Rect::from_min_size(origin.into(), egui::vec2(avail_w, avail_h));

        let node = ctx.node;
        let horizontal = self.horizontal;
        let vertical = self.vertical;

        ctx.ui
            .scope_builder(egui::UiBuilder::new().max_rect(viewport), |ui| {
                egui::ScrollArea::new([horizontal, vertical])
                    // auto-shrink true unless dimension matches `AxisConstraint::Exactly`
                    .auto_shrink([
                        !ctx.constraints
                            .x
                            .is_some_and(|c| matches!(c, AxisConstraint::Exactly(_))),
                        !ctx.constraints
                            .y
                            .is_some_and(|c| matches!(c, AxisConstraint::Exactly(_))),
                    ])
                    .show(ui, |inner| {
                        let content_origin: ScreenPos = inner.cursor().min.into();
                        let child_constraints = DrawConstraints {
                            pos: content_origin,
                            // Unbounded on scrolled axes so the child reports its full extent.
                            x: if horizontal {
                                None
                            } else {
                                Some(AxisConstraint::Exactly(avail_w))
                            },
                            y: if vertical {
                                None
                            } else {
                                Some(AxisConstraint::Exactly(avail_h))
                            },
                            wrap: WrapConstraints::NotAllowed,
                            // The inner ui already clips to the viewport.
                            should_clip: false,
                        };
                        let mut sub = DrawContext::for_ui(node, child_constraints, inner);
                        let res = self.child.draw(&mut sub, child_constraints);
                        // Tell egui how big the content is so the scrollbar is correct.
                        if let Some(region) = res.region() {
                            let size = region.size();
                            inner.allocate_space(egui::vec2(size.x, size.y));
                        }
                    });
            });

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: avail_w,
                    y: avail_h,
                },
            )),
        }
    }
}

defhandlers! { ScrollLayout {} }
