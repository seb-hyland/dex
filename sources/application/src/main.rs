use dex_core::prelude::*;
use dex_nodes::layouts::desktops::Desktops;
use eframe::egui;

struct App {
    workspace: Workspace,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint(); // Request the next frame to be drawn immediately

        let draw_area = ui.max_rect();
        self.workspace.draw_frame(ui, draw_area);
    }
}

fn build_workspace() -> Workspace {
    Desktops::new_workspace()
}

fn main() -> eframe::Result {
    // Initialise Python on the main thread.
    dex_nodes::scripting::init_python();

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
            dex_nodes::primitives::image::install_image_support(&cc.egui_ctx);
            dex_nodes::fonts::install_fonts(&cc.egui_ctx);
            Ok(Box::new(App {
                workspace: build_workspace(),
            }) as Box<dyn eframe::App>)
        }),
    )
}
