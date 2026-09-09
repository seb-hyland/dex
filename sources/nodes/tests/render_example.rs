//! Persistent visual-check harness: render an example's `transform()` node to a
//! PNG so it can be looked at. Kept in the tree on purpose — it is how the
//! image-style examples (t-shirt, dynabook, …) are checked instead of guessed
//! at. `#[ignore]`, so it never runs in an ordinary `cargo test`.
//!
//!   DEX_SRC=examples/tshirt.py DEX_OUT=/tmp/out.png DEX_W=512 DEX_H=384 \
//!     cargo test -p dex-nodes --test render_example -- --ignored --nocapture
//!
//! Env: DEX_SRC (source .py), DEX_OUT (.png path), DEX_W/DEX_H (pixels, default
//! 512×384), DEX_DARK (any value forces the dark theme; the default is light,
//! which is what an image-style node is drawn against). Text glyphs are not
//! rasterised — everything a `dex.Path` paints is. The PNG writer below is
//! dependency-free (uncompressed zlib blocks), so the file is large but needs no
//! image crate.

use std::io::Write;

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, run_script};

#[test]
#[ignore]
fn render() {
    let src_path = std::env::var("DEX_SRC").expect("DEX_SRC");
    let out_path = std::env::var("DEX_OUT").expect("DEX_OUT");
    let w: usize = std::env::var("DEX_W").ok().and_then(|s| s.parse().ok()).unwrap_or(512);
    let h: usize = std::env::var("DEX_H").ok().and_then(|s| s.parse().ok()).unwrap_or(384);
    let source = std::fs::read_to_string(&src_path).expect("read source");

    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let node = match run_script(&source, "", &handle, &[], GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the example returns a node"),
        Err(e) => panic!("{e}"),
    };
    let root = ws.action_handle().insert_node_dyn(node);
    drop(handle);
    for a in actions.try_iter() {
        ws.submit_action_dyn(a);
    }
    ws.process_pending();
    ws.set_root(root);

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    // Light by default: these are drawings on paper, and a dark panel behind a
    // white shirt says nothing about whether the shirt painted.
    if std::env::var("DEX_DARK").is_ok() {
        ctx.set_visuals(egui::Visuals::dark());
    } else {
        ctx.set_visuals(egui::Visuals::light());
    }
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w as f32, h as f32));

    // egui's first pass over a layout it has not seen is a sizing pass, and it
    // fades a new one in over the frames after that — so an early frame paints
    // the drawing at a fraction of its opacity. Run enough of them to settle.
    let mut shapes = Vec::new();
    for _ in 0..16 {
        let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
        shapes = ctx
            .clone()
            .run_ui(input, |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            })
            .shapes;
    }

    let prims = ctx.tessellate(shapes, 1.0);

    // White background, RGB. Glyphs sample the font atlas (uv != white); skip
    // them — everything a `dex.Path` paints is a plain mesh and comes through.
    let mut buf = vec![255u8; w * h * 3];
    let wu = egui::epaint::WHITE_UV;
    for cp in &prims {
        let clip = cp.clip_rect;
        let egui::epaint::Primitive::Mesh(mesh) = &cp.primitive else { continue };
        for tri in mesh.indices.chunks_exact(3) {
            let v = [
                &mesh.vertices[tri[0] as usize],
                &mesh.vertices[tri[1] as usize],
                &mesh.vertices[tri[2] as usize],
            ];
            if v.iter().any(|vx| (vx.uv.x - wu.x).abs() > 1e-4 || (vx.uv.y - wu.y).abs() > 1e-4) {
                continue;
            }
            let (p0, p1, p2) = (v[0].pos, v[1].pos, v[2].pos);
            let minx = p0.x.min(p1.x).min(p2.x).max(clip.min.x).floor().max(0.0) as usize;
            let maxx = (p0.x.max(p1.x).max(p2.x).min(clip.max.x).ceil() as i32).clamp(0, w as i32) as usize;
            let miny = p0.y.min(p1.y).min(p2.y).max(clip.min.y).floor().max(0.0) as usize;
            let maxy = (p0.y.max(p1.y).max(p2.y).min(clip.max.y).ceil() as i32).clamp(0, h as i32) as usize;
            let area = edge(p0, p1, p2);
            if area.abs() < 1e-6 {
                continue;
            }
            for py in miny..maxy {
                for px in minx..maxx {
                    let p = egui::pos2(px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = edge(p1, p2, p) / area;
                    let w1 = edge(p2, p0, p) / area;
                    let w2 = edge(p0, p1, p) / area;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let c0 = v[0].color;
                    let c1 = v[1].color;
                    let c2 = v[2].color;
                    let r = w0 * c0.r() as f32 + w1 * c1.r() as f32 + w2 * c2.r() as f32;
                    let g = w0 * c0.g() as f32 + w1 * c1.g() as f32 + w2 * c2.g() as f32;
                    let b = w0 * c0.b() as f32 + w1 * c1.b() as f32 + w2 * c2.b() as f32;
                    let a = w0 * c0.a() as f32 + w1 * c1.a() as f32 + w2 * c2.a() as f32;
                    let inv = 1.0 - a / 255.0;
                    let o = (py * w + px) * 3;
                    buf[o] = (r + buf[o] as f32 * inv).round().clamp(0.0, 255.0) as u8;
                    buf[o + 1] = (g + buf[o + 1] as f32 * inv).round().clamp(0.0, 255.0) as u8;
                    buf[o + 2] = (b + buf[o + 2] as f32 * inv).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    let png = encode_png(w, h, &buf);
    std::fs::File::create(&out_path)
        .and_then(|mut f| f.write_all(&png))
        .expect("write png");
    eprintln!("wrote {out_path} ({w}x{h})");
}

/// Twice the signed area of the triangle (a, b, c) — the edge function.
fn edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

// -- a minimal, dependency-free PNG encoder ------------------------------------

/// Encode 8-bit RGB `rgb` (row-major, `w*h*3` bytes) as a PNG byte stream.
fn encode_png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    // Each scanline is prefixed with filter byte 0 (none).
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits/channel, colour type 2 (RGB)
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib_store(&raw));
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_in = kind.to_vec();
    crc_in.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
}

/// A zlib stream wrapping `data` in uncompressed DEFLATE blocks. No compression,
/// no dependency — the harness values simplicity over file size.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib: CM=8, no dict, default level
    let mut i = 0;
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
    }
    while i < data.len() {
        let n = (data.len() - i).min(0xffff);
        let last = if i + n >= data.len() { 1 } else { 0 };
        out.push(last); // BFINAL bit, BTYPE=00 (stored)
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[i..i + n]);
        i += n;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
