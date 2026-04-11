use crate::node::view::Window;
use crate::node::{DrawContext, NodeDynamics};
use crate::prelude::*;

use wry::WebViewBuilder;

#[derive(Serialize, Deserialize)]
pub struct WebviewPayload {
    #[serde(skip, default = "init_webview_handle")]
    handle: wry::WebView,
    text_url: String,
    view: Window,
}
fn init_webview_handle() -> wry::WebView {
    todo!()
}

impl WebviewPayload {
    pub fn new(frame: &mut eframe::Frame) -> Result<Self, wry::Error> {
        let text_url = "www.google.com".to_string();
        let view = Window::default();

        let handle = WebViewBuilder::new()
            .with_url(text_url.clone())
            .build_as_child(frame)?;

        Ok(Self {
            handle,
            text_url,
            view,
        })
    }
}

impl NodeDynamics for WebviewPayload {
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }

    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        self.view.show(
            ctx,
            |ui| {
                let pos = ui.next_widget_position();
                let size = ui.available_size_before_wrap();

                let _ = self.handle.set_bounds(wry::Rect {
                    position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition {
                        x: pos.x as f64,
                        y: pos.y as f64,
                    }),
                    size: wry::dpi::Size::Logical(wry::dpi::LogicalSize {
                        width: size.x as f64,
                        height: size.y as f64,
                    }),
                });

                Rect::from_min_size(pos, Vec2 { x: 800.0, y: 500.0 })
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.text_url);
                    if ui.button("Go!").clicked() {
                        let _ = self.handle.load_url(&self.text_url);
                    }
                });
            },
        );
    }
}
