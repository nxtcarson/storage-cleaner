use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::ai::{self, AiSuggestion, AiVerdict};
use crate::config::Config;
use crate::delete;
use crate::scanner::{scan_drive, ScanResult, ScanState};
use crate::snapshot::Snapshot;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubView {
    Overview,
    ByExtension,
    ByFolder,
    ByCategory,
    LargestFiles,
    StaleFiles,
    AiSuggestions,
}

pub struct DiskAnalysisTab {
    drives: Vec<PathBuf>,
    selected_drive: Option<usize>,
    result: Option<ScanResult>,
    scan_state: Arc<Mutex<ScanState>>,
    scan_thread: Option<thread::JoinHandle<Option<ScanResult>>>,
    sub_view: SubView,
    ai_suggestions: HashMap<String, AiSuggestion>,
    ai_loading: bool,
    ai_thread: Option<thread::JoinHandle<Result<Vec<AiSuggestion>, String>>>,
    selected_for_delete: HashSet<usize>,
    error_message: Option<String>,
}

impl Default for DiskAnalysisTab {
    fn default() -> Self {
        let drives = crate::scanner::get_drives();
        let selected_drive = (!drives.is_empty()).then_some(0);
        Self {
            drives,
            selected_drive,
            result: None,
            scan_state: Arc::new(Mutex::new(ScanState::default())),
            scan_thread: None,
            sub_view: SubView::Overview,
            ai_suggestions: HashMap::new(),
            ai_loading: false,
            ai_thread: None,
            selected_for_delete: HashSet::new(),
            error_message: None,
        }
    }
}

impl DiskAnalysisTab {
    fn get_drives(&mut self) -> &[PathBuf] {
        if self.drives.is_empty() {
            self.drives = crate::scanner::get_drives();
        }
        &self.drives
    }

