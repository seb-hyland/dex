use crate::prelude::*;
use crate::{
    node::{DrawContext, NodeDynamics, view::Window},
    registry::{RegistryHandle, RegistryItemInner},
    table::draw_record_batch,
};

use eframe::egui::{Frame, TextEdit, TextStyle};

#[derive(Serialize, Deserialize)]
pub struct DataframePayload {
    pub data_ref: RegistryHandle,
    pub view: Window,
}

impl NodeDynamics for DataframePayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let item = ctx.registry.get(self.data_ref).unwrap();

        if let RegistryItemInner::Dataframe {
            ref mut table_name,
            ref data,
        } = item.borrow_mut().inner
        {
            self.view.show(
                ctx,
                |ui| {
                    let editor = TextEdit::singleline(table_name)
                        .background_color(Color32::TRANSPARENT)
                        .font(TextStyle::Heading)
                        .clip_text(false)
                        .desired_width(0.0)
                        .layouter(&mut Window::wrapping_layouter(
                            ctx.theme.text,
                            ui.available_width(),
                            "",
                        ))
                        .frame(Frame::NONE)
                        .show(ui);
                    (editor.text_clip_rect, None)
                },
                |ui| {
                    draw_record_batch(ui, data);
                },
            );
        } else {
            unreachable!("Data table view for non-df registry item")
        }
    }

    #[inline(always)]
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.screen_location).1
    }
}
