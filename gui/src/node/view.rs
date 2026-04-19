use crate::node::DrawContext;
use crate::node::MoveNode;
use crate::node::NodeDynamics;
use crate::prelude::*;

use std::sync::Arc;

use eframe::egui::text::LayoutJob;
use eframe::egui::{Align, Galley, Layout, Sense, StrokeKind, TextBuffer, TextStyle, UiBuilder};
use egui::{CursorIcon, FontId};

#[derive(Clone)]
pub struct Window {
    body_size: Vec2,
    cached_header_content_height: Transient<f32>,
    collapsed: Rigid<bool>,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            body_size: Vec2 { x: 500.0, y: 300.0 },
            cached_header_content_height: Transient::from(0.0),
            collapsed: Rigid::from(true),
        }
    }
}

impl Window {
    pub fn handle_resize(&mut self, dir: ResizeDir, delta: Vec2) {
        match dir {
            ResizeDir::Left => self.body_size.x -= delta.x,
            ResizeDir::Right => self.body_size.x += delta.x,
            ResizeDir::Top => self.body_size.y -= delta.y,
            ResizeDir::Bottom => self.body_size.y += delta.y,
        }
    }

    /// Returns (header_size, bounding_size, padding)
    pub fn sizes(&self) -> (Vec2, Vec2, f32) {
        let padding = 10.0;

        let header_height = *self.cached_header_content_height.val();
        let header_size = Vec2 {
            x: self.body_size.x,
            y: header_height + 2.0 * padding,
        };

        let bounding_size = if self.collapsed.val() {
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
        &self,
        ctx: &mut DrawContext<'_>,
        background: Color32,
        add_header: impl FnOnce(&mut Ui, &mut Actions),
        add_main: impl FnOnce(&mut Ui, &mut Actions),
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
        {
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
                ctx.action_queue.push(Resize {
                    idx: ctx.index,
                    dir: ResizeDir::Left,
                    delta: left_edge.drag_delta(),
                });
            }
        }
        {
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
                ctx.action_queue.push(Resize {
                    idx: ctx.index,
                    dir: ResizeDir::Right,
                    delta: right_edge.drag_delta(),
                });
            }
        }

        // Dragging bottom works when expanded
        if !self.collapsed.val() {
            {
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
                    ctx.action_queue.push(Resize {
                        idx: ctx.index,
                        dir: ResizeDir::Top,
                        delta: top_edge.drag_delta(),
                    });
                }
            }
            {
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
                    ctx.action_queue.push(Resize {
                        idx: ctx.index,
                        dir: ResizeDir::Bottom,
                        delta: bottom_edge.drag_delta(),
                    });
                }
            }
        }

        ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(header_rect.shrink(padding))
                .id(ctx.id.with("header_ui")),
            |ui| {
                ui.horizontal(|ui| {
                    let header = ui.allocate_ui(ui.available_size_before_wrap(), |ui| {
                        add_header(ui, ctx.action_queue);
                    });
                    self.cached_header_content_height
                        .set(header.response.rect.height());

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let symbol = if self.collapsed.val() { "⏴" } else { "⏷" };
                        if ui.button(symbol).clicked() {
                            self.collapsed.modify(|collapsed| *collapsed = !*collapsed);
                        }
                    });
                });
            },
        );

        if header_area.dragged() {
            cursor_icon!(ctx.ui, Grabbing);
            ctx.action_queue.push(MoveNode {
                idx: ctx.index,
                delta: header_area.drag_delta(),
            });
        }

        if !self.collapsed.val() {
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
                        add_main(ui, ctx.action_queue);
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

#[derive(Clone)]
pub struct HeadlessWindow {
    cached_content_height: Transient<f32>,
    content_width: ContentWidth,
}

#[derive(Clone)]
enum ContentWidth {
    Manual(f32),
    Auto(Transient<f32>),
}

impl ContentWidth {
    fn value(&self) -> f32 {
        match self {
            Self::Manual(v) => *v,
            Self::Auto(v) => *v.val(),
        }
    }
}

impl Default for HeadlessWindow {
    fn default() -> Self {
        Self {
            cached_content_height: Transient::from(0.0),
            content_width: ContentWidth::Manual(600.0),
        }
    }
}

impl HeadlessWindow {
    pub fn auto_width(self) -> Self {
        Self {
            content_width: ContentWidth::Auto(Transient::from(0.0)),
            ..self
        }
    }

    pub fn handle_resize(&mut self, dir: ResizeDir, delta: Vec2) {
        let ContentWidth::Manual(width) = &mut self.content_width else {
            unreachable!("Attempted to resize HeadlessWindow with auto content width");
        };
        match dir {
            ResizeDir::Left => *width -= delta.x,
            ResizeDir::Right => *width += delta.x,
            _ => unreachable!("Commanded HeadlessWindow to vertically resize"),
        }
    }

    /// Returns (bounding_size, padding)
    pub fn size(&self) -> (Vec2, f32) {
        let padding = 10.0;
        let bounding_size = Vec2 {
            x: self.content_width.value() + (padding * 2.0),
            y: *self.cached_content_height.val() + (padding * 2.0),
        };

        (bounding_size, padding)
    }

    /// `add_body` must return (Option<height>, Option<width>)
    pub fn show(
        &self,
        ctx: &mut DrawContext<'_>,
        add_body: impl FnOnce(&mut Ui, &mut Actions) -> Rect,
    ) {
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
        if matches!(self.content_width, ContentWidth::Manual(_)) {
            {
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
                    ctx.action_queue.push(Resize {
                        idx: ctx.index,
                        dir: ResizeDir::Left,
                        delta: left_edge.drag_delta(),
                    });
                }
            }
            {
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
                    ctx.action_queue.push(Resize {
                        idx: ctx.index,
                        dir: ResizeDir::Right,
                        delta: right_edge.drag_delta(),
                    });
                }
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
            ctx.action_queue.push(MoveNode {
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
                    let rect = add_body(ui, ctx.action_queue);
                    self.cached_content_height.set(rect.height());
                    if let ContentWidth::Auto(width) = &self.content_width {
                        width.set(rect.width());
                    }
                });
            },
        );
    }
}

pub enum ResizeDir {
    Left,
    Right,
    Top,
    Bottom,
}
action! {
    Resize { idx: NodeIdx, dir: ResizeDir, delta: Vec2 }
        does(ctx) {
            ctx.unwrap_active_canvas().get_node_mut(idx).variant.resize(dir, delta);
        }
}
