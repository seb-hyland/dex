use std::fmt::Display;
use std::sync::Arc;

use arrow::array::{Array, Float64Array};
use arrow::compute::kernels::cast;
use arrow::datatypes::DataType;
use egui::{Align, ComboBox, Frame, TextEdit};
use egui_plot::{BoxElem, BoxPlot, BoxSpread, Plot, PlotBounds, Points};

use crate::canvas::CanvasCommand;
use crate::node::{DrawContext, NodeDynamics, view::Window};
use crate::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct DataframePlotPayload {
    origin_node: NodeIdx,
    name: String,
    view: Window,
    change: PlotChange,
    ty: PlotType,
    // TODO: implement persistance for arrays
    #[serde(skip)]
    cols: Vec<DataCol>,
    #[serde(skip)]
    x_col: Option<usize>,
    #[serde(skip)]
    y_col: Option<usize>,
}

#[derive(PartialEq, Serialize, Deserialize)]
enum PlotType {
    Scatter,
    Boxplot,
}

#[derive(PartialEq, Serialize, Deserialize)]
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
    pub fn new(origin: NodeIdx, origin_name: String, data: &RecordBatch) -> Self {
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
            name: format!("Plotter: {origin_name}"),
            view: Window::default(),
            ty: PlotType::Scatter,
            change: PlotChange::None,
            cols: numeric_cols,
            x_col: None,
            y_col: None,
        }
    }
}

impl NodeDynamics for DataframePlotPayload {
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }

    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let mut scroll_to_command = None;

        self.view.show(
            ctx,
            ctx.theme.background,
            |ui| {
                let editor = TextEdit::singleline(&mut self.name)
                    .background_color(Color32::TRANSPARENT)
                    .clip_text(false)
                    .desired_width(0.0)
                    .layouter(&mut Window::wrapping_layouter(
                        None,
                        ctx.theme.text,
                        Align::Min,
                        ui.available_width(),
                    ))
                    .frame(Frame::NONE)
                    .show(ui);
                editor.text_clip_rect
            },
            |ui| {
                ui.vertical(|ui| {
                    ComboBox::from_id_salt(ui.id().with("plot_type_selector"))
                        .selected_text(self.ty.to_string())
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.ty == PlotType::Scatter,
                                    PlotType::Scatter.to_string(),
                                )
                                .clicked()
                            {
                                self.change = PlotChange::ChangedLastFrame;
                                self.ty = PlotType::Scatter;
                            };
                            if ui
                                .selectable_label(
                                    self.ty == PlotType::Boxplot,
                                    PlotType::Boxplot.to_string(),
                                )
                                .clicked()
                            {
                                self.change = PlotChange::ChangedLastFrame;
                                self.ty = PlotType::Boxplot;
                            };
                        });

                    ui.horizontal(|ui| {
                        let mut selection_box =
                            |ui: &mut Ui, id_str: &'static str, self_col: &mut Option<usize>| {
                                let selected_text = match self_col {
                                    Some(col) => self.cols.get(*col).unwrap().name.as_str(),
                                    None => "",
                                };

                                ComboBox::from_id_salt(ui.id().with(id_str))
                                    .selected_text(selected_text)
                                    .show_ui(ui, |ui| {
                                        for (i, col) in self.cols.iter().enumerate() {
                                            if ui
                                                .selectable_label(*self_col == Some(i), &col.name)
                                                .clicked()
                                            {
                                                self.change = PlotChange::ChangedLastFrame;
                                                *self_col = Some(i);
                                            }
                                        }
                                    });
                            };

                        ui.label("Plot of");
                        match self.ty {
                            PlotType::Scatter => {
                                selection_box(ui, "x_selection", &mut self.x_col);
                                ui.label("vs");
                                selection_box(ui, "y_selection", &mut self.y_col);
                            }
                            PlotType::Boxplot => {
                                selection_box(ui, "x_selection", &mut self.x_col);
                            }
                        }
                    });

                    match self.ty {
                        PlotType::Scatter => {
                            if let (Some(x_col), Some(y_col)) = (self.x_col, self.y_col) {
                                if let Some(selected_idx) = draw_scatterplot(
                                    ui,
                                    self.cols.get(x_col).unwrap(),
                                    self.cols.get(y_col).unwrap(),
                                    &mut self.change,
                                ) {
                                    // User clicked a point!
                                    scroll_to_command = Some(CanvasCommand::ScrollTable {
                                        table_node: self.origin_node,
                                        row: selected_idx,
                                    });
                                }
                                if ui.button("Reset plot bounds").clicked() {
                                    self.change = PlotChange::ChangedLastFrame;
                                }
                            }
                        }
                        PlotType::Boxplot => {
                            if let Some(x_col) = self.x_col {
                                draw_boxplot(ui, self.cols.get(x_col).unwrap(), &mut self.change);
                                if ui.button("Reset plot bounds").clicked() {
                                    self.change = PlotChange::ChangedLastFrame;
                                }
                            }
                        }
                    }
                });
            },
        );

        if let Some(scroll_to_command) = scroll_to_command {
            ctx.command_queue.push(scroll_to_command);
        }
    }
}

#[must_use]
fn draw_scatterplot(
    ui: &mut Ui,
    x: &DataCol,
    y: &DataCol,
    change: &mut PlotChange,
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
            if *change == PlotChange::ChangedLastFrame {
                plot_ui.set_auto_bounds([true, true]);
                *change = PlotChange::None;
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

fn draw_boxplot(ui: &mut Ui, data: &DataCol, just_changed: &mut PlotChange) -> Option<usize> {
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
            match *just_changed {
                PlotChange::ChangedLastFrame => {
                    plot_ui.set_auto_bounds([true, true]);
                    *just_changed = PlotChange::DrawnLastFrame;
                }
                PlotChange::DrawnLastFrame => {
                    let bounds = plot_ui.plot_bounds();

                    let min = bounds.min();
                    let mut max = bounds.max();
                    let height = bounds.height();

                    max[1] += height * 0.6;
                    let new_bounds = PlotBounds::from_min_max(min, max);

                    plot_ui.set_plot_bounds(new_bounds);
                    *just_changed = PlotChange::None;
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
