use egui::{Image, ImageSource};

use crate::node::view::HeadlessWindow;
use crate::node::{DrawContext, NodeDynamics};
use crate::prelude::*;

use std::path::Path;

#[derive(Serialize, Deserialize)]
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
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rect(ctx).0
    }

    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let uri = &self.uri;
        self.view.show(ctx, |ui| {
            let image = Image::new(ImageSource::Uri(uri.into()))
                .fit_to_original_size(3.0)
                .max_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add(image);
            })
            .response
            .rect
        });
    }
}
