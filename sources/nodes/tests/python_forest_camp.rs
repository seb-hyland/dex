//! Exercises `examples/forest_camp.py`: a canvas built out of its two *fixed*
//! bands — a forest in the background, a fire in the foreground — with the
//! plane running between them.
//!
//! What the example claims is that neither band moves when the plane does, and
//! that the fire moves anyway because it reads the clock. Both are checked by
//! painting, because a Python exception mid-draw is painted as an error rather
//! than raised: a forest that quietly stopped would look much like a canvas
//! nobody put anything on.

use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{
    Canvas, CanvasChildren, CanvasLayerNodes, CanvasViewOrigin, Layer, NodeScreenRect,
};
use dex_nodes::scripting::{ScriptOutput, run_script};

const CAMP: &str = include_str!("../../../examples/forest_camp.py");
const SCREEN: egui::Vec2 = egui::vec2(720.0, 520.0);
/// The surface is drawn *inset* in the window, as the app's own sidebar insets
/// it. A band that misplaces itself by its own origin is exactly right at zero.
const INSET: egui::Vec2 = egui::vec2(120.0, 60.0);

/// Where a surface of `size` is drawn, and the window it is drawn in.
fn pane(size: egui::Vec2) -> (egui::Rect, egui::Rect) {
    (
        egui::Rect::from_min_size(egui::pos2(INSET.x, INSET.y), size),
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size + INSET * 2.0),
    )
}

/// The example's surface, drawn into a window of its own.
struct Harness {
    ws: Workspace,
    ctx: egui::Context,
    canvas: NodeUid<Canvas>,
    pos: egui::Pos2,
    /// The size of the pane the surface is drawn into. The backdrop caches its
    /// geometry against this, so a test can change it to make it rebuild.
    size: egui::Vec2,
}

impl Harness {
    /// Run the example as a lambda would, and show what it built.
    fn new() -> Harness {
        dex_nodes::scripting::init_python();
        let mut ws = Workspace::new_empty();
        let graph = GraphSnapshot::capture(&ws);
        let (handle, actions) = WorkspaceActionHandle::buffered();
        let canvas = match run_script(CAMP, "", &handle, &[], graph) {
            Ok(ScriptOutput::Handle(uid)) => uid,
            Ok(_) => panic!("the example returns the surface it built"),
            Err(e) => panic!("{e}"),
        };
        drop(handle);
        for action in actions.try_iter() {
            ws.submit_action_dyn(action);
        }
        ws.process_pending();
        ws.set_root(canvas);

        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        Harness {
            ws,
            ctx,
            canvas: canvas.cast::<Canvas>(),
            pos: egui::pos2(-100.0, -100.0),
            size: SCREEN,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
        let (pane, window) = pane(self.size);
        let input = egui::RawInput {
            screen_rect: Some(window),
            events,
            ..Default::default()
        };
        let ws = &mut self.ws;
        self.ctx
            .clone()
            .run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ws.draw_frame(ui, pane);
                });
            })
            .shapes
    }

    /// Settle the view: egui's first pass over a layout it has not seen is a
    /// sizing pass, and it fades a new one in over the few after that.
    fn settled(&mut self) -> Vec<egui::epaint::ClippedShape> {
        let mut shapes = Vec::new();
        for _ in 0..12 {
            shapes = self.frame(vec![]);
        }
        shapes
    }

    fn item(&self) -> NodeUid {
        *self
            .ws
            .send_request(self.canvas, CanvasChildren)
            .unwrap_or_default()
            .first()
            .expect("the example put something on the plane")
    }

    /// Where the first item sits on screen, as the canvas maps it.
    fn item_on_screen(&self) -> ScreenPos {
        self.ws
            .send_request(self.canvas, NodeScreenRect { node: self.item() })
            .flatten()
            .expect("the item is on screen")
            .min
    }

    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        self.move_to(from);
        self.move_to(from);
        self.button(true);
        for step in 1..=3 {
            let t = step as f32 / 3.0;
            self.move_to(egui::pos2(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
        self.button(false);
        self.frame(vec![]);
    }

    fn move_to(&mut self, p: egui::Pos2) {
        self.pos = p;
        self.frame(vec![egui::Event::PointerMoved(p)]);
    }

    fn button(&mut self, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton {
            pos: self.pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }]);
    }
}

/// Walk the frame's meshes.
fn meshes(shape: &egui::Shape, found: &mut impl FnMut(&egui::Mesh)) {
    match shape {
        egui::Shape::Vec(inner) => inner.iter().for_each(|s| meshes(s, found)),
        egui::Shape::Mesh(mesh) => found(mesh),
        _ => {}
    }
}

/// Every triangle a frame painted, which is what says a band drew at all.
fn triangles(shapes: &[egui::epaint::ClippedShape]) -> usize {
    let mut found = 0;
    for clipped in shapes {
        meshes(&clipped.shape, &mut |mesh| found += mesh.indices.len() / 3);
    }
    found
}

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

/// The bounds of the widest plain quad in the frame: the backdrop's own sky.
///
/// Not simply the widest mesh — the moon's outermost ring is wider than a small
/// window. The sky is the one full-viewport shape drawn from four corners, and
/// a radial glow is drawn from dozens, so counting vertices tells them apart.
fn sky_bounds(shapes: &[egui::epaint::ClippedShape]) -> egui::Rect {
    let mut widest = egui::Rect::NOTHING;
    for clipped in shapes {
        meshes(&clipped.shape, &mut |mesh| {
            let bounds = mesh.calc_bounds();
            if mesh.vertices.len() <= 8 && bounds.area() > widest.area() {
                widest = bounds;
            }
        });
    }
    widest
}

