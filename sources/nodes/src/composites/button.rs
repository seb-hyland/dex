use dex_core::prelude::*;
use dex_core::theme;

use crate::primitives::icon::{Glyph, Icon};
use crate::primitives::interaction::{ContainsPointer, InteractionBox};
use crate::primitives::shapes::Rect;
use crate::primitives::text::Label;

/// A clickable button. Send it [`WasClicked`](crate::primitives::interaction::WasClicked) to poll it.
#[utils::dynamic_type]
#[utils::portable]
pub struct Button {
    pub label: Label,

    /// A glyph drawn before the label — or in place of it, when the label is
    /// empty and the control is too small for words.
    pub icon: Option<Glyph>,
    /// The gap between the icon and the label, when there is both.
    pub icon_gap: f32,

    /// Space between the content and the border, above and below.
    pub padding: f32,
    /// Extra space at the left and right, on top of [`Button::padding`].
    /// Text wants more room beside it than under it.
    pub padding_x: f32,
    pub corner_radius: f32,
    pub fill_color: Color,
    pub border: Stroke,

    /// The fill while the pointer is over the button.
    pub hover_fill: Color,
    /// The border while the pointer is over the button.
    pub hover_border: Stroke,
    /// The pointer shape offered while the button is hovered.
    pub cursor: CursorIcon,

    /// Stretch the button to the full width its parent offers.
    pub fill_width: bool,
    /// Stretch the button to the full height its parent offers, centring the content in it.
    pub fill_height: bool,

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
        // Hover is sensed so the button can light up under the pointer.
        let interaction = ws.insert_node(InteractionBox::sensing(true, true, false));
        let mut button = Self {
            label,
            icon: None,
            icon_gap: theme::SPACE_MD,
            padding: theme::SPACE_MD,
            padding_x: theme::SPACE_SM,
            corner_radius: theme::RADIUS_MD,
            fill_color: Color::TRANSPARENT,
            border: theme::border(),
            hover_fill: theme::SURFACE_ALT,
            hover_border: theme::border_hover(),
            cursor: CursorIcon::PointingHand,
            fill_width: false,
            fill_height: false,
            interaction,
        };
        configure(&mut button);
        ws.insert_node(button)
    }

    /// A square button showing `glyph` instead of a word: `+`, `×`, a chevron.
    pub fn build_icon(ws: WorkspaceActionHandle, glyph: Glyph) -> NodeUid<Button> {
        Self::build_with(ws, Label::new(String::new()), |b| {
            b.icon = Some(glyph);
            // A glyph is already square, so it needs no optical side padding.
            b.padding = theme::SPACE_SM;
            b.padding_x = 0.0;
        })
    }
}

impl Button {
    /// The glyph to draw, sized and coloured to match the label's text.
    fn icon(&self) -> Option<Icon> {
        self.icon
            .map(|glyph| Icon::new(glyph, self.label.font.size, self.label.shown_color()))
    }
}

#[utils::dynamic_node]
impl Node for Button {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Button".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        // The sensor is drawn after the frame, so what it saw was last frame.
        // The app repaints continuously, so the lag is not visible.
        let hovered = ctx
            .node
            .workspace
            .send_request(self.interaction, ContainsPointer)
            .unwrap_or(false);

        let (pad_x, pad_y) = (self.padding + self.padding_x, self.padding);
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        let origin = ctx.constraints.pos;

        let stretch = self
            .fill_height
            .then(|| avail_h.filter(|h| h.is_finite()))
            .flatten();
        let lift = stretch.map_or(0.0, |height| {
            let measured = if self.label.text.is_empty() {
                0.0
            } else {
                ctx.measure_node(
                    &self.label,
                    DrawConstraints {
                        pos: origin,
                        x: None,
                        y: None,
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: false,
                    },
                )
                .region()
                .map_or(0.0, |r| r.size().y)
            };
            let content = measured.max(self.icon().map_or(0.0, |i| i.size));
            ((height - content) * 0.5 - pad_y).max(0.0)
        });
        let content_origin = origin
            + Vector {
                x: pad_x,
                y: pad_y + lift,
            };
        let content_constraints = DrawConstraints {
            pos: content_origin,
            x: avail_w.map(|w| AxisConstraint::AtMost((w - 2.0 * pad_x).max(0.0))),
            y: avail_h.map(|h| AxisConstraint::AtMost((h - 2.0 * pad_y).max(0.0))),
            wrap: ctx.constraints.wrap,
            should_clip: ctx.constraints.should_clip,
        };

        let icon = self.icon();
        let lead = icon.map_or(0.0, |i| {
            i.size
                + if self.label.text.is_empty() {
                    0.0
                } else {
                    self.icon_gap
                }
        });

        // How big the button ends up, given what its content took.
        let sized = |content: Vector| {
            let mut size = Vector {
                x: content.x + 2.0 * pad_x,
                y: content.y + 2.0 * pad_y,
            };
            // An unbounded offer is not a width to fill.
            if self.fill_width
                && let Some(w) = avail_w
                && w.is_finite()
            {
                size.x = w;
            }
            if let Some(h) = stretch {
                size.y = h;
            }
            size
        };
        let (fill, border) = if hovered {
            (self.hover_fill, self.hover_border)
        } else {
            (self.fill_color, self.border)
        };

        // The frame sizes to its content, so it cannot be painted until that content has drawn.
        let content = ctx.with_backdrop(
            |ctx| {
                let label_result = (!self.label.text.is_empty()).then(|| {
                    ctx.draw_node(
                        &self.label,
                        DrawConstraints {
                            pos: content_origin + Vector { x: lead, y: 0.0 },
                            x: content_constraints.x.map(|a| {
                                AxisConstraint::AtMost((a.provided_value() - lead).max(0.0))
                            }),
                            ..content_constraints
                        },
                    )
                });
                // A label that ran out of room asks to wrap; nothing is framed.
                if let Some(DrawResult::Wrap { continuation, .. }) = label_result {
                    return Err(continuation);
                }
                let label_size = label_result
                    .and_then(|r| r.region())
                    .map(|r| r.size())
                    .unwrap_or_default();

                if let Some(icon) = icon {
                    // Centred against the label's line, so the two share a middle.
                    let drop = ((label_size.y - icon.size) * 0.5).max(0.0);
                    ctx.draw_node(
                        &icon,
                        DrawConstraints {
                            pos: content_origin + Vector { x: 0.0, y: drop },
                            ..content_constraints
                        },
                    );
                }

                Ok(Vector {
                    x: lead + label_size.x,
                    y: label_size.y.max(icon.map_or(0.0, |i| i.size)),
                })
            },
            |content| {
                content.as_ref().ok().map(|content| {
                    Rect {
                        size: sized(*content),
                        corner_radius: self.corner_radius,
                        fill_color: fill,
                        border,
                        stroke_kind: StrokeKind::Inside,
                    }
                    .shape(origin)
                })
            },
        );

        let content_size = match content {
            Ok(size) => size,
            // Pass the label's wrap request up.
            Err(continuation) => {
                return DrawResult::Wrap {
                    region: None,
                    continuation,
                };
            }
        };
        let button_size = sized(content_size);
        if hovered {
            ctx.set_cursor(self.cursor);
        }

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

defhandlers! { Button {
    actions: [
        SetButtonStyle { fill_color: Color, border: Stroke, text_color: Color } => (this, s) {
            this.fill_color = s.fill_color;
            this.border = s.border;
            this.label.color = s.text_color;
        },
        // Retitle the button, for one that stands for a thing it can undo.
        SetButtonLabel { text: String } => (this, s) {
            this.label.text = s.text;
        },
    ],
}}
