use eframe::egui;

use crate::config::Config;
use crate::ui;

pub struct StorageCleanerApp {
    disk_analysis: ui::disk_analysis::DiskAnalysisTab,
    unused_apps: ui::unused_apps::UnusedAppsTab,
    quick_clean: ui::quick_clean::QuickCleanTab,
    config: Config,
    selected_tab: usize,
}

impl Default for StorageCleanerApp {
    fn default() -> Self {
        Self {
            disk_analysis: ui::disk_analysis::DiskAnalysisTab::default(),
            unused_apps: ui::unused_apps::UnusedAppsTab::default(),
            quick_clean: ui::quick_clean::QuickCleanTab::default(),
            config: Config::load(),
            selected_tab: 0,
        }
    }
}

impl StorageCleanerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for StorageCleanerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Storage Cleaner");

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, 0, "Disk Analysis");
                ui.selectable_value(&mut self.selected_tab, 1, "Unused Apps");
                ui.selectable_value(&mut self.selected_tab, 2, "Quick Clean");
                ui.selectable_value(&mut self.selected_tab, 3, "Settings");
            });

            ui.separator();

            match self.selected_tab {
                0 => self.disk_analysis.ui(ui, ctx, &self.config),
                1 => self.unused_apps.ui(ui, ctx),
                2 => self.quick_clean.ui(ui, ctx),
                3 => {
                    ui.label("OpenAI API Key (stored locally):");
                    let mut key = self.config.openai_api_key.clone();
                    if ui.text_edit_singleline(&mut key).changed() {
                        self.config.openai_api_key = key;
                        self.config.save();
                    }
                    ui.label("Model (default: gpt-5-nano):");
                    let mut model = self.config.openai_model.clone();
                    if ui.text_edit_singleline(&mut model).changed() {
                        self.config.openai_model = model;
                        self.config.save();
                    }
                    ui.add_space(8.0);
                    ui.label("Config saved to:");
                    ui.monospace(Config::config_path().display().to_string());
                }
                _ => {}
            }
        });
    }
}
