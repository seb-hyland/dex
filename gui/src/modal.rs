use eframe::egui::{Align, Context, Id, Layout, Modal, ModalResponse, TextEdit};

use crate::{
    canvas::{Canvas, CanvasGraph, NodeIdx},
    node::NodePayload,
};

pub enum CanvasModal {
    Error(ErrorModal),
    Rename(RenameModal),
}

pub struct ErrorModal {
    pub title: &'static str,
    pub message: String,
}

pub struct RenameModal {
    pub index: NodeIdx,
    pub new_name: String,
}

impl CanvasModal {
    pub fn display(&mut self, canvas: &mut CanvasGraph, ctx: &Context) -> ModalResponse<bool> {
        match self {
            CanvasModal::Error(em) => Self::display_error(ctx, em),
            CanvasModal::Rename(rm) => Self::display_rename(ctx, rm, canvas),
        }
    }

    fn display_error(ctx: &Context, err: &ErrorModal) -> ModalResponse<bool> {
        Modal::new(Id::new(err.title)).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(err.title);
                ui.label(&err.message);
                ui.button("Close").clicked()
            })
            .inner
        })
    }

    fn display_rename(
        ctx: &Context,
        modal_data: &mut RenameModal,
        graph: &mut CanvasGraph,
    ) -> ModalResponse<bool> {
        let node = graph.node_weight_mut(modal_data.index).unwrap();

        Modal::new(Id::new(modal_data.index)).show(ctx, |ui| {
            let mut should_close = false;

            ui.vertical_centered(|ui| {
                ui.heading("Rename node");
                ui.add(TextEdit::singleline(&mut modal_data.new_name));
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                    if ui.button("Finished").clicked() {
                        match &mut node.payload {
                            NodePayload::Dataframe { name, .. } => {
                                *name = modal_data.new_name.clone()
                            }
                            NodePayload::Transform { name, .. } => {
                                *name = modal_data.new_name.clone()
                            }
                        }
                        should_close = true;
                    }
                })
            });

            should_close
        })
    }
}
