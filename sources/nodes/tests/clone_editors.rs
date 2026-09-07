//! Copying a node that is being edited copies what is on screen.
//!
//! A text editor commits on focus loss, and holds the edit in a transient
//! buffer until then — but a transient does not survive being copied, and
//! cloning a canvas takes focus away from nothing. So a lambda copied while its
//! script was being written came back holding the script it had before.

use dex_core::prelude::*;
use dex_nodes::primitives::number::Integer;
use dex_nodes::primitives::text::{CodeEditor, GetCommittedText, GetText, LabelEditable, SetText};

const SCREEN: egui::Vec2 = egui::vec2(800.0, 600.0);

fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        },
    );
}

/// Click into the root node at `at` and type `text` into it, without ever
/// clicking away again — so the edit is still in flight.
fn type_into(ws: &mut Workspace, ctx: &egui::Context, at: egui::Pos2, text: &str) {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(ws, ctx, vec![]);
    frame(ws, ctx, vec![egui::Event::PointerMoved(at)]);
    frame(ws, ctx, vec![button(true)]);
    frame(ws, ctx, vec![button(false)]);
    frame(ws, ctx, vec![egui::Event::Text(text.to_owned())]);
    frame(ws, ctx, vec![]);
}

/// A workspace whose root is `node`, drawn and typed into at `at`. A field
/// that shrinks to its text is only a few points wide, so where the press
/// lands is the difference between focusing it and missing it entirely.
fn edited(node: impl Node, at: egui::Pos2, text: &str) -> (Workspace, egui::Context) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let root = ws.insert_node_now(node).erase();
    ws.set_root(root);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    type_into(&mut ws, &ctx, at, text);
    (ws, ctx)
}

/// Copy the root and settle the queue, giving back the copy's id.
fn copy_of_root(ws: &mut Workspace) -> NodeUid {
    let copy = ws.deep_clone(ws.root());
    ws.process_pending();
    copy
}

/// The reported bug: a script being written is not in the copy.
#[test]
fn a_code_editor_copies_what_is_being_typed() {
    let (mut ws, _ctx) = edited(
        CodeEditor::new(
            "def transform():\n    return".to_owned(),
            "python".to_owned(),
        ),
        egui::pos2(SCREEN.x * 0.5, 24.0),
        "X",
    );
    let live = ws
        .send_request(ws.root(), GetText)
        .expect("the editor reads");
    assert!(
        live.contains('X'),
        "the typing reached the editor at all: {live:?}"
    );
    assert_ne!(
        ws.send_request(ws.root(), GetCommittedText),
        Some(live.clone()),
        "and has not been committed yet, which is the case worth testing"
    );

    let copy = copy_of_root(&mut ws);
    assert_eq!(
        ws.send_request(copy, GetCommittedText).as_deref(),
        Some(live.as_str()),
        "the copy holds what was on screen, not what was last committed"
    );
}

/// The same for a plain editable label, which edits the same way.
#[test]
fn an_editable_label_copies_what_is_being_typed() {
    let (mut ws, _ctx) = edited(
        LabelEditable::new("before".to_owned()),
        egui::pos2(10.0, 10.0),
        "Z",
    );
    let live = ws
        .send_request(ws.root(), GetText)
        .expect("the label reads");
    assert!(live.contains('Z'), "the typing landed: {live:?}");

    let copy = copy_of_root(&mut ws);
    assert_eq!(
        ws.send_request(copy, GetText).as_deref(),
        Some(live.as_str()),
        "the copy reads as the original did"
    );
}

/**
    A number settles through its field, and still refuses text that is not one.

    Where the caret lands on a click is the field's business, so this asks only
    that both digits are there — not which order a click at one end put them in.
*/
#[test]
fn a_number_copies_what_is_being_typed_if_it_parses() {
    let (mut ws, _ctx) = edited(Integer::new(4), egui::pos2(10.0, 10.0), "2");
    let copy = copy_of_root(&mut ws);
    let copied = ws
        .send_request(copy, GetText)
        .expect("the copy reads as a number");
    assert_eq!(copied.len(), 2, "both digits came along: {copied:?}");
    assert!(copied.contains('4') && copied.contains('2'), "{copied:?}");
    assert!(copied.parse::<i64>().is_ok(), "and it is still a number");

    // And something that is not a number is dropped, exactly as typing it is.
    let (mut ws, _ctx) = edited(Integer::new(7), egui::pos2(10.0, 10.0), "oops");
    let copy = copy_of_root(&mut ws);
    assert_eq!(
        ws.send_request(copy, GetText).as_deref(),
        Some("7"),
        "a copy never holds a number that is not one"
    );
}

/// Committing normally still works, and settling does not disturb it.
#[test]
fn a_committed_value_copies_as_it_always_did() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let root = ws
        .insert_node_now(LabelEditable::new("committed".to_owned()))
        .erase();
    ws.set_root(root);
    ws.submit_action_dyn(Action {
        dest: root,
        description: "set".into(),
        body: Box::new(SetText {
            value: "changed".to_owned(),
        }),
    });
    ws.process_pending();

    let copy = copy_of_root(&mut ws);
    assert_eq!(
        ws.send_request(copy, GetText).as_deref(),
        Some("changed"),
        "nothing about the ordinary path moved"
    );
}
