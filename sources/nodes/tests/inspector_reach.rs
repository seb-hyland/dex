//! The lens has to stay still long enough to be clicked.
//!
//! The inspector's lens is offered by whatever the pointer is over, and it is
//! drawn beside that thing rather than under the pointer — so reaching for it
//! means leaving the thing that offered it. On a square node the two are close
//! enough together that the walk never crosses anything else. On a long
//! diagonal one it does: the bounding box is mostly the space around the shape,
//! the lens sits well off it, and the neighbours the pointer crosses on the way
//! each offer a lens of their own somewhere else. The lens skipped out from
//! under the cursor and could not be clicked at all.
//!
//! A radial spray of thin diagonal nodes is the worst case, and so the case
//! worth pinning: a couple of hundred branches, every one of their bounding
//! boxes overlapping most of the others.
//!
//! The scene is built here rather than borrowed from an example. It used to
//! come from `circos3.py`, which drew a node per branch; that example now draws
//! its marks directly — which is the right way to draw a big tree and the wrong
//! way to make this test's worst case. A test that needs a pathological scene
//! should say so and build one.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::layouts::inspector::{LensRegion, LensTarget};
use dex_nodes::scripting::{ScriptOutput, run_script};
use std::sync::Arc;

const SCREEN: egui::Vec2 = egui::vec2(1000.0, 1000.0);

/// A radial spray of long thin nodes, each one an inspectable child whose
/// bounding box covers most of the others'.
const SPRAY: &str = r#"
import math

BRANCHES = 220


class Branch:
    """One thin diagonal line, drawn corner to corner of whatever box it gets."""

    def __init__(self, index):
        self.index = index

    def type_name(self):
        return "Branch %d" % self.index

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 0.0
        h = base.y.provided_value() if base.y is not None else 0.0
        (x, y) = (base.pos.x, base.pos.y)
        ctx.draw_node(
            dex.Path.polyline(
                [dex.Vector.new(x, y), dex.Vector.new(x + w, y + h)],
                dex.Stroke.new(1.4, dex.Color.rgb(40, 120, 90)),
            ),
            dex.DrawConstraints(pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None,
                                wrap=None, should_clip=False),
        )
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


class Spray:
    """Branches radiating from the middle, drawn as inspectable children."""

    def __init__(self, branches):
        self.branches = branches

    def type_name(self):
        return "A Spray"

    def owned_nodes(self):
        return list(self.branches)

    def on_delete(self, ctx):
        for uid in self.branches:
            ctx.workspace.delete_node(uid)

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 0.0
        h = base.y.provided_value() if base.y is not None else 0.0
        (cx, cy) = (base.pos.x + w / 2.0, base.pos.y + h / 2.0)
        reach = min(w, h) / 2.0 - 20.0
        for (i, uid) in enumerate(self.branches):
            angle = 2.0 * math.pi * i / len(self.branches)
            (ex, ey) = (cx + reach * math.cos(angle), cy + reach * math.sin(angle))
            (x0, y0) = (min(cx, ex), min(cy, ey))
            ctx.draw_inspectable_node(
                uid,
                dex.DrawConstraints(
                    pos=dex.ScreenPos.new(x0, y0),
                    x=dex.AxisConstraint.Exactly(abs(ex - cx) or 1.0),
                    y=dex.AxisConstraint.Exactly(abs(ey - cy) or 1.0),
                    wrap=None, should_clip=False,
                ),
            )
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


def transform():
    return Spray([dex.ws.insert_node_dyn(Branch(i)) for i in range(BRANCHES)])
"#;

/// A frame's worth of time, so the lens's dwell is measured in frames rather
/// than in however fast the test happens to run.
const FRAME: f64 = 1.0 / 60.0;

/// Drives frames, keeping the clock, because the lens waits on real time.
struct Driver {
    ctx: egui::Context,
    time: f64,
}

impl Driver {
    fn new() -> Self {
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        Self { ctx, time: 0.0 }
    }

    /// One frame, with the pointer at `pointer`.
    fn frame(&mut self, ws: &mut Workspace, pointer: egui::Pos2) {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        self.time += FRAME;
        let input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(self.time),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let _ = self.ctx.run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        });
        ws.process_pending();
    }

    /// Hold the pointer still at `pointer` for `frames` frames, the way a hand
    /// does when it stops to look at something.
    fn dwell(&mut self, ws: &mut Workspace, pointer: egui::Pos2, frames: usize) {
        for _ in 0..frames {
            self.frame(ws, pointer);
        }
    }
}

/// A desktop workspace with the phylogeny on its canvas, drawn and settled.
///
/// It goes on a canvas rather than straight in as the root, because the lens
/// lives in the desktop chrome — and because that is where a plot is when
/// somebody is trying to click one of its branches.
fn plotted() -> (Workspace, Driver) {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let mut driver = Driver::new();
    // A frame first: an item is centred on the canvas viewport, which is not
    // known until the canvas has drawn once.
    driver.frame(&mut ws, egui::pos2(20.0, 20.0));

    let plot = phylogeny(&mut ws);
    ws.submit_action_dyn(Action {
        dest: ws.root(),
        description: "add plot".into(),
        body: Box::new(AddCanvasItem {
            child: plot,
            size: Vector { x: 760.0, y: 760.0 },
        }),
    });
    ws.process_pending();
    // The branches are given their geometry a frame after the size is first
    // seen, and the probe reports the frame after that.
    driver.dwell(&mut ws, egui::pos2(20.0, 20.0), 5);
    (ws, driver)
}