    fn trigger_ai_analysis(&mut self, config: &Config) {
        if config.openai_api_key.is_empty() || self.ai_loading {
            return;
        }
        let Some(ref result) = self.result else { return };
        let entries: Vec<(String, u64)> = result
            .largest_files
            .iter()
            .take(50)
            .map(|e| (e.path.display().to_string(), e.size_bytes))
            .collect();
        if entries.is_empty() {
            return;
        }
        let key = config.openai_api_key.clone();
        let model = config.model().to_string();
        self.ai_loading = true;
        self.ai_thread = Some(thread::spawn(move || ai::analyze_files(&key, &model, &entries)));
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
            egui::ComboBox::from_id_salt("disk_drive_combo")
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

            let sel = self.selected_drive;
            let can_scan = sel.is_some()
                && self.scan_thread.is_none()
                && self.get_drives().get(sel.unwrap_or(0)).is_some();

            if ui.add_enabled(can_scan, egui::Button::new("Scan")).clicked() {
                let idx = self.selected_drive.unwrap_or(0);
                let root = self.get_drives().get(idx).cloned().unwrap();
                let state = Arc::clone(&self.scan_state);
                *state.lock().unwrap() = ScanState::default();
                self.result = None;
                self.ai_suggestions.clear();
                self.selected_for_delete.clear();
                let handle = thread::spawn(move || scan_drive(&root, state));
                self.scan_thread = Some(handle);
            }

            if let Some(handle) = self.scan_thread.take() {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(Some(r)) => {
                            self.result = Some(r);
                            self.trigger_ai_analysis(config);
                        }
                        Ok(None) => {
                            self.error_message = Some("Scan failed".to_string());
                        }
                        Err(_) => {
                            self.error_message = Some("Scan thread panicked".to_string());
                        }
                    }
                } else {
                    self.scan_thread = Some(handle);
                }
            }

            if is_scanning {
                let s = self.scan_state.lock().unwrap();
                ui.spinner();
                ui.label(format!("Scanning... {} files", s.files_scanned));
            } else if let Some(ref r) = self.result {
                ui.label(format!(
                    "{} total, {} largest",
                    format_size(r.total_bytes),
                    r.largest_files.len()
                ));
            }
        });

        if let Some(ref msg) = self.error_message {
            ui.colored_label(egui::Color32::RED, msg);
            if ui.button("Dismiss").clicked() {
                self.error_message = None;
            }
        }

        if let Some(handle) = self.ai_thread.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok(Ok(suggestions)) => {
                        for s in suggestions {
                            self.ai_suggestions
                                .insert(s.path.clone(), s);
                        }
                    }
                    Ok(Err(e)) => {
                        self.error_message = Some(format!("AI error: {}", e));
                    }
                    Err(_) => {
                        self.error_message = Some("AI request failed".to_string());
                    }
                }
                self.ai_loading = false;
            } else {
                self.ai_thread = Some(handle);
            }
        }
        if self.ai_loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        let result = match &self.result {
            Some(r) => r.clone(),
            None => {
                ui.add_space(16.0);
                ui.label("Select a drive and click Scan to analyze disk usage.");
                return;
            }
        };

        ui.horizontal(|ui| {
            let labels = [
                ("Overview", SubView::Overview),
                ("By Extension", SubView::ByExtension),
                ("By Folder", SubView::ByFolder),
                ("By Category", SubView::ByCategory),
                ("Largest Files", SubView::LargestFiles),
                ("Stale Files", SubView::StaleFiles),
                ("AI Suggestions", SubView::AiSuggestions),
            ];
            for (label, v) in labels {
                if ui
                    .selectable_label(self.sub_view == v, label)
                    .clicked()
                {
                    self.sub_view = v;
                }
            }
        });

        if ui.button("Save Snapshot").clicked() {
            let verdicts: HashMap<String, AiVerdict> = self
                .ai_suggestions
                .iter()
                .map(|(k, v)| (k.clone(), v.verdict.clone()))
                .collect();
            let snap = Snapshot::from_scan_result(&result, &verdicts);
            match snap.save() {
                Ok(p) => self.error_message = Some(format!("Saved to {}", p.display())),
                Err(e) => self.error_message = Some(e),
            }
        }

        ui.separator();

        match self.sub_view {
            SubView::Overview => self.ui_overview(ui, &result),
            SubView::ByExtension => self.ui_by_extension(ui, &result),
            SubView::ByFolder => self.ui_by_folder(ui, &result),
            SubView::ByCategory => self.ui_by_category(ui, &result),
            SubView::LargestFiles => self.ui_largest_files(ui, &result),
            SubView::StaleFiles => self.ui_stale_files(ui, &result),
            SubView::AiSuggestions => self.ui_ai_suggestions(ui, &result, config),
        }
    }

    fn ui_overview(&self, ui: &mut egui::Ui, result: &ScanResult) {
        ui.label(format!("Total: {}", format_size(result.total_bytes)));
        ui.label(format!("Largest files: {}", result.largest_files.len()));
        ui.label(format!("Stale files (6+ months): {}", result.stale_files.len()));
        if self.ai_loading {
            ui.label("AI analysis in progress...");
        } else if !self.ai_suggestions.is_empty() {
            let safe = self
                .ai_suggestions
                .values()
                .filter(|s| s.verdict == AiVerdict::SafeToDelete)
                .count();
            ui.label(format!("{} files marked safe to delete by AI", safe));
        }
    }

    fn ui_by_extension(&self, ui: &mut egui::Ui, result: &ScanResult) {
        egui::ScrollArea::vertical().show_rows(
            ui,
            22.0,
            result.by_extension.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(s) = result.by_extension.get(i) {
                        ui.horizontal(|ui| {
                            ui.label(format!(".{}", s.extension));
                            ui.label(format_size(s.total_bytes));
                            ui.label(format!("{} files", s.file_count));
                        });
                    }
                }
            },
        );
    }

    fn ui_by_folder(&self, ui: &mut egui::Ui, result: &ScanResult) {
        egui::ScrollArea::vertical().show_rows(
            ui,
            22.0,
            result.by_folder.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(s) = result.by_folder.get(i) {
                        ui.horizontal(|ui| {
                            ui.label(s.path.display().to_string());
                            ui.label(format_size(s.total_bytes));
                            ui.label(format!("{} files", s.file_count));
                        });
                    }
                }
            },
        );
    }

    fn ui_by_category(&self, ui: &mut egui::Ui, result: &ScanResult) {
        egui::ScrollArea::vertical().show_rows(
            ui,
            22.0,
            result.by_category.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(s) = result.by_category.get(i) {
                        ui.horizontal(|ui| {
                            ui.label(s.category.as_str());
                            ui.label(format_size(s.total_bytes));
                            ui.label(format!("{} files", s.file_count));
                        });
                    }
                }
            },
        );
    }

    fn ui_stale_files(&mut self, ui: &mut egui::Ui, result: &ScanResult) {
        ui.label("Files not modified in 6+ months (top 500 by size):");
        egui::ScrollArea::vertical().show_rows(
            ui,
            24.0,
            result.stale_files.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(entry) = result.stale_files.get(i) {
                        ui.horizontal(|ui| {
                            ui.label(entry.path.display().to_string());
                            ui.label(format_size(entry.size_bytes));
                            ui.label(
                                entry
                                    .last_modified
                                    .map(|t| t.format("%Y-%m-%d").to_string())
                                    .unwrap_or_else(|| "Unknown".to_string()),
                            );
                        });
                    }
                }
            },
        );
    }

    fn ui_largest_files(&mut self, ui: &mut egui::Ui, result: &ScanResult) {
        if ui
            .add_enabled(!self.selected_for_delete.is_empty(), egui::Button::new("Delete selected"))
            .clicked()
        {
            let mut indices: Vec<usize> = self.selected_for_delete.iter().copied().collect();
            indices.sort_by(|a, b| b.cmp(a));
            let to_delete: Vec<PathBuf> = indices
                .iter()
                .filter_map(|&i| result.largest_files.get(i).map(|e| e.path.clone()))
                .collect();
            match delete::delete_paths(&to_delete) {
                Ok(()) => {
                    self.selected_for_delete.clear();
                    self.error_message = Some("Deleted. Rescan to refresh.".to_string());
                }
                Err(e) => self.error_message = Some(e),
            }
        }

        egui::ScrollArea::vertical().show_rows(
            ui,
            24.0,
            result.largest_files.len().max(1),
            |ui, row_range| {
                for i in row_range {
                    if let Some(entry) = result.largest_files.get(i) {
                        ui.horizontal(|ui| {
                            let mut checked = self.selected_for_delete.contains(&i);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    self.selected_for_delete.insert(i);
                                } else {
                                    self.selected_for_delete.remove(&i);
                                }
                            }
                            ui.label(entry.path.display().to_string());
                            ui.label(format_size(entry.size_bytes));
                            if let Some(s) = self.ai_suggestions.get(&entry.path.display().to_string())
                            {
                                let color = match s.verdict {
                                    AiVerdict::SafeToDelete => egui::Color32::GREEN,
                                    AiVerdict::Review => egui::Color32::GOLD,
                                    AiVerdict::Keep => egui::Color32::GRAY,
                                };
                                ui.colored_label(color, format!("{:?}", s.verdict));
                            }
                        });
                    }
                }
            },
        );
    }

    fn ui_ai_suggestions(
        &mut self,
        ui: &mut egui::Ui,
        result: &ScanResult,
        config: &Config,
    ) {
        if self.ai_loading {
            ui.spinner();
            ui.label("Analyzing files with AI...");
            return;
        }

        if self.ai_suggestions.is_empty() {
            if config.openai_api_key.is_empty() {
                ui.label("Add OpenAI API key in Settings to enable AI suggestions.");
            } else {
                ui.label("AI analysis will run automatically after scan.");
            }
            return;
        }

        let safe: Vec<_> = self
            .ai_suggestions
            .values()
            .filter(|s| s.verdict == AiVerdict::SafeToDelete)
            .collect();
        let review: Vec<_> = self
            .ai_suggestions
            .values()
            .filter(|s| s.verdict == AiVerdict::Review)
            .collect();

        ui.label(format!("Safe to delete: {}", safe.len()));
        ui.label(format!("Review: {}", review.len()));

        if ui
            .add_enabled(!self.selected_for_delete.is_empty(), egui::Button::new("Delete selected"))
            .clicked()
        {
            let mut indices: Vec<usize> = self.selected_for_delete.iter().copied().collect();
            indices.sort_by(|a, b| b.cmp(a));
            let to_delete: Vec<PathBuf> = indices
                .iter()
                .filter_map(|&i| result.largest_files.get(i).map(|e| e.path.clone()))
                .collect();
            match delete::delete_paths(&to_delete) {
                Ok(()) => {
                    self.selected_for_delete.clear();
                    self.error_message = Some("Deleted. Rescan to refresh.".to_string());
                }
                Err(e) => self.error_message = Some(e),
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.collapsing("Safe to delete".to_string(), |ui| {
                for s in self.ai_suggestions.values().filter(|s| s.verdict == AiVerdict::SafeToDelete) {
                    if let Some((idx, _)) = result
                        .largest_files
                        .iter()
                        .enumerate()
                        .find(|(_, e)| e.path.display().to_string() == s.path)
                    {
                        ui.horizontal(|ui| {
                            let mut checked = self.selected_for_delete.contains(&idx);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    self.selected_for_delete.insert(idx);
                                } else {
                                    self.selected_for_delete.remove(&idx);
                                }
                            }
                            ui.label(&s.path);
                            ui.label(&s.reason);
                        });
                    }
                }
            });
            ui.collapsing("Review".to_string(), |ui| {
                for s in self.ai_suggestions.values().filter(|s| s.verdict == AiVerdict::Review) {
                    ui.horizontal(|ui| {
                        ui.label(&s.path);
                        ui.label(&s.reason);
                    });
                }
            });
        });
    }
}
