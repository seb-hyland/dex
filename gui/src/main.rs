use eframe::egui::{
    self, Align, Context, CursorIcon, Id, Layout, Modal, ModalResponse, Pos2, Rect, Sense,
    TextStyle, Vec2,
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use lib::{compute::python::apply_transform, load::load_delimited_file};
use petgraph::{graph::NodeIndex, prelude::StableGraph};
use rfd::{FileDialog, MessageDialog};
use std::f32;

use crate::{
    canvas::{Canvas, NodeIdx, NodePayload, ViewState},
    table::{display_data_type, draw_record_batch},
    theme::LIGHT_THEME,
};
mod canvas;
mod table;
mod theme;

fn main() {
    dioxus_devtools::connect_subsecond();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(LIGHT_THEME.clone());
            Ok(Box::new(DexState::new(cc)))
        }),
    )
    .unwrap();
}

struct DexState {
    canvas: Canvas,
    error_modals: Vec<ErrorModal>,
}

pub struct ErrorModal {
    pub title: &'static str,
    pub message: String,
}

impl ErrorModal {
    fn display(&self, ctx: &Context) -> ModalResponse<bool> {
        Modal::new(Id::new(self.title)).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(self.title);
                ui.label(&self.message);
                ui.button("Close").clicked()
            })
            .inner
        })
    }
}

impl DexState {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            canvas: Canvas {
                graph: StableGraph::new(),
                view_state: ViewState {
                    scale: 1.0,
                    offset: Vec2::ZERO,
                },
                newly_added_node: None,
                indices: Vec::new(),
                opened_nodes: Vec::new(),
            },
            error_modals: Vec::new(),
        }
    }
}

impl eframe::App for DexState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        subsecond::call(|| {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add data").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        let df_result = load_delimited_file(&path);
                        match df_result {
                            Ok(df) => self.canvas.add_dataframe(path, df),
                            Err(e) => self.error_modals.push(ErrorModal {
                                title: "Dataframe Load Error",
                                message: format!("{e:?}"),
                            }),
                        }
                    }
                });
            });

            let central_panel = egui::CentralPanel::default().show(ctx, |ui| {
                self.canvas.view_state.scale = 1.0;
                self.canvas.draw(ui);

                let mut i = 0;
                self.canvas.opened_nodes.retain_mut(|(idx, pos)| {
                    let node = self.canvas.graph.node_weight(*idx).unwrap();

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
                    window.show(ctx, |ui| {
                        let top_bar_height = ui.text_style_height(&TextStyle::Heading);
                        let ui_rect = ui.max_rect();
                        let top_rect = Rect::from_min_size(
                            ui_rect.left_top(),
                            Vec2::new(ui_rect.width(), top_bar_height),
                        );
                        let top_resp =
                            ui.interact(top_rect, ui.id().with("header_bar"), Sense::all());
                        if top_resp.dragged() {
                            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                            let new_pos = pos.unwrap_or_else(|| ui.max_rect().left_top())
                                + top_resp.drag_delta();
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
            });

            self.error_modals.retain(|modal| {
                let modal_result = modal.display(ctx);
                let should_close = modal_result.should_close() || modal_result.inner;
                !should_close
            });
        });
    }
}