/// The pathological scene: many overlapping thin diagonal inspectable nodes.
fn phylogeny(ws: &mut Workspace) -> Arc<dyn Node> {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let built = match run_script(SPRAY, "", &handle, &[], graph) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the scene is returned as a node"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    built
}

fn lens_of(ws: &Workspace) -> Option<(NodeUid, ScreenRegion)> {
    let root = ws.root();
    let target = ws.send_request(root, LensTarget).flatten()?;
    let region = ws.send_request(root, LensRegion).flatten()?;
    Some((target, region))
}

/// Walking the pointer to the lens does not change what the lens is offering.
#[test]
fn the_lens_holds_still_while_the_pointer_travels_to_it() {
    let (mut ws, mut driver) = plotted();

    // Stop on a branch, the way you would before reaching for its lens.
    let start = centre_of_plot(&ws);
    driver.dwell(&mut ws, start, 20);

    let (target, lens) = lens_of(&ws).expect("a branch offers a lens");
    let destination = egui::pos2(
        (lens.min.x + lens.max.x) * 0.5,
        (lens.min.y + lens.max.y) * 0.5,
    );
    let travel = (destination - start).length();
    assert!(
        travel > 40.0,
        "the lens is {travel} away — too near to be a test of reaching it"
    );

    // Walk there the way a hand does: many small steps, a frame each.
    let steps = 24;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        driver.frame(&mut ws, start + (destination - start) * t);
        let (held, _) = lens_of(&ws).expect("the lens is still up");
        assert_eq!(
            held, target,
            "the lens changed target {i} steps of {steps} into the walk towards it"
        );
    }

    // And it is still where it was, so the click lands on it.
    let (_, arrived) = lens_of(&ws).expect("the lens is still up");
    assert!(
        arrived.contains(destination.into()),
        "the lens moved out from under the pointer"
    );
}

/// Where the plot drew, so the walk starts somewhere among the branches.
fn centre_of_plot(ws: &Workspace) -> egui::Pos2 {
    let root = ws.root();
    let canvas = ws
        .send_request(root, ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the plot was added");
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the plot is on screen");
    // Off-centre: the middle of the plot is the hub every branch leaves from,
    // and a walk from there is a walk out along one of them.
    egui::pos2(
        rect.min.x + rect.size().x * 0.62,
        rect.min.y + rect.size().y * 0.38,
    )
}

/// Holding is not the same as sticking: the lens follows the pointer whenever
/// the pointer is not on its way to it, or there would be no way to inspect
/// anything but the first thing hovered.
#[test]
fn the_lens_follows_the_pointer_everywhere_else() {
    let (mut ws, mut driver) = plotted();
    let origin = centre_of_plot(&ws);

    // Stop at points across the plot, the way you do when looking for one
    // branch in particular. Each stop is well past the dwell.
    let mut targets = std::collections::HashSet::new();
    for i in 0..8 {
        let step = i as f32 * 26.0;
        driver.dwell(&mut ws, origin + egui::vec2(-step, step * 0.8), 20);
        if let Some((target, _)) = lens_of(&ws) {
            targets.insert(target);
        }
    }
    assert!(
        targets.len() > 3,
        "the lens named {} node(s) across eight stops — it is stuck",
        targets.len()
    );
}

/// A canvas item, which is where the lens has always worked, still works.
#[test]
fn a_square_node_still_offers_its_lens_to_the_pointer() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let mut driver = Driver::new();
    driver.frame(&mut ws, egui::pos2(600.0, 450.0));

    ws.submit_action_dyn(Action {
        dest: ws.root(),
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(dex_nodes::primitives::text::Label::new(
                "Hello, world!".to_owned(),
            )),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    driver.frame(&mut ws, egui::pos2(600.0, 450.0));

    let root = ws.root();
    let canvas = ws.send_request(root, ActiveCanvas).expect("a canvas");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item is on screen");
    let start = egui::pos2(
        (rect.min.x + rect.max.x) * 0.5,
        (rect.min.y + rect.max.y) * 0.5,
    );
    driver.dwell(&mut ws, start, 20);

    let (target, lens) = lens_of(&ws).expect("the item offers a lens");
    assert!(ws.get_node(target).is_some());
    // And walking to it keeps it, exactly as for a branch.
    let destination = egui::pos2(
        (lens.min.x + lens.max.x) * 0.5,
        (lens.min.y + lens.max.y) * 0.5,
    );
    for i in 1..=12 {
        let t = i as f32 / 12.0;
        driver.frame(&mut ws, start + (destination - start) * t);
    }
    let (held, _) = lens_of(&ws).expect("the lens is still up");
    assert_eq!(held, target, "the lens held for a square node too");
}
