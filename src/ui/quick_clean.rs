use eframe::egui;
use std::path::PathBuf;
use std::thread::JoinHandle;

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

struct CleanTarget {
    path: PathBuf,
    label: String,
    size: Option<u64>,
}

impl CleanTarget {
    fn new(path: PathBuf, label: &str) -> Self {
        Self {
            path,
            label: label.to_string(),
            size: None,
        }
    }
}

pub struct QuickCleanTab {
    targets: Vec<CleanTarget>,
    sizes_done: bool,
    error_message: Option<String>,
    clean_thread: Option<JoinHandle<Result<(), String>>>,
    cleaning_label: Option<String>,
}

impl Default for QuickCleanTab {
    fn default() -> Self {
        let mut targets = Vec::new();
        #[cfg(windows)]
        {
            if let Ok(tmp) = std::env::var("TEMP") {
                targets.push(CleanTarget::new(PathBuf::from(&tmp), "User Temp (%TEMP%)"));
            }
            if let Ok(tmp) = std::env::var("TMP") {
                let p = PathBuf::from(&tmp);
                if !targets.iter().any(|t| t.path == p) {
                    targets.push(CleanTarget::new(p, "User TMP (%TMP%)"));
                }
            }
            targets.push(CleanTarget::new(
                PathBuf::from("C:\\Windows\\Temp"),
                "Windows Temp (may need admin)",
            ));
            targets.push(CleanTarget::new(
                PathBuf::from("C:\\Windows\\SoftwareDistribution\\Download"),
                "Windows Update cache (requires admin)",
            ));
        }
        #[cfg(not(windows))]
        {
            targets.push(CleanTarget::new(
                PathBuf::from("/tmp"),
                "System temp",
            ));
        }

        Self {
            targets,
            sizes_done: false,
            error_message: None,
            clean_thread: None,
            cleaning_label: None,
        }
    }
}

impl QuickCleanTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let is_cleaning = self.clean_thread.is_some();

        // Poll background thread
        if let Some(handle) = self.clean_thread.take() {
            if handle.is_finished() {
                if let Ok(result) = handle.join() {
                    match result {
                        Ok(()) => {
                            self.sizes_done = false;
                        }
                        Err(e) => {
                            self.error_message = Some(e);
                        }
                    }
                }
                self.cleaning_label = None;
            } else {
                self.clean_thread = Some(handle);
                ctx.request_repaint();
            }
        }

        ui.label("Quick clean deletes contents of these folders. Sizes are computed on demand.");

        if !self.sizes_done && !is_cleaning && ui.button("Compute sizes").clicked() {
            for t in &mut self.targets {
                if t.path.exists() {
                    t.size = Some(dir_size(&t.path));
                }
            }
            self.sizes_done = true;
        }

        if let Some(ref msg) = self.error_message {
            ui.colored_label(egui::Color32::RED, msg);
            if ui.button("Dismiss").clicked() {
                self.error_message = None;
            }
        }

        if let Some(ref label) = self.cleaning_label {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Clearing {}...", label));
            });
        }

        ui.add_space(8.0);

        let mut clicked_path: Option<(PathBuf, String)> = None;

        for target in &mut self.targets {
            ui.horizontal(|ui| {
                ui.label(&target.label);
                ui.label(target.path.display().to_string());
                if let Some(s) = target.size {
                    ui.label(format_size(s));
                } else if target.path.exists() {
                    ui.label("(click Compute sizes)");
                } else {
                    ui.label("(not found)");
                }
                let btn = ui.add_enabled(
                    !is_cleaning && target.path.exists(),
                    egui::Button::new("Clear"),
                );
                if btn.clicked() {
                    clicked_path = Some((target.path.clone(), target.label.clone()));
                }
            });
        }

        if let Some((path, label)) = clicked_path {
            self.cleaning_label = Some(label);
            let handle = std::thread::spawn(move || clear_dir_contents(&path));
            self.clean_thread = Some(handle);
            ctx.request_repaint();
        }

        ui.add_space(16.0);
        ui.label("Clearing empties the folder contents. Files go to Recycle Bin when possible.");
    }
}

fn clear_dir_contents(path: &std::path::Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err("Not a directory".to_string());
    }
    let entries: Vec<_> = std::fs::read_dir(path).map_err(|e| e.to_string())?.collect();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            trash::delete(&p).map_err(|e| e.to_string())?;
        } else {
            trash::delete(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
