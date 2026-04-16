use crate::prelude::*;
use crate::{canvas::CanvasCommand, node::DrawContext};

use std::sync::Arc;

use eframe::egui::text::LayoutJob;
use eframe::egui::{Align, Galley, Layout, Sense, StrokeKind, TextBuffer, TextStyle, UiBuilder};
use egui::{CursorIcon, FontId};

#[derive(Clone, Serialize, Deserialize)]
pub struct Window {
    body_size: Vec2,
    cached_header_content_height: f32,
    collapsed: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            body_size: Vec2 { x: 500.0, y: 300.0 },
            cached_header_content_height: 0.0,
            collapsed: true,
        }
    }
}

impl Window {
    /// Returns (header_size, bounding_size, padding)
    pub fn sizes(&self) -> (Vec2, Vec2, f32) {
        let padding = 10.0;

        let header_height = self.cached_header_content_height;
        let header_size = Vec2 {
            x: self.body_size.x,
            y: header_height + 2.0 * padding,
        };

        let bounding_size = if self.collapsed {
            header_size
        } else {
            Vec2 {
                y: header_size.y + self.body_size.y,
                ..header_size
            }
        };

        (header_size, bounding_size, padding)
    }

    pub fn show(
        &mut self,
        ctx: &mut DrawContext<'_>,
        background: Color32,
        add_header: impl FnOnce(&mut Ui),
        add_main: impl FnOnce(&mut Ui),
    ) {
        let (header_size, bounding_size, padding) = self.sizes();

        let header_rect = Rect::from_min_size(ctx.screen_location, header_size);
        let bounding_rect = Rect::from_min_size(ctx.screen_location, bounding_size);

        ctx.ui.painter().rect(
            bounding_rect,
            ctx.theme.corner_radius,
            background,
            ctx.theme.border,
            StrokeKind::Inside,
        );
        ctx.ui.painter().rect(
            header_rect,
            ctx.theme.corner_radius,
            background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        // Header
        let header_area = ctx
            .ui
            .interact(
                header_rect,
                ctx.id.with("header_bar"),
                Sense::HOVER | Sense::DRAG,
            )
            .on_hover_cursor(CursorIcon::Grab);

        let edge_width = 6.0;

        // Dragging left or right edge always affects this node
        let left_edge = ctx
            .ui
            .interact(
                bounding_rect
                    .with_min_x(bounding_rect.min.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.min.x + edge_width / 2.0),
                ctx.id.with("left_edge"),
                Sense::DRAG | Sense::HOVER,
            )
            .on_hover_cursor(CursorIcon::ResizeHorizontal);
        if left_edge.dragged() {
            let drag_delta = left_edge.drag_delta();
            self.body_size.x -= drag_delta.x;
            ctx.command_queue.push(CanvasCommand::MoveNode {
                idx: ctx.index,
                delta: Vec2::new(drag_delta.x, 0.0),
            })
        }

        let right_edge = ctx
            .ui
            .interact(
                bounding_rect
                    .with_min_x(bounding_rect.max.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.max.x + edge_width / 2.0),
                ctx.id.with("right_edge"),
                Sense::DRAG | Sense::HOVER,
            )
            .on_hover_cursor(CursorIcon::ResizeHorizontal);
        if right_edge.dragged() {
            self.body_size.x += right_edge.drag_delta().x;
        }

        // Dragging bottom works when expanded
        if !self.collapsed {
            let top_edge = ctx
                .ui
                .interact(
                    bounding_rect
                        .with_min_y(bounding_rect.min.y - edge_width / 2.0)
                        .with_max_y(bounding_rect.min.y + edge_width / 2.0),
                    ctx.id.with("top_edge"),
                    Sense::DRAG,
                )
                .on_hover_cursor(CursorIcon::ResizeVertical);
            if top_edge.dragged() {
                let drag_delta = top_edge.drag_delta();
                self.body_size.y -= drag_delta.y;
                ctx.command_queue.push(CanvasCommand::MoveNode {
                    idx: ctx.index,
                    delta: Vec2::new(0.0, drag_delta.y),
                });
            }

            let bottom_edge = ctx
                .ui
                .interact(
                    bounding_rect
                        .with_min_y(bounding_rect.max.y - edge_width / 2.0)
                        .with_max_y(bounding_rect.max.y + edge_width / 2.0),
                    ctx.id.with("bottom_edge"),
                    Sense::DRAG,
                )
                .on_hover_cursor(CursorIcon::ResizeVertical);
            if bottom_edge.dragged() {
                self.body_size.y += bottom_edge.drag_delta().y;
            }
        }

        ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(header_rect.shrink(padding))
                .id(ctx.id.with("header_ui")),
            |ui| {
                ui.horizontal(|ui| {
                    let header = ui.allocate_ui(ui.available_size_before_wrap(), |ui| {
                        add_header(ui);
                    });
                    self.cached_header_content_height = header.response.rect.height();

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let symbol = if self.collapsed { "⏴" } else { "⏷" };
                        if ui.button(symbol).clicked() {
                            self.collapsed = !self.collapsed;
                        }
                    });
                });
            },
        );

        if header_area.dragged() {
            cursor_icon!(ctx.ui, Grabbing);
            ctx.command_queue.push(CanvasCommand::MoveNode {
                idx: ctx.index,
                delta: header_area.drag_delta(),
            });
        }

        if !self.collapsed {
            // Ui for main body
            let body_rect = {
                let mut rect = bounding_rect;
                rect.min.y += header_rect.height();
                rect
            };
            ctx.ui.scope_builder(
                UiBuilder::new()
                    .max_rect(body_rect.shrink(padding))
                    .id(ctx.id.with("body")),
                |ui| {
                    ui.vertical(|ui| {
                        add_main(ui);
                    });
                },
            );
        }
    }

    pub fn wrapping_layouter(
        font: Option<FontId>,
        text_color: Color32,
        alignment: Align,
        wrap_width: f32,
    ) -> impl FnMut(&Ui, &dyn TextBuffer, f32) -> Arc<Galley> {
        move |ui, buffer, _wrap| {
            let mut layout_job = LayoutJob::simple(
                buffer.as_str().to_string(),
                font.clone()
                    .unwrap_or_else(|| TextStyle::Body.resolve(ui.style())),
                text_color,
                wrap_width,
            );
            layout_job.halign = alignment;
            ui.fonts_mut(|f| f.layout_job(layout_job))
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct HeadlessWindow {
    cached_content_height: f32,
    content_width: f32,
    auto_width: bool,
}

impl Default for HeadlessWindow {
    fn default() -> Self {
        Self {
            cached_content_height: 0.0,
            content_width: 600.0,
            auto_width: false,
        }
    }
}

impl HeadlessWindow {
    pub fn auto_width(self) -> Self {
        Self {
            auto_width: true,
            ..self
        }
    }

    /// Returns (bounding_size, padding)
    pub fn size(&self) -> (Vec2, f32) {
        let padding = 10.0;
        let bounding_size = Vec2 {
            x: self.content_width + (padding * 2.0),
            y: self.cached_content_height + (padding * 2.0),
        };

        (bounding_size, padding)
    }

    /// `add_body` must return (Option<height>, Option<width>)
    pub fn show(&mut self, ctx: &mut DrawContext<'_>, add_body: impl FnOnce(&mut Ui) -> Rect) {
        let (bounding_size, padding) = self.size();
        let bounding_rect = Rect::from_min_size(ctx.screen_location, bounding_size);

        if ctx.ui.rect_contains_pointer(bounding_rect.expand(10.0)) {
            ctx.ui.painter().rect(
                bounding_rect,
                ctx.theme.corner_radius,
                Color32::TRANSPARENT,
                ctx.theme.border,
                StrokeKind::Inside,
            );
        }

        let edge_width = 6.0;

        // Dragging left or right edge affects this node if not auto sized
        if !self.auto_width {
            let left_edge = ctx
                .ui
                .interact(
                    bounding_rect
                        .with_min_x(bounding_rect.min.x - edge_width / 2.0)
                        .with_max_x(bounding_rect.min.x + edge_width / 2.0),
                    ctx.id.with("left_edge"),
                    Sense::DRAG | Sense::HOVER,
                )
                .on_hover_cursor(CursorIcon::ResizeHorizontal);
            if left_edge.dragged() {
                let drag_delta = left_edge.drag_delta();
                self.content_width -= drag_delta.x;
                ctx.command_queue.push(CanvasCommand::MoveNode {
                    idx: ctx.index,
                    delta: Vec2::new(drag_delta.x, 0.0),
                });
            }

            let right_edge = ctx
                .ui
                .interact(
                    bounding_rect
                        .with_min_x(bounding_rect.max.x - edge_width / 2.0)
                        .with_max_x(bounding_rect.max.x + edge_width / 2.0),
                    ctx.id.with("right_edge"),
                    Sense::DRAG | Sense::HOVER,
                )
                .on_hover_cursor(CursorIcon::ResizeHorizontal);
            if right_edge.dragged() {
                self.content_width += right_edge.drag_delta().x;
            }
        }

        let top_edge = ctx
            .ui
            .interact(
                bounding_rect
                    .with_min_y(bounding_rect.min.y - edge_width / 2.0)
                    .with_max_y(bounding_rect.min.y + edge_width / 2.0),
                ctx.id.with("top_edge"),
                Sense::DRAG,
            )
            .on_hover_cursor(CursorIcon::Grab);
        if top_edge.dragged() {
            ctx.command_queue.push(CanvasCommand::MoveNode {
                idx: ctx.index,
                delta: top_edge.drag_delta(),
            })
        }

        // Ui for main body
        ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(bounding_rect.shrink(padding))
                .id(ctx.id.with("body")),
            |ui| {
                ui.vertical(|ui| {
                    let rect = add_body(ui);
                    self.cached_content_height = rect.height();
                    if self.auto_width {
                        self.content_width = rect.width();
                    }
                });
            },
        );
    }
}
