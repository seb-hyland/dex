use dex_core::prelude::*;

use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::layouts::horizontal_layout;
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
        let should_clip = ctx.constraints.should_clip;

        let origin = ctx.constraints.pos;

        // `label` then `param_name`, laid out in a row.
        let region = horizontal_layout(
            &mut ctx,
            &[self.label.erase(), self.param_name.erase()],
            ARG_LABEL_GAP,
            false,
            DrawConstraints {
                pos: origin,
                x: avail_w.map(AxisConstraint::AtMost),
                y: avail_h.map(AxisConstraint::AtMost),
                wrap: WrapConstraints::NotAllowed,
                should_clip,
            },
        )
        .region()
        .unwrap_or_else(|| ScreenRegion::from_min_size(origin, Vector { x: 0.0, y: 0.0 }));

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
        /// Vertical gap between the args row, dividing line, and editor
        const SECTION_GAP: f32 = 6.0;
        /// Thickness of the dividing line
        const DIVIDER_THICKNESS: f32 = 1.0;
        /// Horizontal gap between arguments
        const ARG_GAP: f32 = 2.0;
        /// Fallback width when the available width is unbounded
        const FALLBACK_WIDTH: f32 = 400.0;

        let constraints = ctx.constraints;
        let avail_w = constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = constraints.y.map(|a| a.provided_value());
        let divider_w = if avail_w.is_finite() {
            avail_w
        } else {
            FALLBACK_WIDTH
        };
        let should_clip = constraints.should_clip;

        let origin = constraints.pos;

        // Row of arguments.
        let args: Vec<_> = self.args.iter().map(|a| a.erase()).collect();
        let row_h = horizontal_layout(
            &mut ctx,
            &args,
            ARG_GAP,
            true,
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::AtMost(divider_w)),
                y: avail_h.map(AxisConstraint::AtMost),
                wrap: WrapConstraints::NotAllowed,
                should_clip,
            },
        )
        .region()
        .map(|r| r.size().y)
        .unwrap_or(0.0);

        // Dividing line beneath the argument row (a paint primitive).
        let divider_y = origin.y + row_h + SECTION_GAP;
        Line {
            span: Vector {
                x: divider_w,
                y: 0.0,
            },
            stroke: Stroke::new(DIVIDER_THICKNESS, Color32::GRAY),
        }
        .paint(
            ctx.ui.painter(),
            ScreenPos {
                x: origin.x,
                y: divider_y,
            },
        );

        // Code editor below the divider.
        let editor_y = divider_y + SECTION_GAP;
        let editor_region = ctx
            .draw_workspace_node(
                self.editor,
                DrawConstraints {
                    pos: ScreenPos {
                        x: origin.x,
                        y: editor_y,
                    },
                    x: Some(AxisConstraint::Exactly(divider_w)),
                    y: avail_h.map(|h| AxisConstraint::AtMost((origin.y + h - editor_y).max(0.0))),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip,
                },
            )
            .and_then(|r| r.region());

        let mut region = ScreenRegion::from_min_size(
            origin,
            Vector {
                x: divider_w,
                y: divider_y - origin.y,
            },
        );
        if let Some(er) = editor_region {
            region = region.union(er);
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { Lambda {} }
