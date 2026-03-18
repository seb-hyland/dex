use crate::prelude::*;
use crate::registry::Registry;
use crate::{
    canvas::{Canvas, ViewState},
    theme::LIGHT_THEME,
};
use lib::{compute::python::apply_transform, load::load_delimited_file};

use eframe::{
    egui::{self, FontData, FontFamily},
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use petgraph::{graph::NodeIndex, prelude::StableGraph};
use rfd::{FileDialog, MessageDialog};

mod canvas;
mod node;
mod prelude;
mod registry;
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

            let inter_bytes = include_bytes!("../assets/inter.ttf");
            cc.egui_ctx.add_font(FontInsert::new(
                "inter",
                FontData::from_static(inter_bytes),
                vec![InsertFontFamily {
                    family: FontFamily::Proportional,
                    priority: FontPriority::Highest,
                }],
            ));
            Ok(Box::new(DexState::new()))
        }),
    )
    .unwrap();
}

struct DexState {
    registry: Registry,
    canvas: Canvas,
}

impl DexState {
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            canvas: Canvas {
                graph: StableGraph::new(),
                view_state: ViewState::new(Rect::ZERO),
                placing_node: None,
                indices: Vec::new(),
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
                            Ok(df) => self.canvas.add_dataframe(&mut self.registry, path, df),
                            Err(e) => {}
                        }
                    }
                });
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                self.canvas.draw(ui, &mut self.registry);
            });
        });
    }
}
