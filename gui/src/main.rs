use crate::canvas::Canvas;
use crate::prelude::*;
use crate::{drawer::Drawer, registry::Registry, theme::LIGHT_THEME};

use egui::{
    CursorIcon, FontData, FontFamily, Frame, Id, TextEdit, Visuals,
    text::{CCursor, CCursorRange},
};
use egui_dnd::utils::shift_vec;
use egui_extras::install_image_loaders;

use std::hash::Hash;

mod canvas;
mod drawer;
mod node;
mod prelude;
mod registry;
mod theme;

fn main() {
    dioxus_devtools::connect_subsecond();
    env_logger::init();

    #[cfg(target_os = "linux")]
    gtk::init().unwrap();

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
    background_visible: bool,
    drawer: Drawer,
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

impl Tab {
    pub const NEW_TAB_NAME: &'static str = "Unnamed desktop";
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
                name: Tab::NEW_TAB_NAME.to_owned(),
                canvas: canvas::Canvas::default(),
                add_index: 0,
            }],
            active_tab: 0,
            renaming_tab: RenamingTab::None,
            background_visible: true,
            drawer: Drawer {
                visible: true,
                data_items: Vec::new(),
            },
        }
    }

    fn active_canvas(tabs: &mut [Tab], active_tab: usize) -> &mut canvas::Canvas {
        &mut tabs.get_mut(active_tab).unwrap().canvas
    }
}

impl eframe::App for DexState {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        subsecond::call(|| {
            #[cfg(target_os = "linux")]
            gtk::main_iteration_do(false);

            egui::Panel::top("tab_bar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut tab_to_close = None;

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

                                    if ui.button("x").clicked() {
                                        tab_to_close = Some(idx);
                                    }
                                });
                            });
                        },
                    );

                    if let Some(update) = response.update {
                        shift_vec(update.from, update.to, &mut self.tabs);
                    }

                    if let Some(close_idx) = tab_to_close {
                        self.tabs.remove(close_idx);
                        self.active_tab = self.active_tab.saturating_sub(1);

                        if self.tabs.is_empty() {
                            self.tabs.push(Tab {
                                name: Tab::NEW_TAB_NAME.to_owned(),
                                canvas: Canvas::default(),
                                add_index: 0,
                            });
                        }
                    }

                    if ui.button("+").clicked() {
                        let new_tab_idx = self.tabs.len();
                        self.tabs.push(Tab {
                            name: Tab::NEW_TAB_NAME.to_owned(),
                            canvas: canvas::Canvas::default(),
                            add_index: new_tab_idx,
                        });
                        self.active_tab = new_tab_idx;
                        self.renaming_tab = RenamingTab::Newly(self.active_tab);
                    }
                });
            });

            if self.drawer.visible {
                let mut want_center = false;
                let active_canvas = Self::active_canvas(&mut self.tabs, self.active_tab);
                egui::Panel::left("drawer")
                    .exact_size(Drawer::SIZE)
                    .resizable(false)
                    .show_inside(ui, |ui| {
                        self.drawer.draw(
                            ui,
                            active_canvas,
                            &mut self.registry,
                            &mut self.background_visible,
                            &mut want_center,
                        );
                    });

                if want_center {
                    active_canvas.view_state.reset_offset();
                }
            }

            egui::CentralPanel::default().show_inside(ui, |ui| {
                Self::active_canvas(&mut self.tabs, self.active_tab).draw(
                    ui,
                    &mut self.registry,
                    self.background_visible,
                );
            });

            let drawer_button_text = if self.drawer.visible { "⏴" } else { "⏵" };
            egui::Area::new(Id::new("drawer_handle"))
                .fixed_pos(Pos2 {
                    x: if self.drawer.visible {
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
                        self.drawer.visible = !self.drawer.visible;
                    }
                });
        });
    }
}
