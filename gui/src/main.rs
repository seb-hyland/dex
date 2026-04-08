use crate::prelude::*;
use crate::{
    drawer::Drawer,
    node::{
        NodeVariant,
        primitives::{NumericPayload, TextPayload},
        transform::TransformPayload,
        typst::TypstPayload,
    },
    registry::Registry,
    theme::LIGHT_THEME,
};

use egui::FontData;
use egui_extras::install_image_loaders;
use lib::load::load_delimited_file;

use std::hash::Hash;

use egui::{
    FontFamily, Frame, TextEdit, Visuals,
    text::{CCursor, CCursorRange},
};
use egui_dnd::utils::shift_vec;

mod canvas;
mod drawer;
mod node;
mod prelude;
mod registry;
mod theme;

fn main() {
    dioxus_devtools::connect_subsecond();
    env_logger::init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(Visuals::from(LIGHT_THEME));

            let mut fonts = egui::FontDefinitions::default();

            let mut add_font = |name: &'static str, data: &'static [u8]| {
                fonts
                    .font_data
                    .insert(name.to_owned(), FontData::from_static(data).into());
                fonts
                    .families
                    .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
            };

            add_font("inter", include_bytes!("../assets/Inter_Regular.ttf"));
            add_font("inter_italic", include_bytes!("../assets/Inter_Italic.ttf"));
            add_font("inter_bold", include_bytes!("../assets/Inter_Bold.ttf"));
            add_font(
                "inter_bold_italic",
                include_bytes!("../assets/Inter_BoldItalic.ttf"),
            );

            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "inter".to_owned());

            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(DexState::new()))
        }),
    )
    .unwrap();
}

struct DexState {
    registry: Registry,
    tabs: Vec<Tab>,
    active_tab: usize,
    renaming_tab: RenamingTab,
    drawer: Drawer,
    show_debug: bool,
}

struct Tab {
    name: String,
    canvas: canvas::Canvas,
    add_index: usize,
}

impl Hash for Tab {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.add_index.hash(state);
    }
}

enum RenamingTab {
    None,
    Newly(usize),
    Some(usize),
}

impl DexState {
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            tabs: vec![Tab {
                name: "Unnamed tab 0".to_owned(),
                canvas: canvas::Canvas::default(),
                add_index: 0,
            }],
            active_tab: 0,
            renaming_tab: RenamingTab::None,
            drawer: Drawer {
                visible: false,
                items: Vec::new(),
            },
            show_debug: false,
        }
    }

    fn active_canvas(&mut self) -> &mut canvas::Canvas {
        self.active_canvas_and_registry_disjoint().0
    }

    fn active_canvas_and_registry_disjoint(&mut self) -> (&mut canvas::Canvas, &mut Registry) {
        (
            &mut self.tabs.get_mut(self.active_tab).unwrap().canvas,
            &mut self.registry,
        )
    }
}

