//! A sizing draw asks a question; it must not leave anything behind.
//!
//! A layout with a filling child measures its fixed children before it can
//! place anything, and it measures them at the container's own origin, because
//! where they will end up is exactly what it is trying to work out. The draw
//! that answers is invisible — the `Ui` it runs on is — but two things escaped
//! that and left the measuring pass's *positions* on the screen:
//!
//! * the inspect probe, which recorded a second, phantom copy of every measured
//!   node at the top of its container, where the lens then found it in
//!   preference to whatever was really drawn there;
//! * a layer painter, which is taken from the context rather than from the
//!   `Ui`, so a wire drawn through one was painted twice.
//!
//! The wires are pinned in `wire_targets.rs`. This is the probe.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::Lambda;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::layouts::inspector::{LensRegion, LensTarget};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

struct App {
    ws: Workspace,
    ctx: egui::Context,
}

impl App {
    fn new() -> App {
        dex_nodes::scripting::init_python();
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut app = App {
            ws: Desktops::new_workspace(),
            ctx,
        };
        app.hover(None);
        app
    }

    /// One frame, optionally with the pointer somewhere.
    fn hover(&mut self, pointer: Option<egui::Pos2>) {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let ws = &mut self.ws;
        let _ = self.ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        self.ws.process_pending();
    }

    /// Settle the pointer somewhere, past the lens's own hold on its target.
    fn settle_on(&mut self, pointer: egui::Pos2) {
        for _ in 0..20 {
            self.hover(Some(pointer));
        }
    }

    fn lens(&self) -> Option<(String, ScreenRegion)> {
        let root = self.ws.root();
        let target = self.ws.send_request(root, LensTarget).flatten()?;
        let region = self.ws.send_request(root, LensRegion).flatten()?;
        let name = self.ws.get_node(target)?.type_name(NodeContext {
            id: target,
            workspace: &self.ws,
        });
        Some((name, region))
    }
}

/// The lens finds what is drawn under the pointer, not what was measured there.
///
/// A lambda's body fills its card, so every fixed row above it — the name, the
/// arguments, the editor — is measured first, at the card's top-left. The
/// editor is drawn inspectably, so the probe took that measurement for a
/// placement and offered a Code Editor lens sitting over the lambda's title,
/// two hundred points above the editor it named.
#[test]
fn a_measured_node_is_not_offered_to_the_inspector() {
    let mut app = App::new();
    let canvas = app
        .ws
        .send_request(app.ws.root(), ActiveCanvas)
        .expect("the desktop has a canvas");
    app.ws.submit_action(
        canvas,
        "add a lambda",
        AddCanvasItem {
            child: Arc::new(Lambda::new(app.ws.action_handle())),
            size: Vector { x: 420.0, y: 340.0 },
        },
    );
    app.ws.process_pending();
    app.hover(None);

    let item = *app
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the lambda was placed");
    let card = app
        .ws
        .send_request(app.ws.root(), NodeScreenRect { node: item })
        .flatten()
        .expect("the lambda is on screen");

    // Over the title and the argument rows, near the top of the card.
    let near_the_top = egui::pos2(
        card.min.x + card.size().x * 0.5,
        card.min.y + card.size().y * 0.08,
    );
    app.settle_on(near_the_top);
    let (name, _) = app.lens().expect("something offers a lens up there");
    assert_eq!(
        name, "A Lambda",
        "the top of the card belongs to the lambda, not to {name}"
    );

    // The editor still has its own lens, where the editor actually is.
    let over_the_editor = egui::pos2(
        card.min.x + card.size().x * 0.5,
        card.min.y + card.size().y * 0.3,
    );
    app.settle_on(over_the_editor);
    let (name, lens) = app.lens().expect("the editor offers a lens");
    assert_eq!(name, "A Code Editor");
    assert!(
        lens.min.y > card.min.y + 20.0,
        "the editor's lens sits at {}, up at the top of the card ({})",
        lens.min.y,
        card.min.y
    );
}
