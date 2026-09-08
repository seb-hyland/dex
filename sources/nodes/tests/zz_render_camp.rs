//! Temporary: rasterise one frame to a raw RGB dump. See the memory note.
use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, run_script};

const CAMP: &str = include_str!("../../../examples/forest_camp.py");

#[test]
#[ignore]
fn render() {
    let w = std::env::var("W").ok().and_then(|s| s.parse().ok()).unwrap_or(1000.0f32);
    let h = std::env::var("H").ok().and_then(|s| s.parse().ok()).unwrap_or(700.0f32);
    let src = std::env::var("SRC").unwrap_or_default();
    let source = if src.is_empty() { CAMP.to_owned() } else { std::fs::read_to_string(&src).unwrap() };

    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let graph = GraphSnapshot::capture(&ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let canvas = match run_script(&source, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("not a surface"),
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
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h));
    let mut shapes = Vec::new();
    let mut worst = std::time::Duration::ZERO;
    for i in 0..20 {
        let start = std::time::Instant::now();
        shapes = ctx
            .clone()
            .run_ui(
                egui::RawInput { screen_rect: Some(screen), ..Default::default() },
                |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        ws.draw_frame(ui, screen);
                    });
                },
            )
            .shapes;
        if i > 4 {
            worst = worst.max(start.elapsed());
        }
    }
    eprintln!("worst frame: {worst:?}");

    let prims = ctx.tessellate(shapes, 1.0);
    let (iw, ih) = (w as usize, h as usize);
    let mut buf = vec![255u8; iw * ih * 3];
    let mut tris = 0usize;

    for prim in &prims {
        let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else { continue };
        let clip = prim.clip_rect;
        for tri in mesh.indices.chunks_exact(3) {
            let v: Vec<&egui::epaint::Vertex> = tri.iter().map(|i| &mesh.vertices[*i as usize]).collect();
            if v.iter().any(|x| (x.uv.x - egui::epaint::WHITE_UV.x).abs() > 1e-4
                || (x.uv.y - egui::epaint::WHITE_UV.y).abs() > 1e-4)
            {
                continue;
            }
            tris += 1;
            let xs = [v[0].pos.x, v[1].pos.x, v[2].pos.x];
            let ys = [v[0].pos.y, v[1].pos.y, v[2].pos.y];
            let x0 = xs.iter().cloned().fold(f32::MAX, f32::min).max(clip.min.x).max(0.0).floor() as i64;
            let x1 = xs.iter().cloned().fold(f32::MIN, f32::max).min(clip.max.x).min(w).ceil() as i64;
            let y0 = ys.iter().cloned().fold(f32::MAX, f32::min).max(clip.min.y).max(0.0).floor() as i64;
            let y1 = ys.iter().cloned().fold(f32::MIN, f32::max).min(clip.max.y).min(h).ceil() as i64;
            let area = (xs[1] - xs[0]) * (ys[2] - ys[0]) - (xs[2] - xs[0]) * (ys[1] - ys[0]);
            if area.abs() < 1e-9 { continue; }
            for py in y0..y1 {
                for px in x0..x1 {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((xs[1] - fx) * (ys[2] - fy) - (xs[2] - fx) * (ys[1] - fy)) / area;
                    let w1 = ((xs[2] - fx) * (ys[0] - fy) - (xs[0] - fx) * (ys[2] - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }
                    let c = |f: fn(&egui::Color32) -> u8| {
                        w0 * f(&v[0].color) as f32 + w1 * f(&v[1].color) as f32 + w2 * f(&v[2].color) as f32
                    };
                    let (r, g, b, a) = (c(|x| x.r()), c(|x| x.g()), c(|x| x.b()), c(|x| x.a()) / 255.0);
                    let i = (py as usize * iw + px as usize) * 3;
                    for (k, s) in [r, g, b].iter().enumerate() {
                        let d = buf[i + k] as f32;
                        buf[i + k] = (s + d * (1.0 - a)).clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    eprintln!("triangles: {tris}");
    let out = std::env::var("OUT").unwrap();
    std::fs::write(format!("{out}.raw"), &buf).unwrap();
    std::fs::write(format!("{out}.dim"), format!("{iw} {ih}")).unwrap();
}
