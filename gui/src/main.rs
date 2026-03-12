use crate::{
    canvas::{Canvas, NodeIdx, ViewState},
    modal::{CanvasModal, ErrorModal},
    node::NodePayload,
    table::{display_data_type, draw_record_batch},
    theme::LIGHT_THEME,
};
use lib::{compute::python::apply_transform, load::load_delimited_file};

use eframe::{
    egui::{
        self, Align, Context, CursorIcon, FontData, FontFamily, Id, Layout, Modal, ModalResponse,
        Pos2, Rect, Sense, TextStyle, Vec2,
    },
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use petgraph::{graph::NodeIndex, prelude::StableGraph};
use rfd::{FileDialog, MessageDialog};

mod canvas;
mod modal;
mod node;
mod table;
mod theme;
mod windows;

fn main() {
    dioxus_devtools::connect_subsecond();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(LIGHT_THEME.clone());

            let inter_bytes = include_bytes!("../assets/inter.ttf");
            cc.egui_ctx.add_font(FontInsert::new(
                "inter",
                FontData::from_static(inter_bytes),
                vec![InsertFontFamily {
                    family: FontFamily::Proportional,
                    priority: FontPriority::Highest,
                }],
            ));
            Ok(Box::new(DexState::new(cc)))
        }),
    )
    .unwrap();
}

struct DexState {
    canvas: Canvas,
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
                modals: Vec::new(),
            },
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
                            Err(e) => self.canvas.modals.push(CanvasModal::Error(ErrorModal {
                                title: "Dataframe Load Error",
                                message: format!("{e:?}"),
                            })),
                        }
                    }
                });
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                self.canvas.view_state.scale = 1.0;
                self.canvas.draw(ui);
                self.canvas.draw_windows(ui);
                self.canvas.modals.retain_mut(|modal| {
                    let resp = modal.display(&mut self.canvas.graph, ctx);
                    !resp.inner
                });
            });
        });
    }
}
