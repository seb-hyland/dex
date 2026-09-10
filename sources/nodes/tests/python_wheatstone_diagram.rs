//! Exercises `examples/wheatstone_diagram.py`: the bridge as a symbol.
//!
//! It is a diagram of a *kind* of circuit, so the claims are all about what it
//! refuses to say:
//!
//!   * It draws at all. A Python exception mid-draw is *painted* as an error
//!     rather than raised, so a schematic that quietly stopped would look much
//!     like a canvas nobody had put anything on.
//!   * **It writes no values.** Every word it paints is a designator — four arm
//!     names and the letter on the detector — and there is nothing else. A
//!     rating or a potential on this drawing would be a claim about one
//!     particular bridge, and the whole point is that it is about all of them.
//!   * **It paints no background.** A drawing that brings its own paper can
//!     only ever be a rectangle on a canvas; one that does not is part of it.
//!   * It takes no arguments, so wiring something to it changes nothing.

use dex_core::prelude::*;
use dex_core::snapshot::GraphSnapshot;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const DIAGRAM: &str = include_str!("../../../examples/wheatstone_diagram.py");
const SCREEN: egui::Vec2 = egui::vec2(700.0, 520.0);

/// The drawing's own size, as the example declares it.
const DRAWN: egui::Vec2 = egui::vec2(476.0, 384.0);

/// Everything the symbol is allowed to say.
const DESIGNATORS: [&str; 5] = ["R1", "R2", "R3", "R4", "G"];

/// A workspace holding the symbol, built with `args` wired to it.
fn symbol(args: &[(String, ScriptValue)]) -> Workspace {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let graph = GraphSnapshot::capture(&ws);
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let node = match run_script(DIAGRAM, "", &handle, args, graph) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the symbol is returned as a node"),
        Err(e) => panic!("the example did not run: {e}"),
    };
    let root = ws.action_handle().insert_node_dyn(node);
    ws.process_pending();
    ws.set_root(root);
    ws
}

/// Every shape the symbol paints.
///
/// Twice, and the second is the answer: egui's first pass over a layout it has
/// not seen is a sizing pass, and reading that one back says nothing was drawn.
fn painted(ws: &mut Workspace) -> Vec<egui::Shape> {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut flat = Vec::new();
    for _ in 0..2 {
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, found: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, found)),
                other => found.push(other.clone()),
            }
        }
        flat.clear();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut flat));
    }
    flat
}

/// Every word painted, in the order it was painted.
fn words(shapes: &[egui::Shape]) -> Vec<String> {
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// It draws at all — a Python fault mid-draw is painted, not raised.
#[test]
fn the_symbol_draws() {
    let mut ws = symbol(&[]);
    let said = words(&painted(&mut ws));
    assert!(
        !said.iter().any(|w| w.contains("draw error") || w.contains("Traceback")),
        "the symbol should paint a circuit, not a complaint: {said:?}"
    );
    // Something was drawn, and most of it is wire.
    let strokes = painted(&mut ws)
        .iter()
        .filter(|s| matches!(s, egui::Shape::Path(_)))
        .count();
    assert!(strokes > 10, "a bridge is more than {strokes} conductors");
}

/**
    Every word on it is a designator, and there are no others.

    This is the whole of "it shows what the circuit *is*": a rating, a potential
    or a current would each be a statement about one particular bridge.
*/
#[test]
fn it_names_the_parts_and_says_nothing_else() {
    let mut ws = symbol(&[]);
    let said = words(&painted(&mut ws));

    for name in DESIGNATORS {
        assert!(
            said.iter().any(|w| w == name),
            "the symbol should name {name}: {said:?}"
        );
    }
    let extra: Vec<&String> = said
        .iter()
        .filter(|w| !DESIGNATORS.contains(&w.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "and say nothing else at all, but it said {extra:?}"
    );
}

/**
    It brings no paper.

    Checked by size rather than by counting rectangles: the panel it is drawn
    into paints one of its own, and that is not the symbol's doing.
*/
#[test]
fn it_paints_no_background() {
    let mut ws = symbol(&[]);
    let its_own: Vec<egui::Vec2> = painted(&mut ws)
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) => Some(r.rect.size()),
            _ => None,
        })
        .filter(|size| (size.x - DRAWN.x).abs() < 2.0 && (size.y - DRAWN.y).abs() < 2.0)
        .collect();
    assert!(
        its_own.is_empty(),
        "the symbol should paint nothing the size of itself: {its_own:?}"
    );
}

/// Wiring something to it changes nothing, because it reads nothing.
#[test]
fn it_takes_no_arguments() {
    let mut bare = symbol(&[]);
    let mut wired = symbol(&[
        ("supply".to_owned(), ScriptValue::Float(12.0)),
        ("r4".to_owned(), ScriptValue::Float(47.0)),
    ]);
    assert_eq!(
        words(&painted(&mut bare)),
        words(&painted(&mut wired)),
        "a symbol handed values should draw exactly what it drew without them"
    );
}
