//! Rust Studio Tycoon — desktop entry point.

mod app;
mod screens;
mod theme;
mod widgets;

use std::sync::Arc;

use eframe::egui;
use studio_core::data::ContentLibrary;
use studio_core::paths::AppPaths;
use studio_core::settings::Settings;

use crate::app::App;

/// Thin adapter so the real application stays independent of `eframe::Frame`
/// (which keeps it testable without a window).
struct Shell(App);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.0.on_exit();
    }
}

fn main() -> eframe::Result {
    let paths = AppPaths::default_location();
    let settings = Settings::load(&paths);
    let content = Arc::new(ContentLibrary::load(Some(&paths.content_dir())));

    let dev_screen = std::env::args().skip_while(|a| a != "--dev-screen").nth(1);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rust Studio Tycoon")
            .with_inner_size([1360.0, 840.0])
            .with_min_inner_size([1040.0, 660.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Studio Tycoon",
        options,
        Box::new(move |cc| {
            let mut app = App::new(paths, settings, content);
            if let Some(spec) = &dev_screen {
                app.dev_jump(spec);
            }
            crate::theme::apply(&cc.egui_ctx, &app.settings);
            Ok(Box::new(Shell(app)))
        }),
    )
}
