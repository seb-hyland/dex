use crate::node::NodeVariant;
use crate::node::primitives::{NumericPayload, TextPayload};
use crate::node::transform::TransformPayload;
use crate::prelude::*;
use crate::registry::Registry;
use crate::theme::LIGHT_THEME;
use crate::{drawer::Drawer, node::Node};
use lib::{compute::python::apply_transform, load::load_delimited_file};

use eframe::{
    egui::{self, FontData, FontFamily},
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
                            Ok(df) => self.canvas.add_dataframe(&mut self.registry, path, df),
                            Err(e) => {}
                        }
                    }
                    if ui.button("Toggle drawer").clicked() {
                        self.drawer.visible = !self.drawer.visible;
                    }
                    if ui.button("Add edge").clicked() {
                        if let canvas::NodeConnectionState::None = self.canvas.connecting_nodes {
                            self.canvas.connecting_nodes = canvas::NodeConnectionState::Searching;
                        } else {
                            // Do something eventually
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
