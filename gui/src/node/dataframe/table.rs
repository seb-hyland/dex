use crate::prelude::*;

use std::sync::Arc;

use arrow::{datatypes::DataType, util::display::array_value_to_string};
use eframe::egui::{Align, FontSelection, Label, RichText, ScrollArea, Tooltip, WidgetText};
use egui_extras::{Column, TableBuilder};

const HIGHLIGHT_DURATION: f64 = 2.0;
const ANIMATION_TIME: f64 = 0.5;

pub fn draw_record_batch(
    ui: &mut Ui,
    data: &RecordBatch,
    scroll_to: Option<usize>,
    highlight: &mut Option<(usize, f64)>,
) {
    // Revert visuals changes
    ui.scope(|ui| {
        if let Some((_highlight_idx, start_time)) = highlight {
            let time_elapsed = ui.input(|i| i.time) - *start_time;
            let time_remaining = HIGHLIGHT_DURATION - time_elapsed;

            if time_remaining < 0.0 {
                *highlight = None;
            } else {
                ui.request_repaint();
                let animation_proportion = (time_remaining / ANIMATION_TIME).min(1.0);
                let bg = &mut ui.visuals_mut().selection.bg_fill;
                *bg = bg.gamma_multiply(animation_proportion as f32);
            }
        };

        ScrollArea::horizontal()
            .id_salt(ui.id().with("scroll_area"))
            .show(ui, |ui| {
                let (widths, headers): (Vec<_>, Vec<_>) = data
                    .schema()
                    .fields()
                    .iter()
                    .map(|field| {
                        let text = Arc::new(RichText::new(field.name()).heading());
                        let widget_text: WidgetText = Arc::clone(&text).into();
                        let job = widget_text.into_layout_job(
                            ui.style(),
                            FontSelection::Default,
                            Align::Min,
                        );
                        let width = ui.fonts_mut(|fonts| {
                            let galley = fonts.layout_job(Arc::try_unwrap(job).unwrap());
                            galley.rect.width()
                        });
                        (width.clamp(25.0, 100.0), text)
                    })
                    .collect();

                let available_height = ui.available_height();
                let id = ui.id().with("table");
                let mut table = TableBuilder::new(ui)
                    .id_salt(id)
                    .min_scrolled_height(0.0)
                    .max_scroll_height(available_height)
                    .striped(true);

                if let Some(row) = scroll_to {
                    table = table.scroll_to_row(row, Some(Align::Center));
                }

                for width in widths {
                    table =
                        table.column(Column::auto_with_initial_suggestion(width).resizable(true));
                }

                table
                    .header(20.0, |mut header_row| {
                        for header in headers {
                            header_row.col(|ui| {
                                label_with_instant_hover(ui, header.clone(), header);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(18.0, data.num_rows(), |mut row| {
                            let row_idx = row.index();

                            if let Some((highlight_idx, _start_time)) = highlight
                                && row_idx == *highlight_idx
                            {
                                row.set_selected(true);
                            }
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

pub fn label_with_instant_hover(
    ui: &mut Ui,
    label_text: impl Into<WidgetText>,
    tooltip: impl Into<WidgetText>,
) {
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
