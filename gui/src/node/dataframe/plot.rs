use std::sync::Arc;

use arrow::array::{Array, Float64Array};
use arrow::compute::kernels::cast;
use arrow::datatypes::DataType;
use egui::{Align, ComboBox, Frame, TextEdit};
use egui_plot::{Plot, Points};

use crate::canvas::CanvasCommand;
use crate::node::{DrawContext, NodeDynamics, view::Window};
use crate::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct DataframePlotPayload {
    origin_node: NodeIdx,
    name: String,
    view: Window,
    changed_last_frame: bool,
    // TODO: implement persistance for arrays
    #[serde(skip)]
    cols: Vec<DataCol>,
    #[serde(skip)]
    x_col: Option<usize>,
    #[serde(skip)]
    y_col: Option<usize>,
}

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
            changed_last_frame: false,
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
                                                self.changed_last_frame = true;
                                                *self_col = Some(i);
                                            }
                                        }
                                    });
                            };

                        ui.label("Plot of");
                        selection_box(ui, "x_selection", &mut self.x_col);
                        ui.label("vs");
                        selection_box(ui, "y_selection", &mut self.y_col);
                    });

                    if let (Some(x_col), Some(y_col)) = (self.x_col, self.y_col) {
                        if let Some(selected_idx) = draw_plot(
                            ui,
                            self.cols.get(x_col).unwrap(),
                            self.cols.get(y_col).unwrap(),
                            &mut self.changed_last_frame,
                        ) {
                            // User clicked a point!
                            scroll_to_command = Some(CanvasCommand::ScrollTable {
                                table_node: self.origin_node,
                                row: selected_idx,
                            });
                        }
                        if ui.button("Reset plot bounds").clicked() {
                            self.changed_last_frame = true;
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
fn draw_plot(ui: &mut Ui, x: &DataCol, y: &DataCol, just_changed: &mut bool) -> Option<usize> {
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

    let plot_resp = Plot::new(ui.id().with("plot")).show(ui, |plot_ui| {
        if *just_changed {
            plot_ui.set_auto_bounds([true, true]);
            *just_changed = false;
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
