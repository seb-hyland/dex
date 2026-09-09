//! Exercises the three image-style examples: `tshirt.py`, `dynabook.py` and
//! `dynabook_flight.py`.
//!
//! What makes them a family is the claim in each of their docstrings: they are
//! drawings rather than layouts, so they take the box they are given and map a
//! fixed art space into it — uniform scale, centred, letterboxed. That is the
//! thing worth a test, because it is the thing that silently stops being true.
//! A node that stretched to fill would still paint, and still look right in a
//! box that happened to have the art's aspect ratio.
//!
//! Two of them get one more each. The traced drawing must come out in two
//! colours, and the coloured scene must move. A trace
//! is an ink web with holes in it, and the holes are worked out here rather
//! than by a fill rule — get the nesting wrong and the drawing is a black
//! rectangle that still passes every other check.

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, run_script};

const TSHIRT: &str = include_str!("../../../examples/tshirt.py");
const DYNABOOK: &str = include_str!("../../../examples/dynabook.py");
const FLIGHT: &str = include_str!("../../../examples/dynabook_flight.py");

/// The node an example's `transform()` returns, in a workspace showing it.
fn shown(source: &str) -> (Workspace, egui::Context) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let node = match run_script(source, "", &handle, &[], GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("an image-style example returns a node"),
        Err(e) => panic!("{e}"),
    };
    let root = ws.action_handle().insert_node_dyn(node);
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(root);

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    (ws, ctx)
}

/// Draw into a box of `size` until the drawing has settled.
///
/// Several frames: egui's first pass over a layout it has not seen is a sizing
/// pass, and it fades a new one in over the frames after that.
fn painted(
    ws: &mut Workspace,
    ctx: &egui::Context,
    size: egui::Vec2,
) -> Vec<egui::epaint::ClippedShape> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size);
    let mut shapes = Vec::new();
    for _ in 0..12 {
        shapes = ctx
            .clone()
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        ws.draw_frame(ui, screen);
                    });
                },
            )
            .shapes;
    }
    shapes
}

/// Walk the frame's meshes.
fn meshes(shape: &egui::Shape, found: &mut impl FnMut(&egui::Mesh)) {
    match shape {
        egui::Shape::Vec(inner) => inner.iter().for_each(|s| meshes(s, found)),
        egui::Shape::Mesh(mesh) => found(mesh),
        _ => {}
    }
}

/// Where the drawing landed, and how many triangles it took.
///
/// Two things are deliberately not counted. Fully transparent vertices are not
/// the drawing — a glow that ramps to nothing reaches well past what anyone can
/// see of it. And a vertex is clamped into its own clip rectangle before it
/// counts, because a shape drawn larger than the node's box and clipped to it
/// paints only as far as the box: taking the raw vertex would report a drawing
/// as wider than the box it politely stayed inside.
fn drawn(shapes: &[egui::epaint::ClippedShape]) -> (egui::Rect, usize) {
    let mut bounds = egui::Rect::NOTHING;
    let mut triangles = 0;
    for clipped in shapes {
        let clip = clipped.clip_rect;
        meshes(&clipped.shape, &mut |mesh| {
            triangles += mesh.indices.len() / 3;
            for vertex in &mesh.vertices {
                if vertex.color.a() > 8 {
                    bounds.extend_with(clip.clamp(vertex.pos));
                }
            }
        });
    }
    (bounds, triangles)
}

/// Every example, with the aspect its art space says it has and the least it
/// can paint and still have painted.
const EXAMPLES: [(&str, &str, f32, usize); 3] = [
    ("tshirt.py", TSHIRT, 560.0 / 470.0, 60),
    ("dynabook.py", DYNABOOK, 600.0 / 454.0, 400),
    ("dynabook_flight.py", FLIGHT, 980.0 / 560.0, 600),
];

/// Each of them returns a node that paints.
#[test]
fn the_image_examples_paint() {
    for (name, source, _aspect, floor) in EXAMPLES {
        let (mut ws, ctx) = shown(source);
        let (_bounds, triangles) = drawn(&painted(&mut ws, &ctx, egui::vec2(520.0, 420.0)));
        assert!(
            triangles > floor,
            "{name} painted {triangles} triangles, wanted more than {floor}"
        );
    }
}

