use egui::{Color32, FontId, text::LayoutJob};
use serde::{Deserialize, Serialize};
use workspace::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct Label {
    text: String,
    singleline: bool,
    font: FontId,
    color: Color32,
}

#[typetag::serde]
impl Node for Label {
    fn draw(&self, ctx: &mut DrawContext) -> DrawResult {
        let job = LayoutJob::simple_singleline(self.text.clone(), self.font.clone(), self.color);
        let galley = ctx.ui.fonts_mut(|f| f.layout_job(job));
        if self.singleline {
        } else {
        };
        todo!()
    }

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {}
}

impl Requestable for Label {
    fn request(&self, body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
