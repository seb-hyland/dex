use crate::prelude::*;
use crate::{canvas::CanvasCommand, node::DrawContext};

use eframe::egui::{
    Align, Color32, Frame, Layout, Sense, StrokeKind, TextEdit, TextStyle, UiBuilder,
};

pub struct ViewNode {
    size: Vec2,
    collapsed: bool,
}

impl Default for ViewNode {
    fn default() -> Self {
        Self {
            size: Vec2 { x: 500.0, y: 300.0 },
            collapsed: true,
        }
    }
}

impl ViewNode {
    /// Returns (header_rect, bounding_rect, padding)
    pub fn rects(&self, ui: &mut Ui, location: Pos2) -> (Rect, Rect, f32) {
        let padding = 10.0;

        let header_text_height = ui.text_style_height(&TextStyle::Heading);
        let header_size = Vec2 {
            x: self.size.x,
            y: header_text_height + 2.0 * padding,
        };
        let header_rect = Rect::from_center_size(location, header_size);

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
        header_text: &mut String,
        add_main: impl FnOnce(&mut Ui),
    ) {
        let (header_rect, bounding_rect, padding) = self.rects(ctx.ui, ctx.screen_location);

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
            ctx.theme.faint_background,
            ctx.theme.border,
            StrokeKind::Inside,
        );

        // Header
        {
            let mut ui = ctx.ui.new_child(
                UiBuilder::new()
                    .id(ctx.id)
                    .max_rect(header_rect.shrink(padding)),
            );
            let header_area = ui.interact(header_rect, ui.id().with("header_bar"), Sense::all());
            if header_area.clicked() {
                println!("Clicked!");
            }

            let edge_width = 6.0;

            // Dragging left or right edge always affects this node
            let left_edge = ui.interact(
                bounding_rect
                    .with_min_x(bounding_rect.min.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.min.x + edge_width / 2.0),
                ui.id().with("left_edge"),
                Sense::drag() | Sense::hover(),
            );
            let right_edge = ui.interact(
                bounding_rect
                    .with_min_x(bounding_rect.max.x - edge_width / 2.0)
                    .with_max_x(bounding_rect.max.x + edge_width / 2.0),
                ui.id().with("right_edge"),
                Sense::drag() | Sense::hover(),
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
                    Sense::drag(),
                );
                let bottom_edge = ui.interact(
                    bounding_rect
                        .with_min_y(bounding_rect.max.y - edge_width / 2.0)
                        .with_max_y(bounding_rect.max.y + edge_width / 2.0),
                    ui.id().with("bottom_edge"),
                    Sense::drag(),
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
                ui.vertical(|ui| {
                    TextEdit::singleline(header_text)
                        .background_color(Color32::TRANSPARENT)
                        .font(TextStyle::Heading)
                        .clip_text(true)
                        .frame(Frame::NONE)
                        .show(ui);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
        }

        if !self.collapsed {
            // Ui for main body
            let body_rect = {
                let mut rect = bounding_rect;
                rect.min.y += header_rect.height();
                rect
            };
            ctx.ui
                .scope_builder(UiBuilder::new().max_rect(body_rect.shrink(padding)), |ui| {
                    add_main(ui);
                });
        }
    }
}