/// The scene is two fixed bands with a plane between them.
#[test]
fn the_example_builds_a_surface_with_a_band_at_each_end() {
    let h = Harness::new();
    assert!(
        h.ws.get_node(h.canvas.erase())
            .is_some_and(|node| (*node).as_any_ref().is::<Canvas>()),
        "a canvas, not a picture of one"
    );

    let layer = |layer| {
        h.ws.send_request(h.canvas, CanvasLayerNodes { layer })
            .unwrap_or_default()
    };
    assert_eq!(
        layer(Layer::Background).len(),
        1,
        "the forest is the backdrop"
    );
    assert_eq!(layer(Layer::Foreground).len(), 1, "the fire is in front");
    assert_eq!(
        h.ws.send_request(h.canvas, CanvasChildren)
            .unwrap_or_default()
            .len(),
        2,
        "and two things stand on the plane between them"
    );
}

/// Both bands paint, which is the only way to know either of them ran.
#[test]
fn the_forest_and_the_fire_both_paint() {
    let mut h = Harness::new();
    let painted = triangles(&h.settled());
    // The forest alone is a sky, cloud, a moon and its rings, ninety-odd
    // stars, a hundred and forty trees and three swarms of fireflies; the fire
    // is another eighty shapes over it. A few hundred triangles would mean one
    // of the two never ran.
    assert!(
        painted > 2000,
        "the wood and the fire both painted: {painted} triangles"
    );
}

/// The backdrop fills the viewport it was handed, corner to corner.
///
/// A path is painted from the position its constraints carry, so a band that
/// adds its own origin to its points as well lands at twice the origin — and
/// paints a picture of a viewport in the corner of one.
#[test]
fn the_forest_covers_the_whole_viewport() {
    let mut h = Harness::new();
    let sky = sky_bounds(&h.settled());
    let (screen, _window) = pane(h.size);
    assert!(
        (sky.min.x - screen.min.x).abs() < 1.0
            && (sky.min.y - screen.min.y).abs() < 1.0
            && (sky.max.x - screen.max.x).abs() < 1.0
            && (sky.max.y - screen.max.y).abs() < 1.0,
        "the sky spans {sky:?}, and the window is {screen:?}"
    );
}

/// The scene moves without anything being told to animate it.
///
/// Both bands read the clock — the fire in every frame, the backdrop for its
/// stars and its fireflies — and nothing subscribes to anything to do it.
#[test]
fn the_scene_moves_by_itself() {
    let mut h = Harness::new();
    let first = vertices(&h.settled());
    std::thread::sleep(Duration::from_millis(120));
    let second = vertices(&h.frame(vec![]));

    assert!(!first.is_empty(), "the camp painted at all");
    assert_ne!(
        first, second,
        "a drawing that reads the clock is somewhere else a moment later"
    );
}

/// And the point of the arrangement: a pan moves the plane, not the scene.
#[test]
fn panning_the_plane_leaves_the_camp_where_it_is() {
    let mut h = Harness::new();
    let before_sky = sky_bounds(&h.settled());
    let before_item = h.item_on_screen();
    let before_view = {
        let o = h.ws.send_request(h.canvas, CanvasViewOrigin).unwrap();
        (o.x, o.y)
    };

    // Drag the night itself, well clear of anything standing on the plane.
    h.drag(egui::pos2(760.0, 520.0), egui::pos2(640.0, 450.0));

    let after_view = {
        let o = h.ws.send_request(h.canvas, CanvasViewOrigin).unwrap();
        (o.x, o.y)
    };
    assert_ne!(before_view, after_view, "the drag panned the surface");
    let after_item = h.item_on_screen();
    assert!(
        (after_item.x - before_item.x).abs() > 1.0 || (after_item.y - before_item.y).abs() > 1.0,
        "what stands on the plane travelled with it: ({}, {}) to ({}, {})",
        before_item.x,
        before_item.y,
        after_item.x,
        after_item.y
    );

    let after_sky = sky_bounds(&h.settled());
    assert_eq!(
        (
            before_sky.min.x,
            before_sky.min.y,
            before_sky.max.x,
            before_sky.max.y
        ),
        (
            after_sky.min.x,
            after_sky.min.y,
            after_sky.max.x,
            after_sky.max.y
        ),
        "the forest is a band, so the pan went straight past it"
    );
}

/// A resized window gets a backdrop built for it, not the one before it.
///
/// The still half of the forest is cached against the viewport it was built
/// for. A cache that never notices the size changed is the failure this is here
/// for: it paints the old wood, at the old scale, in the corner of the new one.
#[test]
fn the_backdrop_is_rebuilt_when_the_window_changes() {
    let mut h = Harness::new();
    let first = sky_bounds(&h.settled());
    let (screen, _window) = pane(h.size);
    assert!((first.max.x - screen.max.x).abs() < 1.0);

    h.size = egui::vec2(SCREEN.x * 0.6, SCREEN.y * 1.4);
    let second = sky_bounds(&h.settled());
    let (screen, _window) = pane(h.size);
    assert!(
        (second.min.x - screen.min.x).abs() < 1.0
            && (second.min.y - screen.min.y).abs() < 1.0
            && (second.max.x - screen.max.x).abs() < 1.0
            && (second.max.y - screen.max.y).abs() < 1.0,
        "the wood was rebuilt for the new window: {second:?} against {screen:?}"
    );
}
