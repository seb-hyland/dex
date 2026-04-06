use crate::prelude::*;
use crate::{
    drawer::Drawer,
    node::{
        NodeVariant,
        primitives::{NumericPayload, TextPayload},
        transform::TransformPayload,
    },
    registry::Registry,
    theme::LIGHT_THEME,
};

use lib::load::load_delimited_file;

use eframe::{
    egui::{FontData, FontFamily, Visuals},
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};

mod canvas;
mod drawer;
mod node;
mod prelude;
mod registry;
mod table;
mod theme;

fn main() {
    dioxus_devtools::connect_subsecond();
    env_logger::init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(Visuals::from(LIGHT_THEME));

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
    canvas: canvas::Canvas,
    drawer: Drawer,
    show_debug: bool,
}

impl DexState {
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            canvas: canvas::Canvas {
                graph: canvas::CanvasGraph::new(),
                view_state: canvas::ViewState::new(Rect::ZERO),
                placing_node: None,
                connecting_nodes: canvas::NodeConnectionState::None,
                indices_by_depth: Vec::new(),
            },
            drawer: Drawer {
                visible: false,
                items: Vec::new(),
            },
            show_debug: false,
        }
    }
}

impl eframe::App for DexState {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        subsecond::call(|| {
            if self.drawer.visible {
                egui::Panel::left("drawer")
                    .resizable(false)
                    .show_inside(ui, |ui| {});
            }

            egui::Panel::top("toolbar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add data").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        let df_result = load_delimited_file(&path);
                        match df_result {
                            Ok(df) => {
                                self.canvas.add_dataframe(
                                    df,
                                    &mut self.registry,
                                    path.file_name()
                                        .map(|name| name.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Unnamed dataframe".to_owned()),
                                    Some(path),
                                );
                            }
                            Err(_e) => {}
                        };
                    }
                    if ui.button("Toggle drawer").clicked() {
                        self.drawer.visible = !self.drawer.visible;
                    }
                    if ui.button("Add edge").clicked() {
                        if let canvas::NodeConnectionState::None = self.canvas.connecting_nodes {
                            self.canvas.connecting_nodes = canvas::NodeConnectionState::Searching;
                        } else {
                            // Eventually add cancel
                        }
                    }
                    if ui.button("Add transform").clicked() {
                        self.canvas
                            .add_node(NodeVariant::Transform(TransformPayload::default()));
                    }
                    if ui.button("Add text").clicked() {
                        self.canvas
                            .add_node(NodeVariant::Text(TextPayload::default()));
                    }
                    if ui.button("Add integer").clicked() {
                        self.canvas
                            .add_node(NodeVariant::Integer(NumericPayload::default()));
                    }
                    if ui.button("Add float").clicked() {
                        self.canvas
                            .add_node(NodeVariant::Float(NumericPayload::default()));
                    }
                    if ui.button("Open debug menu").clicked() {
                        self.show_debug = true;
                    }
                    if ui.button("Save").clicked() {
                        self.canvas.serialize_to_paths(std::path::Path::new(
                            "/Users/seb-hyland/Downloads/dex_serial/test",
                        ));
                    }
                    if ui.button("Load").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        self.canvas.load_from_path(path);
                    }
                    let ctx = ui.ctx().clone();
                    egui::Window::new("Debug Inspector")
                        .open(&mut self.show_debug)
                        .show(&ctx, |ui| {
                            ctx.settings_ui(ui);
                        });
                });
            });

            egui::CentralPanel::default().show_inside(ui, |ui| {
                self.canvas.draw(ui, &mut self.registry);
            });
        });
    }
}