/// And each of them keeps its shape in a box that is the wrong shape.
///
/// The box here is far wider than any of the art spaces, so a node that filled
/// it would report a drawing as wide as the window. What should happen is that
/// the drawing is scaled to the height, centred, and the width left over stays
/// empty on both sides.
#[test]
fn the_image_examples_letterbox_rather_than_stretch() {
    const BOX: egui::Vec2 = egui::vec2(900.0, 260.0);
    for (name, source, aspect, _floor) in EXAMPLES {
        let (mut ws, ctx) = shown(source);
        let (bounds, _triangles) = drawn(&painted(&mut ws, &ctx, BOX));

        // Height-limited, so the drawing is at most this wide however wide the
        // box is. A shirt is narrower still — it has no full-bleed background —
        // so this is an upper bound rather than an equality.
        let want = BOX.y * aspect;
        assert!(
            bounds.width() <= want + 1.0,
            "{name} came out {:.1} wide in a {:.0}-wide box; its art fits in {want:.1}",
            bounds.width(),
            BOX.x
        );
        assert!(
            bounds.width() > want * 0.5,
            "{name} came out {:.1} wide, which is not the drawing at all",
            bounds.width()
        );
        // Centred, so what is left over is split between the two sides.
        let (left, right) = (bounds.min.x, BOX.x - bounds.max.x);
        assert!(
            (left - right).abs() < 2.0,
            "{name} sits {left:.1} from the left and {right:.1} from the right"
        );
    }
}

/// The trace comes out as ink on paper, not as a filled silhouette.
///
/// A traced outline is one loop with holes inside it, and which loops are holes
/// is decided here by counting how many other loops each one is inside. Get
/// that wrong and every hole fills with ink: the drawing paints, it letterboxes,
/// and it is a black rectangle. So both colours have to actually arrive.
#[test]
fn the_traced_drawing_is_ink_and_paper() {
    let (mut ws, ctx) = shown(DYNABOOK);
    let shapes = painted(&mut ws, &ctx, egui::vec2(600.0, 454.0));

    let (mut ink, mut paper) = (0usize, 0usize);
    for clipped in &shapes {
        meshes(&clipped.shape, &mut |mesh| {
            for vertex in &mesh.vertices {
                let c = vertex.color;
                if c.a() > 200 {
                    let lightest = c.r().max(c.g()).max(c.b());
                    if lightest < 70 {
                        ink += 1;
                    } else if lightest > 200 {
                        paper += 1;
                    }
                }
            }
        });
    }
    assert!(ink > 100, "the ink painted: {ink} vertices");
    assert!(
        paper > 100,
        "and so did the holes in it: {paper} vertices — a trace whose holes \
         filled with ink is a black rectangle"
    );
}

/// The coloured scene moves without anything being told to animate it.
///
/// Birds cross the sky and beat as they go, and the two ships on both screens
/// orbit the star they are falling into. Nothing subscribes to a frame counter
/// to do it: `draw` runs again the moment the last one finished, so a drawing
/// that reads the clock is a drawing that moves. This is the image-style half
/// of the claim `campfire.py` makes for a whole surface.
#[test]
fn the_coloured_scene_moves_by_itself() {
    let (mut ws, ctx) = shown(FLIGHT);
    let size = egui::vec2(560.0, 320.0);

    /// Every vertex a frame painted, in paint order: the picture itself.
    fn vertices(shapes: &[egui::epaint::ClippedShape]) -> Vec<(u32, u32)> {
        let mut found = Vec::new();
        for clipped in shapes {
            meshes(&clipped.shape, &mut |mesh| {
                found.extend(
                    mesh.vertices
                        .iter()
                        .map(|v| (v.pos.x.to_bits(), v.pos.y.to_bits())),
                );
            });
        }
        found
    }

    let first = vertices(&painted(&mut ws, &ctx, size));
    std::thread::sleep(std::time::Duration::from_millis(120));
    let second = vertices(&painted(&mut ws, &ctx, size));

    assert!(!first.is_empty(), "the scene painted at all");
    assert_ne!(
        first, second,
        "a drawing that reads the clock is somewhere else a moment later"
    );
}
