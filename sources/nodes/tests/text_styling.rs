//! Bold and italic select a named font family, and egui panics when asked for
//! a family it was never given. A host that installed the faces must get them;
//! one that did not — a test harness, say — must still draw.

use dex_core::prelude::*;
use dex_nodes::primitives::text::Label;

const SCREEN: egui::Vec2 = egui::vec2(900.0, 600.0);

/// Run `body` inside a frame, so `Context::fonts` has something to answer with.
fn in_a_frame<R>(ctx: &egui::Context, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let mut out = None;
    // `run_ui` wants an `FnMut`, but the frame only ever runs once.
    let mut body = Some(body);
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            if let Some(body) = body.take() {
                out = Some(body(ui));
            }
        });
    });
    out.expect("the frame ran")
}

/// Draw `label` on its own and report the width it took.
fn drawn_width(ctx: &egui::Context, label: Label) -> f32 {
    let mut ws = Workspace::new_empty();
    let uid = ws.insert_node_now(label).erase();
    ws.set_root(uid);
    ws.process_pending();

    in_a_frame(ctx, |ui| {
        let mut ui = ui.new_child(egui::UiBuilder::new());
        let constraints = DrawConstraints {
            pos: ScreenPos { x: 0.0, y: 0.0 },
            x: Some(AxisConstraint::AtMost(SCREEN.x)),
            y: Some(AxisConstraint::AtMost(SCREEN.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: false,
        };
        let mut draw = DrawContext::for_ui(
            NodeContext {
                id: uid,
                workspace: &ws,
            },
            constraints,
            &mut ui,
        );
        draw.draw_workspace_node(uid, constraints)
            .and_then(|r| r.region())
            .map(|r| r.size().x)
            .expect("the label drew")
    })
}

#[test]
fn a_styled_font_picks_the_installed_face() {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let plain = Font::proportional(16.0);
    let bold = Font {
        bold: true,
        ..plain
    };
    let italic = Font {
        italic: true,
        ..plain
    };
    let both = Font {
        bold: true,
        italic: true,
        ..plain
    };
    let mono_bold = Font {
        bold: true,
        ..Font::monospaced(16.0)
    };

    in_a_frame(&ctx, |ui| {
        let c = ui.ctx();
        assert_eq!(plain.font_id_in(c).family, egui::FontFamily::Proportional);
        assert_eq!(
            bold.font_id_in(c).family,
            egui::FontFamily::Name(BOLD_FAMILY.into())
        );
        assert_eq!(
            italic.font_id_in(c).family,
            egui::FontFamily::Name(ITALIC_FAMILY.into())
        );
        assert_eq!(
            both.font_id_in(c).family,
            egui::FontFamily::Name(BOLD_ITALIC_FAMILY.into())
        );
        // No monospace face is styled, so it stays monospace rather than
        // asking for a family that is not there.
        assert_eq!(mono_bold.font_id_in(c).family, egui::FontFamily::Monospace);
    });
}

#[test]
fn bold_text_is_wider_than_plain_text() {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let plain = Label::new("Hamburgefonstiv".to_owned());
    let mut bold = plain.clone();
    bold.font.bold = true;

    let plain_w = drawn_width(&ctx, plain);
    let bold_w = drawn_width(&ctx, bold);
    assert!(
        bold_w > plain_w,
        "the bold face is actually used: {bold_w} vs {plain_w}"
    );
}

#[test]
fn underlining_grows_no_wider_but_still_draws() {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let plain = Label::new("Hamburgefonstiv".to_owned());
    let mut underlined = plain.clone();
    underlined.font.underline = true;

    // A rule under the text is decoration; it must not change the metrics.
    assert_eq!(
        drawn_width(&ctx, underlined),
        drawn_width(&ctx, plain),
        "an underline does not resize the text"
    );
}

/// A context with no installed faces must fall back, not panic.
#[test]
fn a_styled_label_draws_without_the_faces_installed() {
    let ctx = egui::Context::default();

    let mut styled = Label::new("Hamburgefonstiv".to_owned());
    styled.font.bold = true;
    styled.font.italic = true;
    styled.font.underline = true;

    let width = drawn_width(&ctx, styled);
    assert!(width > 0.0, "it drew with the fallback face");
}
