use crate::node::NodeVariant;
use crate::node::view::ResizeDir;
use crate::prelude::*;
use crate::{
    node::{DrawContext, LayoutContext, NodeDynamics, view::Window},
    registry::RegistryItemInner,
};

use std::fmt::Display;
use std::sync::Arc;

use arrow::{
    array::{Array, Float64Array},
    compute::kernels::cast,
    datatypes::DataType,
};
use egui::ComboBox;
use egui_plot::{BoxElem, BoxPlot, BoxSpread, Plot, PlotBounds, Points};

#[derive(Clone)]
pub struct DataframePlotPayload {
    origin_node: NodeIdx,
    view: Window,
    change: Transient<PlotChange>,
    ty: PlotType,
    // TODO: implement persistance for arrays
    cols: Vec<DataCol>,
    x_col: Option<usize>,
    y_col: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PlotType {
    Scatter,
    Boxplot,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum PlotChange {
    None,
    ChangedLastFrame,
    DrawnLastFrame,
}

impl Display for PlotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scatter => write!(f, "Scatterplot"),
            Self::Boxplot => write!(f, "Boxplot"),
        }
    }
}

#[derive(Clone)]
struct DataCol {
    arr: Arc<dyn Array>,
    name: String,
}

impl DataframePlotPayload {
    pub fn new(origin: NodeIdx, data: RecordBatch) -> Self {
        let schema = data.schema();
        let fields = schema.fields();

        let numeric_cols: Vec<_> = data
            .columns()
            .iter()
            .zip(fields.iter().map(|field| field.name()))
            .filter_map(|(col, col_name)| {
                let dt = col.data_type();
                if dt.is_numeric() || dt == &DataType::Boolean {
                    Some(DataCol {
                        arr: Arc::clone(col),
                        name: col_name.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Self {
            origin_node: origin,
            view: Window::default(),
            ty: PlotType::Scatter,
            change: Transient::from(PlotChange::None),
            cols: numeric_cols,
            x_col: None,
            y_col: None,
        }
    }
}

impl NodeDynamics for DataframePlotPayload {
    fn step(&self, _ctx: &mut DrawContext<'_>) {}

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        let idx = ctx.index;
        let graph = ctx.graph;

        let root_node_ref = ctx
            .graph
            .get_node(self.origin_node)
            .variant
            .try_as_dataframe_ref()
            .unwrap()
            .data_ref;
        let reg_item = ctx.registry.get(root_node_ref).unwrap();
        let RegistryItemInner::Dataframe { ref table_name, .. } = reg_item.borrow().inner;

        self.view.show(
            ctx,
            ctx.theme.background,
            |ui, _actions| {
                ui.label(format!("Plotter: {table_name}"));
            },
            |ui, actions| {
                ui.vertical(|ui| {
                    action! {
                        SetPlotType { idx: NodeIdx, val: PlotType }
                            does(ctx) {
                                ctx.unwrap_mut_with(idx, NodeVariant::try_as_dataframe_plot_mut)
                                    .ty = val;
                            }
                    }
                    let plot_type_formatter = |ty: PlotType| match ty {
                        PlotType::Boxplot => "Boxplot",
                        PlotType::Scatter => "Scatter",
                    };
                    let action_creator =
                        move |ty| Box::new(SetPlotType { idx, val: ty }) as Box<dyn Action>;
                    let combo_box_interaction = ui.combo_box(
                        ui.id().with("plot_type_selector"),
                        &self.ty,
                        plot_type_formatter(self.ty),
                        vec![
                            (
                                PlotType::Boxplot,
                                plot_type_formatter(PlotType::Boxplot),
                                Box::new(action_creator)
                                    as Box<dyn FnMut(PlotType) -> Box<dyn Action>>,
                            ),
                            (
                                PlotType::Scatter,
                                plot_type_formatter(PlotType::Scatter),
                                Box::new(action_creator)
                                    as Box<dyn FnMut(PlotType) -> Box<dyn Action>>,
                            ),
                        ],
                    );
                    if let Some(action) = combo_box_interaction {
                        actions.push(action);
                    }

                    ui.horizontal(|ui| {
                        #[derive(Clone, Copy)]
                        enum SelectionCol {
                            X,
                            Y,
                        }
                        action! {
                            SetCol { idx: NodeIdx, col: SelectionCol, new_value: Option<usize> }
                                does(ctx) {
                                    let this_node = ctx.unwrap_mut_with(
                                        idx,
                                        NodeVariant::try_as_dataframe_plot_mut
                                    );
                                    this_node.change.set(PlotChange::ChangedLastFrame);
                                    let mod_col = match col {
                                        SelectionCol::X => &mut this_node.x_col,
                                        SelectionCol::Y => &mut this_node.y_col,
                                    };
                                    *mod_col = new_value;
                                }
                        }

                        let selection_box =
                            |ui: &mut Ui, id_str: &'static str, selection_col: SelectionCol| {
                                let self_col = match selection_col {
                                    SelectionCol::X => &self.x_col,
                                    SelectionCol::Y => &self.y_col,
                                };
                                let selected_text = match self_col {
                                    Some(col) => self.cols.get(*col).unwrap().name.as_str(),
                                    None => "",
                                };
                                let items = self.cols.iter().enumerate().map(|(i, col)| {
                                    (
                                        Some(i),
                                        &col.name,
                                        Box::new(move |i| {
                                            Box::new(SetCol {
                                                idx,
                                                col: selection_col,
                                                new_value: i,
                                            })
                                                as Box<dyn Action>
                                        })
                                            as Box<dyn FnMut(Option<usize>) -> Box<dyn Action>>,
                                    )
                                });

                                ui.combo_box(ui.id().with(id_str), self_col, selected_text, items);
                            };

                        ui.label("Plot of");
                        match self.ty {
                            PlotType::Scatter => {
                                selection_box(ui, "x_selection", SelectionCol::X);
                                ui.label("vs");
                                selection_box(ui, "y_selection", SelectionCol::Y);
                            }
                            PlotType::Boxplot => {
                                selection_box(ui, "x_selection", SelectionCol::X);
                            }
                        }
                    });

                    match self.ty {
                        PlotType::Scatter => {
                            if let (Some(x_col), Some(y_col)) = (self.x_col, self.y_col) {
                                if let Some(selected_row) = draw_scatterplot(
                                    ui,
                                    self.cols.get(x_col).unwrap(),
                                    self.cols.get(y_col).unwrap(),
                                    &self.change,
                                ) {
                                    // User clicked a point!
                                    let variant = &graph.get_node(self.origin_node).variant;
                                    let df_node = variant.try_as_dataframe_ref().unwrap();
                                    df_node.scroll_to(selected_row, ui);
                                }
                                if ui.button("Reset plot bounds").clicked() {
                                    self.change.set(PlotChange::ChangedLastFrame);
                                }
                            }
                        }
                        PlotType::Boxplot => {
                            if let Some(x_col) = self.x_col {
                                if ui.button("Reset plot bounds").clicked() {
                                    self.change.set(PlotChange::ChangedLastFrame);
                                }
                                draw_boxplot(ui, self.cols.get(x_col).unwrap(), &self.change);
                            }
                        }
                    }
                });
            },
        );
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.sizes().1
    }
}

#[must_use]
fn draw_scatterplot(
    ui: &mut Ui,
    x: &DataCol,
    y: &DataCol,
    change: &Transient<PlotChange>,
) -> Option<usize> {
    let x_cast = cast(&x.arr, &DataType::Float64).unwrap();
    let x_vals = x_cast.as_any().downcast_ref::<Float64Array>().unwrap();
    let y_cast = cast(&y.arr, &DataType::Float64).unwrap();
    let y_vals = y_cast.as_any().downcast_ref::<Float64Array>().unwrap();

    let plot_points_and_row_indices = x_vals
        .iter()
        .zip(y_vals.iter())
        .enumerate()
        .filter_map(|(i, (x, y))| Some((i, [x?, y?])))
        .collect::<Vec<(usize, [f64; 2])>>();

    let points_element = Points::new(
        format!("{} vs {}", x.name, y.name),
        plot_points_and_row_indices
            .iter()
            .map(|&(_idx, arr)| arr)
            .collect::<Vec<_>>(),
    )
    .name(format!("{} vs {}", x.name, y.name))
    .radius(2.0);

    let plot_resp = Plot::new(ui.id().with("plot"))
        .x_axis_label(&x.name)
        .y_axis_label(&y.name)
        .label_formatter(|_, point| format!("{:.3}, {:.3}", point.x, point.y))
        .show(ui, |plot_ui| {
            if *change.val() == PlotChange::ChangedLastFrame {
                plot_ui.set_auto_bounds([true, true]);
                change.set(PlotChange::None);
            }
            plot_ui.points(points_element);
        });

    if plot_resp.response.clicked() {
        let pointer_pos = plot_resp.response.interact_pointer_pos().unwrap();
        let point = plot_resp.transform.value_from_position(pointer_pos);
        let (row_index, _) = plot_points_and_row_indices
            .iter()
            .min_by(|&(_, [x1, y1]), &(_, [x2, y2])| {
                let d1 = (point.x - x1).powi(2) + (point.y - y1).powi(2);
                let d2 = (point.x - x2).powi(2) + (point.y - y2).powi(2);
                d1.total_cmp(&d2)
            })
            .unwrap();
        Some(*row_index)
    } else {
        None
    }
}

fn draw_boxplot(
    ui: &mut Ui,
    data: &DataCol,
    just_changed: &Transient<PlotChange>,
) -> Option<usize> {
    let data_cast = cast(&data.arr, &DataType::Float64).unwrap();
    let data_vals = data_cast.as_any().downcast_ref::<Float64Array>().unwrap();

    let plot_points_and_row_indices = data_vals
        .iter()
        .enumerate()
        .filter_map(|(i, point)| Some((i, point?)))
        .collect::<Vec<(usize, f64)>>();
    let spread = stats(&plot_points_and_row_indices);

    let box_elem = BoxElem::new(0.0, spread);
    let box_plot = BoxPlot::new(&data.name, vec![box_elem]);

    let plot_resp = Plot::new(ui.id().with("plot"))
        .y_axis_label(&data.name)
        .show(ui, |plot_ui| {
            match *just_changed.val() {
                PlotChange::ChangedLastFrame => {
                    plot_ui.set_auto_bounds([true, true]);
                    just_changed.set(PlotChange::DrawnLastFrame);
                }
                PlotChange::DrawnLastFrame => {
                    let bounds = plot_ui.plot_bounds();

                    let min = bounds.min();
                    let mut max = bounds.max();
                    let height = bounds.height();

                    max[1] += height * 0.6;
                    let new_bounds = PlotBounds::from_min_max(min, max);

                    plot_ui.set_plot_bounds(new_bounds);
                    just_changed.set(PlotChange::None);
                }
                PlotChange::None => {}
            }
            plot_ui.box_plot(box_plot);
        });

    // if plot_resp.response.clicked() {
    //     let pointer_pos = plot_resp.response.interact_pointer_pos().unwrap();
    //     let point = plot_resp.transform.value_from_position(pointer_pos);
    //     let (row_index, _) = plot_points_and_row_indices
    //         .iter()
    //         .min_by(|&(_, [x1, y1]), &(_, [x2, y2])| {
    //             let d1 = (point.x - x1).powi(2) + (point.y - y1).powi(2);
    //             let d2 = (point.x - x2).powi(2) + (point.y - y2).powi(2);
    //             d1.total_cmp(&d2)
    //         })
    //         .unwrap();
    //     Some(*row_index)
    // } else {
    //     None
    // }
    None
}

/// Takes a sorted vector of data
fn stats(data: &[(usize, f64)]) -> BoxSpread {
    let mut data = Vec::from(data);
    data.sort_by(|(_, p1), (_, p2)| p1.total_cmp(p2));
    let len = data.len();

    let min = data[0].1;
    let q1 = data[len / 4].1;
    let median = data[len / 2].1;
    let q3 = data[3 * len / 4].1;
    let max = data[len - 1].1;

    BoxSpread {
        lower_whisker: min,
        quartile1: q1,
        median,
        quartile3: q3,
        upper_whisker: max,
    }
}
