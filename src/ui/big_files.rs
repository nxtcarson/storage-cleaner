use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::ai;
use crate::config::Config;
use crate::delete;
use crate::scanner::{scan_big_files, BigFileEntry, ScanState};

pub struct BigFilesTab {
    pub drives: Vec<PathBuf>,
    pub selected_drive: Option<usize>,
    pub min_size_mb: u64,
    pub results: Vec<BigFileEntry>,
    pub selected: std::collections::HashSet<usize>,
    pub scan_state: Arc<Mutex<ScanState>>,
    pub scan_thread: Option<thread::JoinHandle<Vec<BigFileEntry>>>,
    pub error_message: Option<String>,
    pub ai_response: Option<String>,
    pub ai_loading: bool,
    pub ai_thread: Option<thread::JoinHandle<Result<String, String>>>,
}

impl Default for BigFilesTab {
    fn default() -> Self {
        let drives = crate::scanner::get_drives();
        let selected_drive = (!drives.is_empty()).then_some(0);
        Self {
            drives,
            selected_drive,
            min_size_mb: 50,
            results: Vec::new(),
            selected: std::collections::HashSet::new(),
            scan_state: Arc::new(Mutex::new(ScanState::default())),
            scan_thread: None,
            error_message: None,
            ai_response: None,
            ai_loading: false,
            ai_thread: None,
        }
    }
}

impl BigFilesTab {
    fn get_drives(&mut self) -> &[PathBuf] {
        if self.drives.is_empty() {
            self.drives = crate::scanner::get_drives();
        }
        &self.drives
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, config: &Config) {
        let state = self.scan_state.lock().unwrap();
        let is_scanning = !state.is_done;
        drop(state);
        if is_scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        ui.horizontal(|ui| {
            let drives = self.get_drives().to_vec();
            ui.label("Drive:");
            egui::ComboBox::from_id_salt("drive_combo")
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
            ui.add(
                egui::DragValue::new(&mut self.min_size_mb)
                    .prefix("Min size: ")
                    .suffix(" MB")
                    .clamp_range(1..=10_000),
            );
        });

        ui.horizontal(|ui| {
            let sel = self.selected_drive;
            let can_scan = sel.is_some()
                && self.scan_thread.is_none()
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
                let min_mb = self.min_size_mb;
                let state = Arc::clone(&self.scan_state);
                *state.lock().unwrap() = ScanState::default();
                self.results.clear();
                self.selected.clear();
                let handle = thread::spawn(move || {
                    let results = scan_big_files(&root, min_mb, state);
                    results
                });
                self.scan_thread = Some(handle);
            }

            if let Some(handle) = self.scan_thread.take() {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(results) => {
                            self.results = results;
                        }
                        Err(_) => {
                            self.error_message = Some("Scan thread panicked".to_string());
                        }
                    }
                } else {
                    self.scan_thread = Some(handle);
                }
            }

            let state = self.scan_state.lock().unwrap();
            if state.is_done && self.scan_thread.is_none() {
                ui.label(format!("Done. {} large files found.", self.results.len()));
            } else if !state.is_done {
                ui.spinner();
                ui.label(format!("Scanning... {} files", state.files_scanned));
            }
            if let Some(ref err) = state.error {
                ui.colored_label(egui::Color32::RED, err);
            }
        });

        if let Some(ref msg) = self.error_message {
            ui.colored_label(egui::Color32::RED, msg);
            if ui.button("Dismiss").clicked() {
                self.error_message = None;
            }
        }

        ui.add_space(8.0);

        let one_selected = self.selected.len() == 1;
        let sel_idx = self.selected.iter().next().copied();
        if ui
            .add_enabled(one_selected && !self.ai_loading, egui::Button::new("Ask AI"))
            .clicked()
        {
            if let Some(idx) = sel_idx {
                if let Some(entry) = self.results.get(idx) {
                    let path = entry.path.display().to_string();
                    let size = entry.size_bytes;
                    let key = config.openai_api_key.clone();
                    let model = config.model().to_string();
                    self.ai_loading = true;
                    self.ai_response = None;
                    self.ai_thread = Some(thread::spawn(move || ai::ask_about_file(&key, &model, &path, size)));
                }
            }
        }
        if ui
            .add_enabled(!self.results.is_empty() && !self.ai_loading, egui::Button::new("AI Suggest"))
            .clicked()
        {
            let entries: Vec<(String, u64)> = self
                .results
                .iter()
                .take(50)
                .map(|e| (e.path.display().to_string(), e.size_bytes))
                .collect();
            let key = config.openai_api_key.clone();
            let model = config.model().to_string();
            self.ai_loading = true;
            self.ai_response = None;
            self.ai_thread = Some(thread::spawn(move || ai::suggest_deletions(&key, &model, &entries)));
        }
        if self.ai_loading {
            if let Some(handle) = self.ai_thread.take() {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(Ok(msg)) => self.ai_response = Some(msg),
                        Ok(Err(e)) => self.ai_response = Some(format!("Error: {}", e)),
                        Err(_) => self.ai_response = Some("Request failed".to_string()),
                    }
                    self.ai_loading = false;
                } else {
                    self.ai_thread = Some(handle);
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(msg) = self.ai_response.as_ref() {
            ui.add_space(4.0);
            let mut dismiss = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("AI:");
                ui.label(msg);
                dismiss = ui.button("Dismiss").clicked();
            });
            if dismiss {
                self.ai_response = None;
            }
        }
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
                Err(e) => {
                    self.error_message = Some(e);
                }
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
                            ui.label(path_str);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(size_str);
                                },
                            );
                        });
                    }
                }
            },
        );
    }
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
