use dex_core::prelude::*;
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::primitives::interaction::{InteractionBox, WasClicked};
use crate::primitives::shapes::Rect;
use crate::primitives::text::Label;
use crate::resolve_center_origin;

#[derive(Clone, Serialize, Deserialize)]
pub struct Button {
    pub label: Label,
    pub onclick_action: Action,

    /// Space between the label and the surrounding border on every side
    pub padding: f32,
    pub corner_radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
    last_size: Transient<Vector>,

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
            last_size: Transient::default(),
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

        let origin = resolve_center_origin(&mut ctx, &self.last_size, Vector::splat(50.0));

        let content_origin = origin + Vector::splat(padding);
        let label_constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(content_origin),
            x: avail_w.map(|w| AxisConstraint::AtMost((w - 2.0 * padding).max(0.0))),
            y: avail_h.map(|h| AxisConstraint::AtMost((h - 2.0 * padding).max(0.0))),
            wrap: ctx.constraints.wrap,
            should_clip: ctx.constraints.should_clip,
        };
        let label_result = ctx.draw_node(
            &self.label,
            NodeUid::new_local(ctx.node.id, "button label"),
            label_constraints,
        );
        // If the label couldn't fit and requested a new line, pass that request up.
        if let DrawResult::Wrap { continuation, .. } = label_result {
            return DrawResult::Wrap {
                region: None,
                continuation,
            };
        }
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
            NodeUid::new_local(ctx.node.id, "button border"),
            box_constraints,
        );

        ctx.draw_node(
            &self.interaction,
            NodeUid::new_local(ctx.node.id, "button interaction"),
            box_constraints,
        );
        let clicked = self
            .interaction
            .request(WasClicked, ctx.node)
            .unwrap_or(false);
        if clicked {
            ctx.node
                .workspace
                .submit_action_dyn(self.onclick_action.clone());
        }

        self.last_size.set(button_size);

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, button_size)),
        }
    }
}

impl Reset for Button {
    fn reset(&self) {
        self.label.reset();
        self.interaction.reset();
        self.last_size.reset();
    }
}

defhandlers! { Button {} }
