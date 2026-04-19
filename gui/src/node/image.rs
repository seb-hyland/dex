use egui::{Image, ImageSource};

use crate::node::view::{HeadlessWindow, ResizeDir};
use crate::node::{DrawContext, LayoutContext, NodeDynamics};
use crate::prelude::*;

use std::f32;
use std::path::Path;

#[derive(Clone)]
pub struct ImagePayload {
    uri: String,
    view: HeadlessWindow,
}

impl ImagePayload {
    pub fn new(path: &Path) -> Self {
        Self {
            uri: format!("file://{}", path.to_string_lossy()),
            view: HeadlessWindow::default(),
        }
    }
}

impl Default for ImagePayload {
    fn default() -> Self {
        let mut path = rfd::FileDialog::new().pick_file();
        while path.is_none() {
            path = rfd::FileDialog::new().pick_file();
        }

        Self::new(&path.unwrap())
    }
}

impl NodeDynamics for ImagePayload {
    fn step(&self, _ctx: &mut DrawContext<'_>) {}

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        let uri = &self.uri;
        self.view.show(ctx, |ui, _actions| {
            let w = ui.available_width();
            // Allow the image to expand vertically as much as it wants
            let image = Image::new(ImageSource::Uri(uri.into()))
                .maintain_aspect_ratio(true)
                .max_width(w)
                .fit_to_exact_size(Vec2::new(f32::INFINITY, w));

            ui.vertical_centered(|ui| {
                ui.add(image);
            })
            .response
            .rect
        });
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.size().0
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }
}
