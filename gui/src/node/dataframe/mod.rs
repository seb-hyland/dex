use crate::{
    canvas::AddConnectedNode,
    node::{
        DrawContext, LayoutContext, NodeDynamics, NodeInitialization, NodeVariant,
        dataframe::plot::DataframePlotPayload,
        view::{ResizeDir, Window},
    },
    prelude::*,
    registry::{RegistryHandle, RegistryItemInner},
};

use egui::{Align, Frame, TextEdit};

pub mod plot;
mod table;

#[derive(Clone)]
pub struct DataframePayload {
    pub data_ref: RegistryHandle,
    pub scroll_to: Transient<Option<usize>>,
    pub highlighted_row: Transient<Option<(usize, f64)>>,
    pub view: Window,
}

impl DataframePayload {
    pub fn new(handle: RegistryHandle) -> Self {
        Self {
            data_ref: handle,
            highlighted_row: Transient::from(None),
            scroll_to: Transient::from(None),
            view: Window::default(),
        }
    }

    pub fn scroll_to(&self, row: usize, ui: &Ui) {
        self.scroll_to.set(Some(row));
        self.highlighted_row.set(Some((row, ui.time())));
    }
}

impl NodeDynamics for DataframePayload {
    fn step(&self, _ctx: &mut DrawContext<'_>) {}

    fn draw(&self, ctx: &mut DrawContext<'_>) {
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
                |ui, _actions| {
                    TextEdit::singleline(table_name)
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
                |ui, _actions| {
                    if ui.button("Plot view").clicked() {
                        create_new_plot = true;
                    }
                    self.highlighted_row.modify(|row| {
                        table::draw_record_batch(ui, data, &self.scroll_to.val(), row)
                    });
                    self.scroll_to.set(None);
                },
            );

            if create_new_plot {
                let idx = ctx.index;
                let batch = data.clone();
                ctx.action_queue.push(AddConnectedNode {
                    origin: ctx.index,
                    constructor: move |_| {
                        NodeVariant::DataframePlot(DataframePlotPayload::new(idx, batch))
                    },
                });
            }
        } else {
            unreachable!("Data table view for non-df registry item")
        }
    }

    #[inline(always)]
    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.sizes().1
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }
}
