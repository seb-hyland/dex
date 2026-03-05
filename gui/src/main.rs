use eframe::egui::{
    self, Button, Id, InnerResponse, Label, LayerId, Modal, Popup, PopupAnchor, Pos2, TextEdit,
    Window,
};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use egui_graphs::{
    Graph, GraphView, LayoutHierarchical, SettingsInteraction, SettingsNavigation, SettingsStyle,
};
use lib::{compute::python::apply_transform, load::load_delimited_file};
use petgraph::{graph::NodeIndex, prelude::StableGraph};
use rfd::{FileDialog, MessageDialog};
use std::f32;

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

#[derive(PartialEq)]
enum TransformState {
    None,
    WaitingSelection,
    Editing {
        node_idx: NodeIndex<u32>,
        code: String,
        name: String,
    },
}

struct DexState {
    graph: DisplayGraph,
    transform_state: TransformState,
    previous_node: Option<NodeIndex<u32>>,
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
            previous_node: None,
            windows: Vec::new(),
            reset_view: true,
            transform_state: TransformState::None,
        }
    }
}

impl eframe::App for DexState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("nav_panel")
            .default_height(40.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(Button::new("Load data...")).clicked()
                        && let Some(file) = FileDialog::new().pick_file()
                    {
                        match load_delimited_file(&file) {
                            Ok(data) => {
                                let payload = SharedNode::new(Node {
                                    inner: NodeInner::Dataframe { data },
                                    name: file.file_name().unwrap().to_string_lossy().to_string(),
                                });

                                let i = self.graph.add_node(payload);

                                // First node added
                                if let Some(prev_idx) = self.previous_node {
                                    // If only one node was added, reset its location to 0
                                    // If we set it on insertion of the first node, egui moves it
                                    if self.graph.node_count() == 1 {
                                        let (i, _) = self.graph.nodes_iter().next().unwrap();
                                        self.graph.node_mut(i).unwrap().set_location(Pos2::ZERO);
                                    }

                                    let prev_node = self.graph.node(prev_idx).unwrap();
                                    let prev_node_edge = {
                                        let location = prev_node.location().x;
                                        let half_width = prev_node.display().size().x / 2.0;
                                        location + half_width
                                    };

                                    let added_node = self.graph.node_mut(i).unwrap();
                                    let new_center = {
                                        let padding = 50.0;
                                        let half_width = added_node.display().size().x / 2.0;
                                        prev_node_edge + padding + half_width
                                    };

                                    added_node.set_location(Pos2 {
                                        x: new_center,
                                        y: 0.0,
                                    });
                                }
                                self.previous_node = Some(i);

                                print!("Locations: ");
                                self.graph
                                    .nodes_iter()
                                    .map(|(_, node)| node.location())
                                    .for_each(|location| print!("{location}; "));
                                println!();
                            }
                            Err(e) => {
                                MessageDialog::new()
                                    .set_description(format!("{e:#?}"))
                                    .show();
                            }
                        }
                    }
                    match self.transform_state {
                        TransformState::None => {
                            if ui.add(Button::new("Transform a node...")).clicked() {
                                self.transform_state = TransformState::WaitingSelection
                            }
                        }
                        TransformState::WaitingSelection => {
                            ui.add(Button::new("Select a node to transform"));
                        }
                        TransformState::Editing { .. } => {
                            ui.add(Button::new("Editing a transformation..."));
                        }
                    }
                    if ui.add(Button::new("Reset graph")).clicked() {
                        self.reset_view = true;
                    }
                });
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
                if self.graph.edge_count() == 0 {
                    let mut view = GraphView::<'_, _, _, _, _, _, _, _>::new(&mut self.graph)
                        .with_styles(&style)
                        .with_navigations(&navigation)
                        .with_interactions(&interaction);
                    ui.add(&mut view);
                } else {
                    let mut view = GraphView::<'_, _, _, _, _, _, _, _, LayoutHierarchical>::new(
                        &mut self.graph,
                    )
                    .with_styles(&style)
                    .with_navigations(&navigation)
                    .with_interactions(&interaction);
                    ui.add(&mut view);
                }
            }
        });

        if let TransformState::Editing {
            node_idx,
            code,
            name,
        } = &mut self.transform_state
        {
            let close = Window::new("Transformation")
                .resizable([true; 2])
                .default_width(500.0)
                .show(ctx, |ui| {
                    ui.heading("Name");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY));

                    ui.heading("Transformation");
                    CodeEditor::default()
                        .id_source("TransformationEditor")
                        .with_rows(15)
                        .with_fontsize(12.0)
                        .with_theme(ColorTheme::AYU)
                        .with_syntax(Syntax::python())
                        .show(ui, code);

                    ui.add(Button::new("Finished")).clicked()
                });
            if let Some(InnerResponse {
                inner: Some(true), ..
            }) = close
            {
                let node = self.graph.node(*node_idx).unwrap();
                let dataframe = if let Node {
                    inner: NodeInner::Dataframe { data },
                    ..
                } = node.payload().as_ref()
                {
                    data
                } else {
                    panic!("Should not transform a transform");
                };

                let transform_result = apply_transform(
                    vec![dataframe],
                    code,
                    Some(
                        "/Users/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages",
                    ),
                );
                match transform_result {
                    Ok(data) if !name.is_empty() => {
                        let payload = SharedNode::new(Node {
                            inner: NodeInner::Dataframe { data },
                            name: name.clone(),
                        });
                        let i = self.graph.add_node(payload);
                        self.graph.add_edge(*node_idx, i, ());
                        self.transform_state = TransformState::None;
                        self.reset_view = true;
                    }
                    Ok(_) => {
                        Modal::new(Id::new("Transform Unnamed")).show(ctx, |ui| {
                            ui.label("Transformation must have a name.");
                        });
                    }
                    Err(e) => {
                        Modal::new(Id::new("Transform Error")).show(ctx, |ui| {
                            ui.label("Transformation failed:");
                            ui.label(format!("{e:#?}"));
                        });
                    }
                }
            }
        }

        // Max one can be selected at a time
        if let Some(&idx) = self.graph.selected_nodes().first() {
            if self.transform_state == TransformState::WaitingSelection {
                let code_template = r#"import pyarrow as pa
import polars as pl

def transform(batch):
    df = pl.from_arrow(batch)
    # Add code here!
    return output_df"#;
                self.transform_state = TransformState::Editing {
                    node_idx: idx,
                    code: String::from(code_template),
                    name: String::new(),
                };
            } else {
                self.windows.push(idx);
            }
            self.graph.node_mut(idx).unwrap().set_selected(false);
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
