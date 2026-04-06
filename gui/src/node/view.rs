use crate::prelude::*;
use crate::{canvas::CanvasCommand, node::DrawContext};

use std::sync::Arc;

use eframe::egui::text::LayoutJob;
use eframe::egui::{Align, Galley, Layout, Sense, StrokeKind, TextBuffer, TextStyle, UiBuilder};

#[derive(Serialize, Deserialize)]
pub struct Window {
    size: Vec2,
    cached_header_height: f32,
    collapsed: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            size: Vec2 { x: 500.0, y: 300.0 },
            cached_header_height: 0.0,
            collapsed: true,
        }
    }
}

impl Window {
    /// Returns (header_rect, bounding_rect, padding)
    pub fn rects(&self, location: Pos2) -> (Rect, Rect, f32) {
        let padding = 10.0;

        let header_height = self.cached_header_height;
        let header_size = Vec2 {
            x: self.size.x,
            y: header_height + 2.0 * padding,
        };
        let header_rect = Rect::from_center_size(
            location
                + Vec2 {
                    x: 0.0,
                    y: header_height / 2.0,
                },
            header_size,
        );

        let bounding_rect = if self.collapsed {
            header_rect
        } else {
            let mut rect = header_rect;
            rect.max.y += self.size.y;
            rect
        };

        (header_rect, bounding_rect, padding)
    }

    pub fn show(
        &mut self,
        ctx: &mut DrawContext<'_>,
        add_header: impl FnOnce(&mut Ui) -> (Rect, Option<Vec<CanvasCommand>>),
        add_main: impl FnOnce(&mut Ui),
    ) {
        let (header_rect, bounding_rect, padding) = self.rects(ctx.screen_location);

        ctx.ui.painter().rect(
            bounding_rect,
            ctx.theme.corner_radius,
            ctx.theme.background,
            ctx.theme.border,
            StrokeKind::Inside,
        );
        ctx.ui.painter().rect(
            header_rect,
            ctx.theme.corner_radius,
            ctx.theme.background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        // Header
        ctx.ui.scope_builder(
            UiBuilder::new()
                .id(ctx.id.with("header"))
                .max_rect(header_rect.shrink(padding)),
            |ui| {
                let header_area = ui.interact(
                    header_rect,
                    ui.id().with("header_bar"),
                    Sense::HOVER | Sense::DRAG,
                );

                let edge_width = 6.0;

                // Dragging left or right edge always affects this node
                let left_edge = ui.interact(
                    bounding_rect
                        .with_min_x(bounding_rect.min.x - edge_width / 2.0)
                        .with_max_x(bounding_rect.min.x + edge_width / 2.0),
                    ui.id().with("left_edge"),
                    Sense::DRAG | Sense::HOVER,
                );
                let right_edge = ui.interact(
                    bounding_rect
                        .with_min_x(bounding_rect.max.x - edge_width / 2.0)
                        .with_max_x(bounding_rect.max.x + edge_width / 2.0),
                    ui.id().with("right_edge"),
                    Sense::DRAG | Sense::HOVER,
                );
                match DrawInteraction::from(left_edge) {
                    DrawInteraction::Hovered => cursor_icon!(ui, ResizeHorizontal),
                    DrawInteraction::Dragged(drag_delta) => {
                        self.size.x -= drag_delta.x;
                        ctx.command_queue.push(CanvasCommand::MoveNode {
                            idx: ctx.index,
                            delta: Vec2::new(drag_delta.x / 2.0, 0.0),
                        })
                    }
                    _ => {}
                }
                match DrawInteraction::from(right_edge) {
                    DrawInteraction::Hovered => cursor_icon!(ui, ResizeHorizontal),
                    DrawInteraction::Dragged(drag_delta) => {
                        self.size.x += drag_delta.x;
                        ctx.command_queue.push(CanvasCommand::MoveNode {
                            idx: ctx.index,
                            delta: Vec2::new(drag_delta.x / 2.0, 0.0),
                        })
                    }
                    _ => {}
                }

                // Dragging top or bottom works when expanded
                if !self.collapsed {
                    let top_edge = ui.interact(
                        bounding_rect
                            .with_min_y(bounding_rect.min.y - edge_width / 2.0)
                            .with_max_y(bounding_rect.min.y + edge_width / 2.0),
                        ui.id().with("top_edge"),
                        Sense::DRAG,
                    );
                    let bottom_edge = ui.interact(
                        bounding_rect
                            .with_min_y(bounding_rect.max.y - edge_width / 2.0)
                            .with_max_y(bounding_rect.max.y + edge_width / 2.0),
                        ui.id().with("bottom_edge"),
                        Sense::DRAG,
                    );
                    match DrawInteraction::from(top_edge) {
                        DrawInteraction::Hovered => cursor_icon!(ui, ResizeVertical),
                        DrawInteraction::Dragged(drag_delta) => {
                            self.size.y -= drag_delta.y;
                            ctx.command_queue.push(CanvasCommand::MoveNode {
                                idx: ctx.index,
                                delta: Vec2::new(0.0, drag_delta.y),
                            })
                        }
                        _ => {}
                    }
                    match DrawInteraction::from(bottom_edge) {
                        DrawInteraction::Hovered => cursor_icon!(ui, ResizeVertical),
                        DrawInteraction::Dragged(drag_delta) => self.size.y += drag_delta.y,
                        _ => {}
                    }
                }

                ui.horizontal(|ui| {
                    ui.with_layout(
                        Layout::left_to_right(Align::Center).with_main_wrap(true),
                        |ui| {
                            let (header_rect, commands) = add_header(ui);
                            self.cached_header_height = header_rect.height();
                            if let Some(command_vec) = commands {
                                ctx.command_queue.extend(command_vec);
                            }
                        },
                    );

                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        let symbol = if self.collapsed { "<" } else { "v" };
                        if ui.button(symbol).clicked() {
                            self.collapsed = !self.collapsed;
                        }
                    });
                });

                match DrawInteraction::from(header_area) {
                    DrawInteraction::Hovered => cursor_icon!(ui, PointingHand),
                    DrawInteraction::Dragged(drag_delta) => {
                        cursor_icon!(ui, Grabbing);
                        ctx.command_queue.push(CanvasCommand::MoveNode {
                            idx: ctx.index,
                            delta: drag_delta,
                        });
                    }
                    _ => {}
                }
            },
        );

