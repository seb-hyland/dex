use dex_core::prelude::*;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::primitives::interaction::{InteractionBox, WasClicked};
use crate::primitives::shapes::Rect;
use crate::primitives::text::Label;

#[derive(Clone, Serialize, Deserialize)]
pub struct Button {
    pub label: Label,
    pub onclick_action: Action,

    /// Space between the label and the surrounding border on every side
    pub padding: f32,
    pub corner_radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,

    /// Click sensor covering the whole button
    interaction: InteractionBox,
}

impl Button {
    pub fn new(label: Label, action: Action) -> Self {
        let mut interaction = InteractionBox::default();
        interaction.senses_clicks = true;

        Self {
            label,
            onclick_action: action,
            padding: 4.0,
            corner_radius: 0.0,
            fill_color: Color32::TRANSPARENT,
            border: Stroke::new(1.0, Color32::GRAY),
            interaction,
        }
    }
}

#[typetag::serde]
impl Node for Button {
    fn type_name(&self) -> String {
        "Button".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let padding = self.padding;
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        let origin = match ctx.constraints.pos {
            PositionConstraint::TopLeft(tl) => tl,
            PositionConstraint::Center(_) => {
                let size = match ctx.this_node_last_frame_location().map(|r| r.size()) {
                    Some(s) => s,
                    None => {
                        ctx.request_skip_frame();
                        Vector::splat(50.0)
                    }
                };
                ctx.constraints.pos.to_top_left(size)
            }
        };

        let content_origin = origin + Vector::splat(padding);
        let label_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(content_origin),
            x: avail_w.map(|w| AxisConstraint::AtMost((w - 2.0 * padding).max(0.0))),
            y: avail_h.map(|h| AxisConstraint::AtMost((h - 2.0 * padding).max(0.0))),
            wrap: WrapConstraints::NotAllowed,
            should_clip: ctx.constraints.should_clip,
        };
        let label_result = ctx.draw_node(
            &self.label,
            NodeUid::new_local(ctx.id, "button label"),
            label_constraints,
        );
        let label_size = label_result
            .region()
            .map(|r| r.size())
            .unwrap_or(Vector { x: 0.0, y: 0.0 });

        let button_size = Vector {
            x: label_size.x + 2.0 * padding,
            y: label_size.y + 2.0 * padding,
        };
        let box_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(origin),
            x: Some(AxisConstraint::Exactly(button_size.x)),
            y: Some(AxisConstraint::Exactly(button_size.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: ctx.constraints.should_clip,
        };

        let border = Rect {
            size: button_size,
            corner_radius: self.corner_radius,
            fill_color: self.fill_color,
            border: self.border,
        };
        ctx.draw_node(
            &border,
            NodeUid::new_local(ctx.id, "button border"),
            box_constraints,
        );

        ctx.draw_node(
            &self.interaction,
            NodeUid::new_local(ctx.id, "button interaction"),
            box_constraints,
        );
        let clicked = self.interaction.request(WasClicked).unwrap_or(false);
        if clicked {
            ctx.workspace.submit_action_dyn(self.onclick_action.clone());
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, button_size)),
        }
    }
}

impl Reset for Button {
    fn reset(&self) {
        self.label.reset();
        self.interaction.reset();
    }
}

defhandlers! { Button {} }
