use eframe::egui::{self, Context, Id, Modal, ModalResponse, Vec2};
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

            egui::CentralPanel::default().show(ctx, |ui| {
                self.canvas.view_state.scale = 1.0;
                self.canvas.draw(ui);

                self.canvas.opened_nodes.retain(|idx| {
                    let node = self.canvas.graph.node_weight(*idx).unwrap();

                    let mut open = true;
                    egui::Window::new(node.payload.name())
                        .id(Id::new(idx))
                        .open(&mut open)
                        .resizable([true; 2])
                        .default_width(500.)
                        .show(ctx, |ui| match &node.payload {
                            NodePayload::Dataframe { df, .. } => draw_record_batch(ui, df),
                            NodePayload::Transform { .. } => {}
                        });

                    // Retain if window still open
                    open
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
