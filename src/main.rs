mod ai;
mod app;
mod config;
mod delete;
mod prefetch;
mod scanner;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("Storage Cleaner"),
        ..Default::default()
    };
    eframe::run_native(
        "Storage Cleaner",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app::StorageCleanerApp::new(cc)))
        }),
    )
}
