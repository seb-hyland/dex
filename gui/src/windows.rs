use crate::{canvas::Canvas, node::NodePayload, table::draw_record_batch};

use eframe::egui::{self, Align, CursorIcon, Layout, Rect, Sense, TextStyle, Ui, Vec2};

impl Canvas {
    pub fn draw_windows(&mut self, ui: &mut Ui) {
        let mut i = 0;
        self.opened_nodes.retain_mut(|(idx, pos)| {
            let node = self.graph.node_weight(*idx).unwrap();

            let window = egui::Window::new("")
                .id(ui.id().with(i))
                .resizable([true; 2])
                .default_width(500.)
                .movable(false)
                .title_bar(false);

            let window = if let Some(pos) = pos {
                window.fixed_pos(*pos)
            } else {
                window
            };

            let mut stay_open = true;
            window.show(ui.ctx(), |ui| {
                let top_bar_height = ui.text_style_height(&TextStyle::Heading);
                let ui_rect = ui.max_rect();
                let top_rect = Rect::from_min_size(
                    ui_rect.left_top(),
                    Vec2::new(ui_rect.width(), top_bar_height),
                );
                let top_resp = ui.interact(top_rect, ui.id().with("header_bar"), Sense::all());
                if top_resp.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                    let new_pos =
                        pos.unwrap_or_else(|| ui.max_rect().left_top()) + top_resp.drag_delta();
                    *pos = Some(new_pos);
                } else if top_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }

                ui.horizontal(|ui| {
                    ui.heading(node.payload.name());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button("X")
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            stay_open = false;
                        }
                        if ui
                            .button("🗖")
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            //
                        } else {
                            //
                        }
                    })
                });
                ui.separator();
                match &node.payload {
                    NodePayload::Dataframe { df, .. } => draw_record_batch(ui, df),
                    NodePayload::Transform { .. } => {}
                }
            });

            i += 1;
            // Retain if window still open
            stay_open
        });
    }
}
