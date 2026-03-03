#![cfg_attr(windows, windows_subsystem = "windows")]

mod ai;
mod app;
mod config;
mod delete;
mod prefetch;
mod scanner;
mod service;
mod service_ipc;
mod snapshot;
mod ui;

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, storage_cleaner_service_main);

#[cfg(windows)]
fn storage_cleaner_service_main(_arguments: Vec<std::ffi::OsString>) {
    service::windows::run_pipe_server();
}

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            match args[1].as_str() {
                "--service" => {
                    if windows_service::service_dispatcher::start(
                        service::windows::SERVICE_NAME,
                        ffi_service_main,
                    )
                    .is_err()
                    {
                        service::windows::run_pipe_server();
                    }
                    return Ok(());
                }
                "--install-service" => {
                    if let Err(e) = service::windows::install_service() {
                        eprintln!("Install failed: {}", e);
                        std::process::exit(1);
                    }
                    println!("Storage Cleaner service installed. Start with: sc start StorageCleanerScan");
                    return Ok(());
                }
                "--uninstall-service" => {
                    if let Err(e) = service::windows::uninstall_service() {
                        eprintln!("Uninstall failed: {}", e);
                        std::process::exit(1);
                    }
                    println!("Storage Cleaner service uninstalled.");
                    return Ok(());
                }
                _ => {}
            }
        }
    }

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
