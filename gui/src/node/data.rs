use crate::prelude::*;
use crate::{
    canvas::NodeIdx,
    node::{DrawContext, NodeDynamics},
};

use eframe::egui::UiBuilder;

pub struct TransformPayload {
    name: Vec<(String, String)>,
    code: String,
    input_ports: Vec<NodeIdx>,
}

impl NodeDynamics for TransformPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        ctx.ui.scope_builder(UiBuilder::new(), |ui| {});
    }

    fn size(&self, ctx: &mut DrawContext<'_>) -> Vec2 {
        todo!()
    }
}
