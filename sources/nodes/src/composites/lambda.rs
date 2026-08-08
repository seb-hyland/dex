use dex_core::prelude::*;

use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::layouts::LayoutChild;
use crate::layouts::horizontal::HorizontalLayout;
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::shapes::Line;
use crate::primitives::text::{CodeEditor, LabelEditable};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct LambdaEditor {
    active: LambdaLang,
    steel: NodeUid<CodeEditor>,
    python: NodeUid<CodeEditor>,
}

#[typetag::serde]
impl Node for LambdaEditor {
    fn type_name(&self) -> String {
        "Lambda Editor".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        // Delegate entirely to whichever code editor is currently active.
        let active = self
            .deref_target()
            .expect("Deref to active editor should be implemented");
        let constraints = ctx.constraints;
        ctx.draw_workspace_node(active, constraints)
            .unwrap_or(DrawResult::Complete { region: None })
    }

    fn deref_target(&self) -> Option<NodeUid> {
        Some(match self.active {
            LambdaLang::Steel => self.steel.erase(),
            LambdaLang::Python => self.python.erase(),
        })
    }
}

defhandlers! { LambdaEditor {} }

#[derive(Clone, Reset, Serialize, Deserialize)]
pub enum LambdaLang {
    Steel,
    Python,
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct LambdaArg {
    label: NodeUid<LabelEditable>,
    param_name: NodeUid<LabelEditable>,
}

#[typetag::serde]
impl Node for LambdaArg {
    fn type_name(&self) -> String {
        "Lambda Argument".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        /// Horizontal gap drawn between the label and the parameter name.
        const ARG_LABEL_GAP: f32 = 4.0;

        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        let origin = resolve_origin(&mut ctx, Vector::splat(50.0));

        // Draw the label at the left
        let label_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(origin),
            x: avail_w.map(AxisConstraint::AtMost),
            y: avail_h.map(AxisConstraint::AtMost),
            wrap: WrapConstraints::NotAllowed,
            should_clip: ctx.constraints.should_clip,
        };
        let label_region = ctx
            .draw_workspace_node(self.label.erase(), label_constraints)
            .and_then(|r| r.region());
        let label_size = label_region
            .map(|r| r.size())
            .unwrap_or(Vector { x: 0.0, y: 0.0 });

        // Draw the parameter name after a small gap
        let param_origin = origin
            + Vector {
                x: label_size.x + ARG_LABEL_GAP,
                y: 0.0,
            };
        let param_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(param_origin),
            x: avail_w.map(|w| AxisConstraint::AtMost((w - label_size.x - ARG_LABEL_GAP).max(0.0))),
            y: avail_h.map(AxisConstraint::AtMost),
            wrap: WrapConstraints::NotAllowed,
            should_clip: ctx.constraints.should_clip,
        };
        let param_region = ctx
            .draw_workspace_node(self.param_name, param_constraints)
            .and_then(|r| r.region());

        // The occupied region is the union of both drawn pieces
        let mut region = ScreenRegion::from_min_size(origin, Vector { x: 0.0, y: 0.0 });
        if let Some(l) = label_region {
            region = region.union(l);
        }
        if let Some(p) = param_region {
            region = region.union(p);
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { LambdaArg {} }

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Lambda {
    args: Vec<NodeUid<LambdaArg>>,
    editor: NodeUid<LambdaEditor>,
}

#[typetag::serde]
impl Node for Lambda {
    fn type_name(&self) -> String {
        "Lambda".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        /// Vertical gap between args, dividing line, editor
        const SECTION_GAP: f32 = 6.0;
        /// Thickness of the dividing line
        const DIVIDER_THICKNESS: f32 = 1.0;
        /// Fallback width for the dividing line when the available width is unbounded
        const FALLBACK_WIDTH: f32 = 400.0;

        let constraints = ctx.constraints;
        let avail_w = constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);
        let divider_w = if avail_w.is_finite() {
            avail_w
        } else {
            FALLBACK_WIDTH
        };

        // Vertical stack of the args, dividing line, code editor
        let body = VerticalLayout {
            spacing: SECTION_GAP,
            children: vec![
                LayoutChild::local(HorizontalLayout {
                    children: self
                        .args
                        .iter()
                        .copied()
                        .map(NodeUid::erase)
                        .map(LayoutChild::Workspace)
                        .collect(),
                    allow_wrap: true,
                    wrap_spacing: 2.0,
                }),
                LayoutChild::local(Line {
                    span: Vector {
                        x: divider_w,
                        y: 0.0,
                    },
                    stroke: Stroke::new(DIVIDER_THICKNESS, Color32::GRAY),
                }),
                LayoutChild::Workspace(self.editor.erase()),
            ],
        };

        ctx.draw_node(
            &body,
            NodeUid::new_local(ctx.id, "lambda body"),
            constraints,
        )
    }
}

defhandlers! { Lambda {} }

/// Resolve the top-left origin for a content-sized node, with a fallback "guess" size
fn resolve_origin(ctx: &mut DrawContext, fallback: Vector) -> ScreenPos {
    match ctx.constraints.pos {
        PositionConstraint::TopLeft(tl) => tl,
        PositionConstraint::Center(_) => {
            let last_known_size = ctx.this_node_last_frame_location().map(|reg| reg.size());
            let estimate = match last_known_size {
                Some(s) => s,
                None => {
                    ctx.request_skip_frame();
                    fallback
                }
            };
            ctx.constraints.pos.to_top_left(estimate)
        }
    }
}