        if !self.collapsed {
            // Ui for main body
            let body_rect = {
                let mut rect = bounding_rect;
                rect.min.y += header_rect.height();
                rect
            };
            ctx.ui.scope_builder(
                UiBuilder::new()
                    .id(ctx.id.with("body"))
                    .max_rect(body_rect.shrink(padding)),
                |ui| {
                    add_main(ui);
                },
            );
        }
    }

    pub fn wrapping_layouter(
        text_color: Color32,
        wrap_width: f32,
        suffix: &str,
    ) -> impl FnMut(&Ui, &dyn TextBuffer, f32) -> Arc<Galley> {
        move |ui, buffer, _wrap| {
            let mut output_string = buffer.as_str().to_string();
            output_string.push_str(suffix);

            let layout_job = LayoutJob::simple(
                output_string,
                TextStyle::Heading.resolve(ui.style()),
                text_color,
                wrap_width,
            );
            ui.fonts_mut(|f| f.layout_job(layout_job))
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct HeadlessWindow {
    cached_height: f32,
    width: f32,
    auto_width: bool,
}

impl Default for HeadlessWindow {
    fn default() -> Self {
        Self {
            cached_height: 0.0,
            width: 300.0,
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

    /// Returns (bounding_rect, padding)
    pub fn rect(&self, ctx: &mut DrawContext<'_>) -> (Rect, f32) {
        let padding = 10.0;
        let bounding_rect = Rect::from_min_size(
            ctx.screen_location,
            Vec2 {
                x: self.width + 2.0 * padding,
                y: self.cached_height + 2.0 * padding,
            },
        );

        (bounding_rect, padding)
    }

    /// `add_body` must return (Option<height>, Option<width>)
    pub fn show(&mut self, ctx: &mut DrawContext<'_>, add_body: impl FnOnce(&mut Ui) -> Rect) {
        let (bounding_rect, padding) = self.rect(ctx);

        ctx.ui.painter().rect(
            bounding_rect,
            ctx.theme.corner_radius,
            ctx.theme.background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        ctx.ui.push_id(ctx.id, |ui| {
            let edge_width = 6.0;

            // Dragging left or right edge always affects this node
            let left_edge = ui.interact(
                bounding_rect
                    .with_min_x(bounding_rect.min.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.min.x + edge_width / 2.0),
                ui.id().with("left_edge"),
                Sense::DRAG | Sense::HOVER,
            );
            let right_edge = ui.interact(
                bounding_rect
                    .with_min_x(bounding_rect.max.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.max.x + edge_width / 2.0),
                ui.id().with("right_edge"),
                Sense::DRAG | Sense::HOVER,
            );
            match DrawInteraction::from(left_edge) {
                DrawInteraction::Hovered => cursor_icon!(ui, ResizeHorizontal),
                DrawInteraction::Dragged(drag_delta) => {
                    self.width -= drag_delta.x;
                    ctx.command_queue.push(CanvasCommand::MoveNode {
                        idx: ctx.index,
                        delta: Vec2::new(drag_delta.x, 0.0),
                    })
                }
                _ => {}
            }
            match DrawInteraction::from(right_edge) {
                DrawInteraction::Hovered => cursor_icon!(ui, ResizeHorizontal),
                DrawInteraction::Dragged(drag_delta) => {
                    self.width += drag_delta.x;
                }
                _ => {}
            }

            let top_edge = ui.interact(
                bounding_rect
                    .with_min_y(bounding_rect.min.y - edge_width / 2.0)
                    .with_max_y(bounding_rect.min.y + edge_width / 2.0),
                ui.id().with("top_edge"),
                Sense::DRAG,
            );
            match DrawInteraction::from(top_edge) {
                DrawInteraction::Hovered => cursor_icon!(ui, Grab),
                DrawInteraction::Dragged(drag_delta) => {
                    ctx.command_queue.push(CanvasCommand::MoveNode {
                        idx: ctx.index,
                        delta: drag_delta,
                    })
                }
                _ => {}
            }
        });

        // Ui for main body
        ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(bounding_rect.shrink(padding))
                .id(ctx.id.with("body")),
            |ui| {
                let rect = add_body(ui);
                self.cached_height = rect.height();
                if self.auto_width {
                    self.width = rect.width();
                }
            },
        );
    }
}
