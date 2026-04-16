use crate::canvas::{Canvas, NodeConnectionState};
use crate::node::dataframe::DataframePayload;
use crate::node::{NodeVariant, NodeVariantDiscriminants};
use crate::prelude::*;
use crate::registry::{Registry, RegistryItemInner};

use std::mem;

use egui::{Align, Button, Layout};
use strum::VariantArray;

#[derive(Default)]
pub struct Drawer {
    pub visible: bool,
    pub data_items: Vec<DataframePayload>,
}

type NodeCreator = fn(&mut Ui) -> Option<NodeVariant>;

impl Drawer {
    pub const SIZE: f32 = 160.0;

    pub const NUM_VARIANTS: usize = NodeVariantDiscriminants::VARIANTS.len();

    pub const DRAW_FNS: [NodeCreator; Self::NUM_VARIANTS] = {
        let mut arr = [mem::MaybeUninit::uninit(); Self::NUM_VARIANTS];
        let mut i = 0;

        while i < Self::NUM_VARIANTS {
            let draw_fn = draw_variant(NodeVariantDiscriminants::VARIANTS[i]);
            arr[i] = mem::MaybeUninit::new(draw_fn);

            i += 1;
        }

        unsafe { mem::transmute(arr) }
    };

    pub fn draw(
        &mut self,
        ui: &mut Ui,
        canvas: &mut Canvas,
        registry: &mut Registry,
        background_visible: &mut bool,
        want_center: &mut bool,
    ) {
        ui.add_space(10.0);
        ui.label("Blueprints");
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            for draw_fn in Drawer::DRAW_FNS {
                if let Some(new_node) = draw_fn(ui) {
                    canvas.add_node(new_node);
                }
            }

            let available = canvas.connecting_nodes == NodeConnectionState::None;
            if ui.add_enabled(available, square_button("↕")).clicked() {
                canvas.connecting_nodes = NodeConnectionState::Searching;
            }
        });

        ui.add_space(30.0);
        ui.label("Data");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.add(square_button("+")).clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                let df_result = lib::load::load_delimited_file(&path);
                match df_result {
                    Ok(df) => {
                        let variant = canvas.add_dataframe(
                            df,
                            registry,
                            path.file_stem()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Unnamed dataframe".to_owned()),
                            Some(path),
                        );
                        let dataframe = variant.try_as_dataframe().unwrap();
                        self.data_items.push(dataframe);
                    }
                    Err(_e) => {}
                };
            }

            for data_item in self.data_items.iter() {
                let reg_ref = registry.get(data_item.data_ref).unwrap();
                let RegistryItemInner::Dataframe { table_name, .. } = &reg_ref.borrow().inner;

                if ui.add(square_button(table_name)).clicked() {
                    canvas.add_node(NodeVariant::Dataframe(data_item.clone()));
                }
            }
        });

        ui.allocate_ui_with_layout(
            ui.available_size(),
            Layout::bottom_up(Align::Center),
            |ui| {
                ui.add_space(30.0);
                *want_center = ui.button("Center active canvas").clicked();
                ui.checkbox(background_visible, "Background visible");
            },
        );
    }
}

macro_rules! match_arm {
    ($text:literal for $variant:ident) => {
        |ui| {
            let button = square_button($text);
            if ui.add(button).clicked() {
                Some(NodeVariant::$variant(Default::default()))
            } else {
                None
            }
        }
    };
}

fn square_button(text: &'_ str) -> Button<'_> {
    Button::new(text).min_size(Vec2::splat(40.0))
}

const fn draw_variant(ty: NodeVariantDiscriminants) -> NodeCreator {
    match ty {
        NodeVariantDiscriminants::Dataframe => |_| None,
        NodeVariantDiscriminants::DataframePlot => |_| None,

        NodeVariantDiscriminants::Integer => match_arm!("3" for Integer),
        NodeVariantDiscriminants::Float => match_arm!("3.14" for Float),
        NodeVariantDiscriminants::Text => match_arm!("T" for Text),

        NodeVariantDiscriminants::Typst => match_arm!("ƒ" for Typst),
        // NodeVariantDiscriminants::Webview => |_| None,
        NodeVariantDiscriminants::Image => match_arm!("🖼" for Image),

        NodeVariantDiscriminants::Transform => match_arm!("λ" for Transform),
        NodeVariantDiscriminants::TransformArg => |_| None,
    }
}
