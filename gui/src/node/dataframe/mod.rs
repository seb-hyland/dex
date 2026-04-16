use crate::{
    canvas::CanvasCommand,
    node::{
        DrawContext, NodeDynamics, NodeVariant, dataframe::plot::DataframePlotPayload, view::Window,
    },
    prelude::*,
    registry::{RegistryHandle, RegistryItemInner},
};

use egui::{Align, Frame, TextEdit};

pub mod plot;
mod table;

#[derive(Clone, Serialize, Deserialize)]
pub struct DataframePayload {
    pub data_ref: RegistryHandle,
    pub scroll_to: Option<usize>,
    pub highlighted_row: Option<(usize, f64)>,
    pub view: Window,
}

impl NodeDynamics for DataframePayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let item = ctx.registry.get(self.data_ref).unwrap();
        let mut create_new_plot = false;

        if let RegistryItemInner::Dataframe {
            ref mut table_name,
            ref data,
        } = item.borrow_mut().inner
        {
            self.view.show(
                ctx,
                ctx.theme.background,
                |ui| {
                    let editor = TextEdit::singleline(table_name)
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
                },
                |ui| {
                    if ui.button("Plot view").clicked() {
                        create_new_plot = true;
                    }
                    table::draw_record_batch(ui, data, self.scroll_to, &mut self.highlighted_row);
                    self.scroll_to = None;
                },
            );

            if create_new_plot {
                ctx.command_queue.push(CanvasCommand::AddNode {
                    origin: ctx.index,
                    node: NodeVariant::DataframePlot(DataframePlotPayload::new(
                        ctx.index,
                        table_name.clone(),
                        data,
                    )),
                });
            }
        } else {
            unreachable!("Data table view for non-df registry item")
        }
    }

    #[inline(always)]
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }
}
