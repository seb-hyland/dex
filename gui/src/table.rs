use std::sync::Arc;

use arrow::{array::RecordBatch, datatypes::DataType, util::display::array_value_to_string};
use eframe::egui::{
    Align, FontId, FontSelection, Label, RichText, ScrollArea, Style, TextFormat, Tooltip, Ui,
    WidgetText, text::LayoutJob,
};
use egui_extras::{Column, TableBuilder};

pub fn draw_record_batch(ui: &mut Ui, data: &RecordBatch, height: f32) {
    ScrollArea::horizontal().show(ui, |ui| {
        let (widths, headers): (Vec<_>, Vec<_>) = data
            .schema()
            .fields()
            .iter()
            .map(|field| {
                let text = Arc::new(RichText::new(field.name()).heading());
                let widget_text: WidgetText = Arc::clone(&text).into();
                let job =
                    widget_text.into_layout_job(ui.style(), FontSelection::Default, Align::Min);
                let width = ui.fonts_mut(|fonts| {
                    let galley = fonts.layout_job(Arc::try_unwrap(job).unwrap());
                    galley.rect.width()
                });
                (width.min(500.0), text)
            })
            .collect();

        let mut table = TableBuilder::new(ui)
            .min_scrolled_height(height - 20.0)
            .striped(true);

        for width in widths {
            table = table.column(Column::initial(width).resizable(true));
        }
        table
            .header(20.0, |mut header_row| {
                for header in headers {
                    header_row.col(|ui| {
                        ui.add(Label::new(header).truncate());
                    });
                }
            })
            .body(|body| {
                body.rows(18.0, data.num_rows(), |mut row| {
                    let row_idx = row.index();
                    for col_idx in 0..data.num_columns() {
                        row.col(|ui| {
                            let label_text = array_value_to_string(data.column(col_idx), row_idx)
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
