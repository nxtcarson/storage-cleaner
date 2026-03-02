use eframe::egui;
use std::path::PathBuf;
use std::thread;

use crate::delete;
use crate::prefetch;
use crate::scanner::{scan_executables, ExeEntry};

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

pub struct UnusedAppsTab {
    pub drives: Vec<PathBuf>,
    pub selected_drive: Option<usize>,
    pub unused_days: u32,
    pub results: Vec<ExeEntry>,
    pub selected: std::collections::HashSet<usize>,
    pub scan_thread: Option<thread::JoinHandle<Vec<ExeEntry>>>,
    pub error_message: Option<String>,
}

impl Default for UnusedAppsTab {
    fn default() -> Self {
        let drives = crate::scanner::get_drives();
        let selected_drive = (!drives.is_empty()).then_some(0);
        Self {
            drives,
            selected_drive,
            unused_days: 90,
            results: Vec::new(),
            selected: std::collections::HashSet::new(),
            scan_thread: None,
            error_message: None,
        }
    }
}

impl UnusedAppsTab {
    fn get_drives(&mut self) -> &[PathBuf] {
        if self.drives.is_empty() {
            self.drives = crate::scanner::get_drives();
        }
        &self.drives
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let is_scanning = self.scan_thread.is_some();
        if is_scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        ui.horizontal(|ui| {
            let drives = self.get_drives().to_vec();
            ui.label("Drive:");
            egui::ComboBox::from_id_salt("unused_drive_combo")
                .selected_text(
                    self.selected_drive
                        .and_then(|i| drives.get(i))
                        .and_then(|p| p.to_str())
                        .unwrap_or("Select drive"),
                )
                .show_ui(ui, |ui| {
                    for (i, d) in drives.iter().enumerate() {
                        if ui
                            .selectable_label(self.selected_drive == Some(i), d.display().to_string())
                            .clicked()
                        {
                            self.selected_drive = Some(i);
                            ui.close_menu();
                        }
                    }
                });
            ui.label("Consider unused if not run in:");
            ui.add(
                egui::DragValue::new(&mut self.unused_days)
                    .suffix(" days")
                    .clamp_range(1..=365),
            );
        });

        ui.horizontal(|ui| {
            let sel = self.selected_drive;
            let can_scan = sel.is_some()
                && !is_scanning
                && self.get_drives().get(sel.unwrap_or(0)).is_some();
            if ui
                .add_enabled(!self.results.is_empty(), egui::Button::new("Clear results"))
                .clicked()
            {
                self.results.clear();
                self.selected.clear();
            }
            if ui.add_enabled(can_scan, egui::Button::new("Scan")).clicked() {
                let idx = self.selected_drive.unwrap_or(0);
                let root = self.get_drives().get(idx).cloned().unwrap();
                let days = self.unused_days;
                let handle = thread::spawn(move || {
                    #[cfg(windows)]
                    let prefetch_map = prefetch::parse_prefetch_folder(
                        std::path::Path::new("C:\\Windows\\Prefetch"),
                    );
                    #[cfg(not(windows))]
                    let prefetch_map = std::collections::HashMap::new();

                    let mut entries = scan_executables(&root, &prefetch_map);
                    let cutoff = chrono::Utc::now()
                        - chrono::Duration::days(days as i64);
                    entries.retain(|e| {
                        e.last_run.map_or(true, |t| t < cutoff)
                    });
                    entries
                });
                self.results.clear();
                self.selected.clear();
                self.scan_thread = Some(handle);
            }

            if let Some(handle) = self.scan_thread.take() {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(results) => self.results = results,
                        Err(_) => {
                            self.error_message = Some("Scan thread panicked".to_string())
                        }
                    }
                } else {
                    self.scan_thread = Some(handle);
                }
            }

            if is_scanning {
                ui.spinner();
                ui.label("Scanning for executables...");
            } else if !self.results.is_empty() {
                ui.label(format!("{} possibly unused executables", self.results.len()));
            }
        });

        if let Some(ref msg) = self.error_message {
            ui.colored_label(egui::Color32::RED, msg);
            if ui.button("Dismiss").clicked() {
                self.error_message = None;
            }
        }

        ui.add_space(8.0);

        if ui
            .add_enabled(!self.selected.is_empty(), egui::Button::new("Delete selected"))
            .clicked()
        {
            let mut indices: Vec<usize> = self.selected.iter().copied().collect();
            indices.sort_by(|a, b| b.cmp(a));
            let to_delete: Vec<PathBuf> = indices
                .iter()
                .filter_map(|&i| self.results.get(i).map(|e| e.path.clone()))
                .collect();
            match delete::delete_paths(&to_delete) {
                Ok(()) => {
                    for &i in &indices {
                        if i < self.results.len() {
                            self.results.remove(i);
                        }
                    }
                    self.selected.clear();
                }
                Err(e) => self.error_message = Some(e),
            }
        }

        ui.separator();

        egui::ScrollArea::vertical().show_rows(
            ui,
            24.0,
            self.results.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(entry) = self.results.get(i) {
                        ui.horizontal(|ui| {
                            let mut checked = self.selected.contains(&i);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    self.selected.insert(i);
                                } else {
                                    self.selected.remove(&i);
                                }
                            }
                            let path_str = entry.path.display().to_string();
                            let size_str = format_size(entry.size_bytes);
                            let last_run_str = entry
                                .last_run
                                .map(|t| t.format("%Y-%m-%d").to_string())
                                .unwrap_or_else(|| "Never".to_string());
                            ui.label(path_str);
                            ui.label(last_run_str);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| ui.label(size_str),
                            );
                        });
                    }
                }
            },
        );
    }
}
