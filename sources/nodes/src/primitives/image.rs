use std::hash::{Hash, Hasher};

use dex_core::prelude::*;
use egui::{Align, Layout, UiBuilder};

/// Install the image loaders that the [`Image`] node relies on.
pub fn install_image_support(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Displays image data, scaled to fit fully within its constraints.
#[utils::dynamic_type]
#[utils::portable]
pub struct Image {
    /// Raster bytes, or SVG source as UTF-8 when [`Image::is_svg`].
    pub bytes: Vec<u8>,
    pub is_svg: bool,
}

#[utils::dynamic_methods]
impl Image {
    /// An image from raster bytes (PNG, JPEG, etc.).
    pub fn from_bytes(bytes: Vec<u8>) -> Image {
        Image {
            bytes,
            is_svg: false,
        }
    }

    /// An image from an SVG source string.
    pub fn from_svg(source: String) -> Image {
        Image {
            bytes: source.into_bytes(),
            is_svg: true,
        }
    }
}

impl Image {
    /// A cache key derived from the content, so replacing the bytes loads afresh.
    fn uri(&self) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.bytes.hash(&mut h);
        let ext = if self.is_svg { "svg" } else { "png" };
        format!("bytes://img-{:x}.{ext}", h.finish())
    }
}

#[utils::dynamic_node]
impl Node for Image {
    fn type_name(&self) -> String {
        "Image".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        if self.bytes.is_empty() {
            return DrawResult::Complete { region: None };
        }

        let origin = ctx.constraints.pos;
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());
        // Bound the box; fall back to a sane cap on unconstrained axes.
        let max = egui::vec2(avail_w.unwrap_or(512.0), avail_h.unwrap_or(512.0));
        let rect = egui::Rect::from_min_size(origin.into(), max);

        let image = egui::Image::from_bytes(self.uri(), self.bytes.clone())
            .maintain_aspect_ratio(true)
            .max_size(max);

        let response = ctx
            .ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| ui.add(image),
            )
            .inner;

        DrawResult::Complete {
            region: Some(response.rect.into()),
        }
    }
}

defhandlers! { Image {} }
