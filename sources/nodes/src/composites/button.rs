use dex_core::prelude::*;

use crate::primitives::interaction::InteractionBox;
use crate::primitives::shapes::Rect;
use crate::primitives::text::Label;

/// A clickable button. Send it [`WasClicked`](crate::primitives::interaction::WasClicked) to poll it.
#[utils::dynamic_type]
#[utils::portable]
pub struct Button {
    pub label: Label,

    /// Space between the label and the surrounding border on every side
    pub padding: f32,
    pub corner_radius: f32,
    pub fill_color: Color,
    pub border: Stroke,
    /// Stretch the button to the full width its parent offers.
    pub fill_width: bool,

    interaction: NodeUid<InteractionBox>,
}

#[utils::dynamic_methods]
impl Button {
    /// Build a button and return its id.
    pub fn build(ws: WorkspaceActionHandle, label: Label) -> NodeUid<Button> {
        Self::build_with(ws, label, |_| {})
    }

    /// Like [`Button::build`], but `configure` may adjust the visual style before the button is inserted.
    pub fn build_with(
        ws: WorkspaceActionHandle,
        label: Label,
        configure: impl FnOnce(&mut Self),
    ) -> NodeUid<Button> {
        let interaction = ws.insert_node(InteractionBox::sensing(false, true, false));
        let mut button = Self {
            label,
            padding: 4.0,
            corner_radius: 0.0,
            fill_color: Color::TRANSPARENT,
            border: Stroke::new(1.0, Color::GRAY),
            fill_width: false,
            interaction,
        };
        configure(&mut button);
        ws.insert_node(button)
    }
}

#[utils::dynamic_node]
impl Node for Button {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Button".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let padding = self.padding;
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        let origin = ctx.constraints.pos;

        let content_origin = origin + Vector::splat(padding);
        let label_constraints = DrawConstraints {
            pos: content_origin,
            x: avail_w.map(|w| AxisConstraint::AtMost((w - 2.0 * padding).max(0.0))),
            y: avail_h.map(|h| AxisConstraint::AtMost((h - 2.0 * padding).max(0.0))),
            wrap: ctx.constraints.wrap,
            should_clip: ctx.constraints.should_clip,
        };
        let label_result = ctx.draw_node(&self.label, label_constraints);
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

        let mut button_size = Vector {
            x: label_size.x + 2.0 * padding,
            y: label_size.y + 2.0 * padding,
        };

        // An unbounded offer is not a width to fill.
        if self.fill_width
            && let Some(w) = avail_w
            && w.is_finite()
        {
            button_size.x = w;
        }

        let border = Rect {
            size: button_size,
            corner_radius: self.corner_radius,
            fill_color: self.fill_color,
            border: self.border,
            stroke_kind: StrokeKind::Inside,
        };
        border.paint(ctx.ui.painter(), origin);

        ctx.draw_workspace_node(
            self.interaction.erase(),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(button_size.x)),
                y: Some(AxisConstraint::Exactly(button_size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: ctx.constraints.should_clip,
            },
        );

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, button_size)),
        }
    }

    fn deref_target(&self) -> Option<NodeUid> {
        // Polling messages fall through to the click sensor.
        Some(self.interaction.erase())
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.interaction.erase());
    }
}

defhandlers! { Button {} }
