use dex_core::prelude::*;
use dex_core::theme;
use utils::Transient;

use crate::primitives::interaction::{InteractionBox, WasClicked};
use crate::primitives::text::Label;

/// A dropdown that selects one of a fixed list of string options by index.
#[utils::portable]
pub struct SelectionBox {
    pub options: Vec<String>,
    pub selected: usize,
    /// Whether the dropdown is currently expanded (transient UI state).
    open: Transient<bool>,
    header: NodeUid<InteractionBox>,
    option_sensors: Vec<NodeUid<InteractionBox>>,
}

impl SelectionBox {
    /// Build a selection box over `options` into `ws`, selecting the first.
    pub fn build(ws: WorkspaceActionHandle, options: Vec<String>) -> NodeUid<SelectionBox> {
        let header = ws.insert_node(InteractionBox::sensing(false, true, false));
        let option_sensors: Vec<_> = options
            .iter()
            .map(|_| ws.insert_node(InteractionBox::sensing(false, true, false)))
            .collect();
        ws.insert_node(Self {
            options,
            selected: 0,
            open: Transient::default(),
            header,
            option_sensors,
        })
    }
}

const ROW_H: f32 = 26.0;
const PAD: f32 = theme::SPACE_MD;

#[utils::dynamic_node(skip)]
impl Node for SelectionBox {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Selection Box".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let width = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(160.0);
        let origin = ctx.constraints.pos;

        let mut is_open = (*self.open.val()).unwrap_or(false);

        // Header row: the current selection plus a caret.
        draw_row_bg(&mut ctx, origin, width, egui::Color32::WHITE, true);
        let selected_text = self.options.get(self.selected).cloned().unwrap_or_default();
        draw_row_label(&mut ctx, origin, width, &selected_text);
        ctx.draw_workspace_node(self.header.erase(), exact_box(origin, width));
        if ctx
            .node
            .workspace
            .send_request(self.header, WasClicked)
            .unwrap_or(false)
        {
            is_open = !is_open;
            self.open.set(is_open);
        }

        let mut height = ROW_H;
        if is_open {
            for (i, option) in self.options.iter().enumerate() {
                let row_origin = origin + Vector { x: 0.0, y: height };
                draw_row_bg(
                    &mut ctx,
                    row_origin,
                    width,
                    egui::Color32::from_gray(245),
                    false,
                );
                draw_row_label(&mut ctx, row_origin, width, option);
                ctx.draw_workspace_node(
                    self.option_sensors[i].erase(),
                    exact_box(row_origin, width),
                );
                if ctx
                    .node
                    .workspace
                    .send_request(self.option_sensors[i], WasClicked)
                    .unwrap_or(false)
                {
                    ctx.submit_action_for_self::<Self, _>(
                        SetSelected { index: i },
                        "Select option",
                    );
                    self.open.set(false);
                }
                height += ROW_H;
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: width,
                    y: height,
                },
            )),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.header.erase());
        for sensor in &self.option_sensors {
            ctx.workspace.delete_node(sensor.erase());
        }
    }
}

/// An exactly-sized, one-row draw region.
fn exact_box(pos: ScreenPos, width: f32) -> DrawConstraints {
    DrawConstraints {
        pos,
        x: Some(AxisConstraint::Exactly(width)),
        y: Some(AxisConstraint::Exactly(ROW_H)),
        wrap: WrapConstraints::NotAllowed,
        should_clip: false,
    }
}

fn draw_row_bg(
    ctx: &mut DrawContext,
    pos: ScreenPos,
    width: f32,
    fill: egui::Color32,
    border: bool,
) {
    let region = ScreenRegion::from_min_size(pos, Vector { x: width, y: ROW_H });
    let stroke = if border {
        egui::Stroke::new(1.0, egui::Color32::from_gray(170))
    } else {
        egui::Stroke::NONE
    };
    ctx.ui
        .painter()
        .rect(region.into(), 3.0, fill, stroke, egui::StrokeKind::Inside);
}

fn draw_row_label(ctx: &mut DrawContext, pos: ScreenPos, width: f32, text: &str) {
    let label = Label::new(text.to_owned());
    // Vertically center the text within the row so descenders aren't clipped.
    let text_h = ctx.row_height(label.font);
    let y = pos.y + ((ROW_H - text_h) * 0.5).max(0.0);
    ctx.draw_node(
        &label,
        DrawConstraints {
            pos: ScreenPos { x: pos.x + PAD, y },
            x: Some(AxisConstraint::AtMost((width - 2.0 * PAD).max(0.0))),
            y: None,
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        },
    );
}

defhandlers! { SelectionBox {
    actions: [
        SetSelected { index: usize } => (this, s) { this.selected = s.index; },
    ],
    requests: [
        Selected => (this, _q): usize { this.selected },
    ],
}}
