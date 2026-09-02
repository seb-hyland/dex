//! Editing text that does not fit.
//!
//! A label is laid out centred, which is right for one that fits and has a cost
//! for one that does not: egui follows the caret in a clipped single-line
//! editor only when the galley sits flush with the left of the field, so a
//! centred one never scrolls — and typing past the right-hand edge means typing
//! into text you cannot see. A field narrower than its contents is the ordinary
//! case in a sidebar, so this is not an edge.

use dex_core::prelude::*;
use dex_nodes::primitives::text::LabelEditable;

const SCREEN: egui::Vec2 = egui::vec2(600.0, 200.0);
/// Narrower than the text below, so the field always has to clip.
const FIELD_W: f32 = 130.0;
const FIELD_ORIGIN: ScreenPos = ScreenPos { x: 20.0, y: 30.0 };
const LONG: &str = "/Users/seb-hyland/Documents/dex/examples/scatterplot.py";

struct Field {
    ws: Workspace,
    ctx: egui::Context,
    uid: NodeUid<LabelEditable>,
}

impl Field {
    fn new(text: &str) -> Field {
        dex_nodes::scripting::init_python();
        let mut ws = Workspace::new_empty();
        let mut label = LabelEditable::new(text.to_owned());
        // A field, not a word: it fills the width it is given.
        label.shrink_to_text = false;
        let uid = ws.insert_node_now(label);
        ws.process_pending();

        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut field = Field { ws, ctx, uid };
        field.frame();
        field
    }

    /// Draw one frame, and return the text egui painted, if any.
    fn frame(&mut self) -> Option<egui::Rect> {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let (ws, uid) = (&self.ws, self.uid);
        let output = self.ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let constraints = DrawConstraints {
                    pos: FIELD_ORIGIN,
                    x: Some(AxisConstraint::Exactly(FIELD_W)),
                    y: Some(AxisConstraint::Exactly(24.0)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                };
                let mut draw = DrawContext::for_ui(
                    NodeContext {
                        id: uid.erase(),
                        workspace: ws,
                    },
                    constraints,
                    &mut ui,
                );
                draw.draw_workspace_node(uid.erase(), constraints);
            });
        });
        output.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(text) => Some(text.galley.rect.translate(text.pos.to_vec2())),
            _ => None,
        })
    }

    /// Focus the editor and put the caret `chars` in, the way clicking there
    /// and typing would.
    fn caret_at(&mut self, chars: usize) {
        let id = egui::Id::new(self.uid.erase());
        self.ctx.memory_mut(|mem| mem.request_focus(id));
        let mut state = egui::text_edit::TextEditState::load(&self.ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(chars),
            )));
        state.store(&self.ctx, id);
        // Twice: the caret is read on one frame and scrolled to on the next.
        self.frame();
        self.frame();
    }
}

/// The end of a long value is reachable: putting the caret there scrolls the
/// text under the field so what is being edited is what is on screen.
#[test]
fn editing_the_end_of_a_long_value_scrolls_it_into_view() {
    let mut field = Field::new(LONG);
    let right = FIELD_ORIGIN.x + FIELD_W;

    field.caret_at(0);
    let at_start = field.frame().expect("the field painted its text");
    assert!(
        at_start.min.x >= FIELD_ORIGIN.x - 0.5,
        "with the caret at the start, the text begins at the field's left edge: {}",
        at_start.min.x
    );
    assert!(
        at_start.max.x > right,
        "and runs off the right, which is the whole problem: {} past {right}",
        at_start.max.x
    );

    field.caret_at(LONG.chars().count());
    let at_end = field.frame().expect("the field painted its text");
    assert!(
        at_end.min.x < at_start.min.x - 1.0,
        "with the caret at the end, the text has scrolled left: {} was {}",
        at_end.min.x,
        at_start.min.x
    );
    assert!(
        (at_end.max.x - right).abs() < 2.0,
        "far enough that the end of it is at the field's right edge: {} vs {right}",
        at_end.max.x
    );
}

/// Text that fits is still centred: the scrolling is for the case that needs
/// it, and does not quietly restyle every label that does not.
#[test]
fn a_value_that_fits_is_still_centred() {
    let mut field = Field::new("short");
    field.caret_at(0);
    let painted = field.frame().expect("the field painted its text");

    let slack = FIELD_W - painted.width();
    assert!(slack > 1.0, "the value fits with room to spare: {slack}");
    let left_gap = painted.min.x - FIELD_ORIGIN.x;
    assert!(
        (left_gap - slack / 2.0).abs() < 2.0,
        "and sits centred in the field: {left_gap} of {slack} spare on the left"
    );
}
