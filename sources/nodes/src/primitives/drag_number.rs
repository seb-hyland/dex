//! A number set by dragging across it.
//!
//! For a quantity you want to *find* rather than state: an angle, a weight, a
//! radius. Typing one into a field means guessing, looking, and guessing again;
//! dragging one shows the answer as you pass it.

use dex_core::prelude::*;
use dex_core::theme;

use crate::primitives::dropdown::{RowStyle, paint_row, row_height};
use crate::primitives::interaction::{InteractionBox, WasDragged, WasHovered};

/// A number changed by dragging horizontally across it.
#[utils::dynamic_type]
#[utils::portable]
pub struct DragNumber {
    pub value: f32,
    /// The range it stays inside.
    pub min: f32,
    pub max: f32,
    /// Whether it comes back round at the ends rather than stopping there.
    pub wraps: bool,
    /// What one point of pointer travel is worth.
    pub step: f32,
    /// How many decimals are shown.
    pub decimals: usize,
    /// Written after the number: a degree sign, a unit.
    pub suffix: String,
    /// Written before it, naming what it sets.
    pub prefix: String,

    sensor: NodeUid<InteractionBox>,
}

#[utils::dynamic_methods]
impl DragNumber {
    /// A whole-numbered control over `min..=max`, one unit per point dragged.
    pub fn build(ws: WorkspaceActionHandle, value: f32, min: f32, max: f32) -> NodeUid<DragNumber> {
        ws.insert_node(Self {
            value,
            min,
            max,
            wraps: false,
            step: 1.0,
            decimals: 0,
            suffix: String::new(),
            prefix: String::new(),
            sensor: ws.insert_node(InteractionBox::sensing(true, false, true)),
        })
    }

    /// The same, with the style adjusted before it is inserted.
    pub fn build_with(
        ws: WorkspaceActionHandle,
        value: f32,
        min: f32,
        max: f32,
        configure: impl FnOnce(&mut Self),
    ) -> NodeUid<DragNumber> {
        let mut control = Self {
            value,
            min,
            max,
            wraps: false,
            step: 1.0,
            decimals: 0,
            suffix: String::new(),
            prefix: String::new(),
            sensor: ws.insert_node(InteractionBox::sensing(true, false, true)),
        };
        configure(&mut control);
        ws.insert_node(control)
    }
}

impl DragNumber {
    /// `value` brought inside the range, round the ends or stopped at them.
    fn bounded(&self, value: f32) -> f32 {
        let span = self.max - self.min;
        if !self.wraps || span <= f32::EPSILON {
            return value.clamp(self.min, self.max);
        }
        self.min + (value - self.min).rem_euclid(span)
    }

    /// What the row reads.
    fn shown(&self) -> String {
        format!(
            "{}{:.*}{}",
            self.prefix, self.decimals, self.value, self.suffix
        )
    }
}

#[utils::dynamic_node]
impl Node for DragNumber {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Draggable Number".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let style = RowStyle {
            font: theme::text_small(),
            color: theme::INK_MUTED,
            boxed: true,
            padded: true,
        };
        let shown = self.shown();
        let height = row_height(&ctx, &shown, style);
        let width = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .filter(|w| w.is_finite())
            .unwrap_or(120.0);
        let region = ScreenRegion::from_min_size(
            ctx.constraints.pos,
            Vector {
                x: width,
                y: height,
            },
        );

        let ws = ctx.node.workspace;
        let hovered = ws.send_request(self.sensor, WasHovered).unwrap_or(false);
        paint_row(&mut ctx, region, &shown, style, hovered, false);
        if hovered {
            ctx.set_cursor(CursorIcon::ResizeHorizontal);
        }

        ctx.draw_workspace_node(
            self.sensor.erase(),
            DrawConstraints {
                pos: region.min,
                x: Some(AxisConstraint::Exactly(width)),
                y: Some(AxisConstraint::Exactly(height)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        // Sideways travel only: a hand moving diagonally is still asking for
        // more or for less, and folding the vertical in would make it jumpy.
        if let Some(delta) = ws.send_request(self.sensor, WasDragged).flatten() {
            let moved = self.bounded(self.value + delta.x * self.step);
            if moved != self.value {
                ctx.submit_action_for_self::<Self, _>(
                    SetDragNumber { value: moved },
                    "Dragged a number",
                );
            }
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.sensor.erase());
    }
}

defhandlers! { DragNumber {
    actions: [
        SetDragNumber { value: f32 } => (this, a) {
            this.value = this.bounded(a.value);
        },
    ],
    requests: [
        DragNumberValue => (this, _q): f32 { this.value },
    ],
}}