impl eframe::App for DexState {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        subsecond::call(|| {
            if self.drawer.visible {
                egui::Panel::left("drawer")
                    .resizable(false)
                    .show_inside(ui, |ui| {});
            }

            egui::Panel::top("tab_bar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let response = egui_dnd::dnd(ui, "tabs_dnd").show_vec(
                        &mut self.tabs,
                        |ui, tab, handle, state| {
                            let idx = state.index;
                            if state.dragged {
                                self.active_tab = idx;
                            }
                            let is_active = idx == self.active_tab;

                            ui.horizontal(|ui| {
                                handle.ui(ui, |ui| {
                                    if let RenamingTab::Newly(idx) | RenamingTab::Some(idx) =
                                        self.renaming_tab
                                    {
                                        if idx == state.index {
                                            let output = TextEdit::singleline(&mut tab.name)
                                                .clip_text(false)
                                                .desired_width(0.0)
                                                .frame(Frame::NONE)
                                                .show(ui);

                                            if matches!(self.renaming_tab, RenamingTab::Newly(_)) {
                                                output.response.request_focus();
                                                let mut state = TextEdit::load_state(
                                                    ui.ctx(),
                                                    output.response.id,
                                                )
                                                .unwrap_or_default();
                                                state.cursor.set_char_range(Some(
                                                    CCursorRange::two(
                                                        CCursor::new(0),
                                                        CCursor::new(tab.name.chars().count()),
                                                    ),
                                                ));
                                                state.store(ui.ctx(), output.response.id);

                                                self.renaming_tab = RenamingTab::Some(idx);
                                            }

                                            if output.response.lost_focus()
                                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                            {
                                                self.renaming_tab = RenamingTab::None;
                                            }
                                        } else {
                                            let _ = ui.selectable_label(is_active, &tab.name);
                                        }
                                    } else {
                                        let label_res = ui.selectable_label(is_active, &tab.name);

                                        if label_res.clicked() {
                                            self.active_tab = idx;
                                        }
                                        if label_res.double_clicked() {
                                            self.renaming_tab = RenamingTab::Newly(idx);
                                        }
                                    }
                                });
                            });
                        },
                    );

                    if let Some(update) = response.update {
                        shift_vec(update.from, update.to, &mut self.tabs);
                    }

                    if ui.button("+").clicked() {
                        let new_tab_idx = self.tabs.len();
                        self.tabs.push(Tab {
                            name: "New tab".to_owned(),
                            canvas: canvas::Canvas::default(),
                            add_index: new_tab_idx,
                        });
                        self.active_tab = new_tab_idx;
                        self.renaming_tab = RenamingTab::Newly(self.active_tab);
                    }
                });
            });

            egui::Panel::top("toolbar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add data").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        let df_result = load_delimited_file(&path);
                        match df_result {
                            Ok(df) => {
                                let (active_canvas, registry) =
                                    self.active_canvas_and_registry_disjoint();
                                active_canvas.add_dataframe(
                                    df,
                                    registry,
                                    path.file_name()
                                        .map(|name| name.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Unnamed dataframe".to_owned()),
                                    Some(path),
                                );
                            }
                            Err(_e) => {}
                        };
                    }
                    if ui.button("Toggle drawer").clicked() {
                        self.drawer.visible = !self.drawer.visible;
                    }
                    if ui.button("Add edge").clicked() {
                        if let canvas::NodeConnectionState::None =
                            self.active_canvas().connecting_nodes
                        {
                            self.active_canvas().connecting_nodes =
                                canvas::NodeConnectionState::Searching;
                        } else {
                            // Eventually add cancel
                        }
                    }
                    if ui.button("Add transform").clicked() {
                        self.active_canvas()
                            .add_node(NodeVariant::Transform(TransformPayload::default()));
                    }
                    if ui.button("Add text").clicked() {
                        self.active_canvas()
                            .add_node(NodeVariant::Text(TextPayload::default()));
                    }
                    if ui.button("Add integer").clicked() {
                        self.active_canvas()
                            .add_node(NodeVariant::Integer(NumericPayload::default()));
                    }
                    if ui.button("Add float").clicked() {
                        self.active_canvas()
                            .add_node(NodeVariant::Float(NumericPayload::default()));
                    }
                    if ui.button("Add typst").clicked() {
                        self.active_canvas()
                            .add_node(NodeVariant::Typst(TypstPayload::default()));
                    }
                    if ui.button("Open debug menu").clicked() {
                        self.show_debug = true;
                    }
                    if ui.button("Save").clicked() {
                        self.active_canvas()
                            .serialize_to_paths(std::path::Path::new(
                                "/Users/seb-hyland/Downloads/dex_serial/test",
                            ));
                    }
                    if ui.button("Load").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        self.active_canvas().load_from_path(path);
                    }
                    let ctx = ui.ctx().clone();
                    egui::Window::new("Debug Inspector")
                        .open(&mut self.show_debug)
                        .show(&ctx, |ui| {
                            ctx.settings_ui(ui);
                        });
                });
            });

            egui::CentralPanel::default().show_inside(ui, |ui| {
                let (active_canvas, registry) = self.active_canvas_and_registry_disjoint();
                active_canvas.draw(ui, registry);
            });
        });
    }
}
