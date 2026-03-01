use arrow::{array::RecordBatch, datatypes::DataType, util::display::array_value_to_string};
use eframe::egui::{Label, ScrollArea, Tooltip, Ui};
use egui_extras::{Column, TableBuilder};

pub fn draw_record_batch(ui: &mut Ui, data: &RecordBatch) {
    ScrollArea::horizontal()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .columns(Column::auto(), data.num_columns())
                .header(20.0, |mut header_row| {
                    for field in data.schema().fields() {
                        header_row.col(|ui| {
                            ui.heading(field.name());
                        });
                    }
                })
                .body(|body| {
                    body.rows(18.0, data.num_rows(), |mut row| {
                        let row_idx = row.index();
                        for col_idx in 0..data.num_columns() {
                            row.col(|ui| {
                                let label_text =
                                    array_value_to_string(data.column(col_idx), row_idx)
                                        .unwrap_or_else(|_| "FORMAT_ERR".to_string());
                                let tooltip = {
                                    let schema = data.schema();
                                    let data_type = schema.fields()[col_idx].data_type();
                                    let cell_type = display_data_type(data_type);
                                    format!("<{cell_type} @ {row_idx}x{col_idx}> {label_text}")
                                };
                                label_with_instant_hover(ui, label_text, tooltip);
                            });
                        }
                    });
                });
        });
}

pub fn display_data_type(ty: &DataType) -> &'static str {
    if ty.is_decimal() {
        "Decimal"
    } else if ty.is_floating() {
        "Float"
    } else if ty.is_integer() {
        "Int"
    } else if ty.is_nested() {
        "Nested"
    } else if ty.is_null() {
        "Null"
    } else if ty.is_string() {
        "String"
    } else if ty.is_temporal() {
        "Temporal"
    } else {
        "Special"
    }
}

pub fn label_with_instant_hover(ui: &mut Ui, label_text: String, tooltip: String) {
    let resp = ui.add(
        Label::new(label_text)
            .truncate()
            .show_tooltip_when_elided(false),
    );
    if resp.hovered() {
        Tooltip::for_widget(&resp).show(|ui| {
            ui.add(Label::new(tooltip));
        });
    }
}
