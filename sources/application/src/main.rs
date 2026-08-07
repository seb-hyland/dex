use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::CanvasLayout;
use eframe::egui;
use egui::Panel;

struct App {
    workspace: Workspace,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint(); // Request the next frame to be drawn immediately

        let sidebar_area = Panel::left("sidebar")
            .min_size(100.0)
            .max_size(600.0)
            .default_size(200.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.take_available_space();
                ui.max_rect()
            })
            .inner;

        let full = ui.max_rect();
        let draw_area = egui::Rect::from_min_max(
            egui::pos2(sidebar_area.right(), full.top()),
            full.right_bottom(),
        );

        self.workspace.draw_frame(ui, draw_area, sidebar_area);
    }
}

fn build_workspace() -> Workspace {
    Workspace::new_with_root(Box::new(CanvasLayout::default()))
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "dex",
        native_options,
        Box::new(|cc| {
            // Always run in light mode, ignoring system theme changes
            cc.egui_ctx
                .options_mut(|opt| opt.theme_preference = egui::ThemePreference::Light);
            Ok(Box::new(App {
                workspace: build_workspace(),
            }) as Box<dyn eframe::App>)
        }),
    )
}
