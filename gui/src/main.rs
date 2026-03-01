use eframe::egui::{
    self, Button, Id, Label, LayerId, PointerState, Popup, PopupAnchor, Pos2, Rect, Response,
    Tooltip,
};
use egui_graphs::{
    Graph, GraphView, LayoutHierarchical, SettingsInteraction, SettingsNavigation, SettingsStyle,
};
use lib::load::load_delimited_file;
use petgraph::{graph::NodeIndex, prelude::StableGraph};
use rfd::{FileDialog, MessageDialog};

use crate::{
    graph::{DisplayGraph, Node, NodeInner, SharedNode},
    table::{display_data_type, draw_record_batch},
    theme::LIGHT_THEME,
};
mod graph;
mod table;
mod theme;

fn main() {
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
    graph: DisplayGraph,
    windows: Vec<NodeIndex<u32>>,
    reset_view: bool,
}

impl DexState {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut g = StableGraph::new();
        // g.add_node(SharedNode::new(Node {
        //     inner: NodeInner::Transformation {},
        //     name: "mylongfilename.csv".to_owned(),
        // }));
        // g.add_node(SharedNode::new(Node {
        //     inner: NodeInner::Transformation {},
        //     name: "myotherfile.csv".to_owned(),
        // }));
        let graph = Graph::from(&g);
        Self {
            graph,
            windows: Vec::new(),
            reset_view: true,
        }
    }
}

impl eframe::App for DexState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("nav_panel")
            .default_height(40.)
            .show(ctx, |ui| {
                if ui.add(Button::new("Load data...")).clicked()
                    && let Some(file) = FileDialog::new().pick_file()
                {
                    match load_delimited_file(&file) {
                        Ok(data) => {
                            let payload = SharedNode::new(Node {
                                inner: NodeInner::Dataframe { data },
                                name: file.file_name().unwrap().to_string_lossy().to_string(),
                            });
                            self.graph.add_node(payload);
                        }
                        Err(e) => {
                            MessageDialog::new()
                                .set_description(format!("{e:#?}"))
                                .show();
                        }
                    }
                }
                if ui.add(Button::new("Reset graph")).clicked() {
                    self.reset_view = true;
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let style = SettingsStyle::new();

            let interaction = SettingsInteraction::new()
                .with_dragging_enabled(false)
                .with_node_selection_enabled(true)
                .with_edge_selection_enabled(false);
            let navigation = if !self.reset_view {
                SettingsNavigation::new()
                    .with_fit_to_screen_enabled(false)
                    .with_zoom_and_pan_enabled(true)
            } else {
                self.reset_view = false;
                SettingsNavigation::new().with_fit_to_screen_padding(5.)
            };

            if self.graph.node_count() > 0 {
                if self.graph.edge_count() != 0 {
                    let mut view = GraphView::<'_, _, _, _, _, _, _, _, LayoutHierarchical>::new(
                        &mut self.graph,
                    )
                    .with_styles(&style)
                    .with_navigations(&navigation)
                    .with_interactions(&interaction);
                    ui.add(&mut view);
                } else {
                    // If no edges, use a layout with all nodes in a horizontal line
                    let mut view = GraphView::<'_, _, _, _, _, _, _, _>::new(&mut self.graph)
                        .with_styles(&style)
                        .with_navigations(&navigation)
                        .with_interactions(&interaction);
                    ui.add(&mut view);
                }
            }
        });

        // Max one can be selected at a time
        if let Some(idx) = self.graph.selected_nodes().first() {
            self.windows.push(*idx);
            self.graph.node_mut(*idx).unwrap().set_selected(false);
        }

        let mut counter = 0;
        self.windows.retain(|idx| {
            let node = self.graph.node(*idx).unwrap();

            let mut open = true;
            let payload = node.payload();
            egui::Window::new(format!("{} ({})", payload.name, counter))
                .open(&mut open)
                .resizable([true; 2])
                .default_width(500.)
                .show(ctx, |ui| match &payload.inner {
                    NodeInner::Dataframe { data } => draw_record_batch(ui, data),
                    NodeInner::Transformation {} => {}
                });

            counter += 1;
            // Retain if window still open
            open
        });

        if let Some(idx) = self.graph.hovered_node() {
            let node = self.graph.node(idx).unwrap();
            let info = if let NodeInner::Dataframe { data } = &node.payload().inner {
                let (rows, cols) = (data.num_rows(), data.num_columns());
                let fields = data
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| format!("{} ({})", f.name(), display_data_type(f.data_type())))
                    .enumerate()
                    .fold(String::new(), |mut acc, (i, s)| {
                        if i > 5 {
                            // First elided element
                            if i == 6 {
                                acc.push_str(", ...");
                            }
                            return acc;
                        }

                        if !acc.is_empty() {
                            acc.push_str(", ");
                        }
                        acc.push_str(&s);
                        acc
                    });
                let filename = &node.payload().name;

                format!(
                    r#"File: {filename}
Dimensions: {rows} rows x {cols} columns
Fields: {fields}"#
                )
            } else {
                "".to_string()
            };

            let popup_id = Id::new("Hovered Node Popup");
            Popup::new(
                popup_id,
                ctx.clone(),
                PopupAnchor::Pointer,
                LayerId::new(egui::Order::Tooltip, popup_id),
            )
            .gap(20.0 * ctx.zoom_factor())
            .show(|ui| {
                ui.add(Label::new(info));
            });
        }
    }
}
