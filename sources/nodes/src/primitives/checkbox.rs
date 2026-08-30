use dex_core::prelude::*;
use egui::{Pos2, text::LayoutJob};
use utils::Transient;

use crate::primitives::interaction::{InteractionBox, TakeClicked, WasHovered};
use crate::primitives::shapes::Rect;

/// Space between the tick box and its label.
const GAP: f32 = 6.0;
/// How much shorter than a text row the box is, so it sits inside the line.
const BOX_INSET: f32 = 2.0;
const MIN_SIDE: f32 = 10.0;

/**
    A labelled tick box that owns its own state.
    Read it with [`IsChecked`].
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct Checkbox {
    pub label: String,
    pub checked: bool,

    pub font: Font,
    pub color: Color,

    /// A toggle nobody has picked up yet.
    toggled: Transient<bool>,
    interaction: NodeUid<InteractionBox>,
}

#[utils::dynamic_methods]
impl Checkbox {
    /// Build a tick box into `ws` and return its id.
    pub fn build(ws: WorkspaceActionHandle, label: String, checked: bool) -> NodeUid<Checkbox> {
        Self::build_with(ws, label, checked, |_| {})
    }

    /// Like [`Checkbox::build`], but `configure` may adjust the style before the
    /// box is inserted.
    pub fn build_with(
        ws: WorkspaceActionHandle,
        label: String,
        checked: bool,
        configure: impl FnOnce(&mut Self),
    ) -> NodeUid<Checkbox> {
        let interaction = ws.insert_node(InteractionBox::sensing(true, true, false));
        let mut tick = Self {
            label,
            checked,
            font: Font::proportional(13.0),
            color: Color::BLACK,
            toggled: Transient::default(),
            interaction,
        };
        configure(&mut tick);
        ws.insert_node(tick)
    }

    /// What the box is showing: a click the user has just made, until the action carrying it lands and `checked` catches up.
    fn shown(&self) -> bool {
        self.toggled.val().unwrap_or(self.checked)
    }
}

#[utils::dynamic_node]
impl Node for Checkbox {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Checkbox".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let mut job = LayoutJob::single_section(
            self.label.clone(),
            self.font.text_format(ctx.ui.ctx(), self.color),
        );
        // A tick box's label is one line, however long it is.
        job.break_on_newline = false;
        let galley = ctx.ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));

        let row_h = galley.rows[0].height();
        let side = (row_h - 2.0 * BOX_INSET).max(MIN_SIDE);
        let content_w = side + GAP + galley.rect.width();

        let avail_w = ctx.constraints.x.map(|axis| axis.provided_value());
        if let Some(w) = avail_w
            && content_w > w
            && ctx.constraints.wrap.can_retry_on_newline()
        {
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }
        let width = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) => w,
            _ => avail_w.map_or(content_w, |w| content_w.min(w)),
        };
        let height = match ctx.constraints.y {
            Some(AxisConstraint::Exactly(h)) => h,
            _ => row_h,
        };

        let origin = ctx.constraints.pos;
        let size = Vector {
            x: width,
            y: height,
        };
        let region = ScreenRegion::from_min_size(origin, size);

        // The whole row is the sensor, so the label is as clickable as the box.
        ctx.draw_workspace_node(
            self.interaction.erase(),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(size.x)),
                y: Some(AxisConstraint::Exactly(size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: ctx.constraints.should_clip,
            },
        );
        let ws = ctx.node.workspace;
        let hovered = ws
            .send_request(self.interaction, WasHovered)
            .unwrap_or(false);
        // The choice the user just made outranks the value still in flight.
        let checked = self.shown();

        let box_tl = origin
            + Vector {
                x: 0.0,
                y: (height - side) * 0.5,
            };
        Rect {
            size: Vector::splat(side),
            corner_radius: 3.0,
            fill_color: match (checked, hovered) {
                (true, _) => Color::rgb(64, 108, 196),
                (false, true) => Color::gray(232),
                (false, false) => Color::TRANSPARENT,
            },
            border: Stroke::new(1.0, Color::gray(if hovered { 120 } else { 160 })),
            stroke_kind: StrokeKind::Inside,
        }
        .paint(ctx.ui.painter(), box_tl);

        if checked {
            let tick = egui::Stroke::new(1.6, egui::Color32::WHITE);
            let at = |x: f32, y: f32| {
                Pos2::from(
                    box_tl
                        + Vector {
                            x: side * x,
                            y: side * y,
                        },
                )
            };
            let painter = ctx.ui.painter();
            painter.line_segment([at(0.22, 0.52), at(0.44, 0.74)], tick);
            painter.line_segment([at(0.44, 0.74), at(0.78, 0.28)], tick);
        }

        let text_pos = Pos2 {
            x: origin.x + side + GAP,
            y: origin.y + (height - galley.rect.height()) * 0.5,
        };
        ctx.ui.painter().galley(text_pos, galley, self.color.into());

        // Taken, so one click is one toggle even where the box is drawn twice.
        if ws
            .send_request(self.interaction, TakeClicked)
            .unwrap_or(false)
        {
            let on = !checked;
            self.toggled.set(on);
            ctx.submit_action_for_self::<Self, _>(SetChecked { on }, "Toggled a checkbox");
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.interaction.erase());
    }
}

defhandlers! { Checkbox {
    actions: [
        // Landing the click clears the value that was standing in for it.
        SetChecked { on: bool } => (this, s) {
            this.checked = s.on;
            *this.toggled.val_mut() = None;
        },
    ],
    requests: [
        // What the box is showing, click included. Polled, never consumed.
        IsChecked => (this, _q): bool { this.shown() },
    ],
}}
