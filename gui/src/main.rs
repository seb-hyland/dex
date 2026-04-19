use crate::actions::DoActionContext;
use crate::prelude::*;
use crate::tab::TabState;
use crate::{drawer::Drawer, registry::Registry, theme::LIGHT_THEME};

use eframe::CreationContext;
use egui::{FontData, FontFamily, Visuals};
use egui_extras::install_image_loaders;

mod actions;
mod canvas;
mod components;
mod drawer;
mod node;
mod prelude;
mod registry;
mod tab;
mod text;
mod theme;
mod types;

#[derive(Default)]
struct Main {
    registry: Registry,
    active: Situation,
    action_queue: Actions,
    situations: Vec<Situation>,
}

#[derive(Default, Clone)]
struct Situation {
    tab_state: TabState,
    drawer: Drawer,
}

impl Situation {
    pub fn active_canvas(&mut self) -> Option<&mut canvas::Canvas> {
        self.tab_state.active_canvas()
    }
}

#[global_allocator]
static GLOBAL: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

fn main() {
    dioxus_devtools::connect_subsecond();
    #[cfg(debug_assertions)]
    env_logger::init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            install_image_loaders(&cc.egui_ctx);
            initialize_fonts(cc);
            cc.egui_ctx.set_visuals(Visuals::from(LIGHT_THEME));

            Ok(Box::new(Main::default()))
        }),
    )
    .unwrap();
}

fn initialize_fonts(cc: &CreationContext) {
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
}

impl eframe::App for Main {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        subsecond::call(|| {
            let active_situation = &mut self.active;

            active_situation
                .tab_state
                .draw_fluent(ui, &mut self.action_queue);

            match active_situation.tab_state.active_canvas() {
                None => {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label("No desktops have been created!");
                            if ui.button("Create new...").clicked() {
                                self.action_queue.push(tab::NewTab {});
                            }
                        });
                    });
                }
                Some(canvas) => {
                    active_situation.drawer.draw_fluent(
                        ui,
                        &mut self.action_queue,
                        canvas,
                        &self.registry,
                    );
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        canvas.sync_placing_node(ui);
                        canvas.draw_fluent(ui, &mut self.action_queue, &mut self.registry);
                    });
                }
            }

            if self.action_queue.is_dirty() {
                let mut context = DoActionContext {
                    situation: active_situation,
                    registry: &mut self.registry,
                    frame_time: ui.time(),
                };
                self.action_queue.do_all(&mut context);

                self.situations.push(self.active.clone());
            }

            // let stats = GLOBAL.stats();
            // println!(
            //     "Net allocation: {}",
            //     stats.bytes_allocated - stats.bytes_deallocated
            // );
        });
    }
}
