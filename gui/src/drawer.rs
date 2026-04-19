use crate::canvas::{AddDataframe, AddNode};
use crate::prelude::*;
use crate::{
    canvas::Canvas,
    node::{NodeVariant, NodeVariantDiscriminants, dataframe::DataframePayload},
    registry::{Registry, RegistryItemInner},
};

use std::mem;

use egui::{Align, Button, CursorIcon, Id, Layout};
use strum::VariantArray;

#[derive(Clone)]
pub struct Drawer {
    pub visible: Rigid<bool>,
    pub data_items: Vec<DataframePayload>,
}

impl Default for Drawer {
    fn default() -> Self {
        Self {
            visible: Rigid::from(true),
            data_items: Vec::new(),
        }
    }
}

type NodeCreator = fn(&mut Ui) -> Option<fn(u32) -> NodeVariant>;

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
}

impl Drawer {
    pub fn draw_fluent(
        &self,
        ui: &mut Ui,
        actions: &mut Actions,
        canvas: &mut Canvas,
        registry: &Registry,
    ) {
        let drawer_button_text = if self.visible.val() { "⏴" } else { "⏵" };
        egui::Area::new(Id::new("drawer_handle"))
            .fixed_pos(Pos2 {
                x: if self.visible.val() {
                    Drawer::SIZE
                } else {
                    0.0
                },
                y: ui.max_rect().height() / 2.0,
            })
            .show(ui.ctx(), |ui| {
                if ui
                    .button(drawer_button_text)
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.visible.modify(|vis| *vis = !*vis);
                }
            });

        if !self.visible.val() {
            return;
        };
        egui::Panel::left("drawer")
            .exact_size(Self::SIZE)
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.add_space(10.0);
                ui.label("Blueprints");
                ui.separator();

                ui.horizontal_wrapped(|ui| {
                    for draw_fn in Drawer::DRAW_FNS {
                        if let Some(new_node) = draw_fn(ui) {
                            actions.push(AddNode {
                                constructor: new_node,
                            });
                        }
                    }

                    let available = canvas.can_connect_nodes();
                    if ui.add_enabled(available, square_button("↕")).clicked() {
                        canvas.start_node_connection_search();
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
                                actions.push(AddDataframe { df, path });
                            }
                            Err(_e) => {}
                        };
                    }

                    for data_item in self.data_items.iter() {
                        let reg_ref = registry.get(data_item.data_ref).unwrap();
                        let reg_ref_borrow = reg_ref.borrow();
                        let RegistryItemInner::Dataframe { table_name, data } =
                            &reg_ref_borrow.inner;

                        if ui.add(square_button(table_name)).clicked() {
                            actions.push(AddDataframe {
                                df: data.clone(),
                                path: reg_ref_borrow.backing_file.as_ref().unwrap().clone(),
                            });
                        }
                    }
                });

                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    Layout::bottom_up(Align::Center),
                    |ui| {
                        ui.add_space(30.0);
                        if ui.button("Center active canvas").clicked() {
                            canvas.reset_view();
                        }

                        let background_visible = canvas.background_visible();
                        let label = |is_visible| if is_visible { "Visible" } else { "Empty" };
                        action! {
                            SetCanvasBackgroundVisibility { visible: bool }
                                does(ctx) {
                                    ctx.unwrap_active_canvas().set_background_visible(visible);
                                }
                        }

                        ui.combo_box(
                            ui.id().with("background_combo_box"),
                            &background_visible,
                            label(background_visible),
                            vec![false, true].into_iter().map(|vis| {
                                (
                                    vis,
                                    label(vis),
                                    Box::new(|vis| {
                                        Box::new(SetCanvasBackgroundVisibility { visible: vis })
                                            as Box<dyn Action>
                                    })
                                        as Box<dyn FnMut(bool) -> Box<dyn Action>>,
                                )
                            }),
                        );
                    },
                );
            });
    }
}

macro_rules! match_arm {
    ($text:literal for default $variant:ident) => {
        match_arm!(@private $text, $variant, |_| Default::default())
    };
    ($text:literal for init $variant:ident) => {
        match_arm!(@private $text, $variant, |i| $crate::node::NodeInitialization::init(i))
    };
    (@private $text:literal, $variant:ident, $constructor:expr) => {
        |ui| {
            let button = square_button($text);
            if ui.add(button).clicked() {
                Some(|i| NodeVariant::$variant($constructor(i)))
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

        NodeVariantDiscriminants::Integer => {
            match_arm!("3" for init Integer)
        }
        NodeVariantDiscriminants::Float => match_arm!("3.14" for init Float),
        NodeVariantDiscriminants::Text => match_arm!("T" for init Text),

        NodeVariantDiscriminants::Typst => match_arm!("ƒ" for init Typst),
        // NodeVariantDiscriminants::Webview => |_| None,
        NodeVariantDiscriminants::Image => match_arm!("🖼" for default Image),

        NodeVariantDiscriminants::Transform => match_arm!("λ" for init Transform),
        NodeVariantDiscriminants::TransformArg => |_| None,
    }
}
