mod app_state;
mod token_manager; // Declare the new module

use app_state::AppState;
use eframe::NativeOptions;
use egui::ViewportBuilder;

// Use #[tokio::main] to manage the async runtime
#[tokio::main]
async fn main() {
    let opts = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NovaCibes Editor",
        opts,
        Box::new(|_cc| Ok(Box::new(AppState::new()))), // Use the new AppState
    )
    .unwrap();
}