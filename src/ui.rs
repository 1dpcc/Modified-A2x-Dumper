use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use log::{error, info, Level, Log, Metadata, Record};

use crate::core::{execute_dump, DumpConfig};
use crate::parser;

// ── In-app logger ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LogLine {
    pub level: Level,
    pub message: String,
}


pub type LogBuffer = Arc<Mutex<Vec<LogLine>>>;

pub struct UiLogger {
    buffer: LogBuffer,
}

impl UiLogger {
    pub fn new(buffer: LogBuffer) -> Self {
        Self { buffer }
    }
}

impl Log for UiLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let line = LogLine {
                level: record.level(),
                message: format!("[{}] {}", record.level(), record.args()),
            };
            if let Ok(mut buf) = self.buffer.lock() {
                buf.push(line);
                if buf.len() > 2_000 {
                    buf.drain(..500);
                }
            }
        }
    }

    fn flush(&self) {}
}


pub struct UiApp {
    output_path: String,
    process_name: String,
    process_name_shared: Arc<Mutex<String>>,
    indent_size: i32,
    file_types: FileTypesState,
    selected_tab: Tab,
    cs2_running: bool,
    cs2_status_receiver: mpsc::Receiver<bool>,
    selected_offset_file: Option<PathBuf>,
    update_status: String,
    update_details: String,
    auto_update_on_dump: bool,
    log_buffer: LogBuffer,
    console_auto_scroll: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Dumper,
    Update,
    Console,
    Credits,
}

#[derive(Clone)]
struct FileTypesState {
    cs: bool,
    hpp: bool,
    json: bool,
    rs: bool,
    zig: bool,
}

impl Default for FileTypesState {
    fn default() -> Self {
        Self { cs: true, hpp: true, json: true, rs: true, zig: true }
    }
}

impl UiApp {
    pub fn new_with_logger() -> Self {
        let log_buffer: LogBuffer = Arc::new(Mutex::new(Vec::new()));

        let logger = Box::new(UiLogger::new(Arc::clone(&log_buffer)));
        let _ = log::set_boxed_logger(logger);
        log::set_max_level(log::LevelFilter::Info);

        let process_name = "cs2.exe".to_string();
        let process_name_shared = Arc::new(Mutex::new(process_name.clone()));
        let (tx, rx) = mpsc::channel();
        let thread_process_name = Arc::clone(&process_name_shared);

        thread::spawn(move || loop {
            let current_name = {
                let guard = thread_process_name.lock().unwrap();
                guard.clone()
            };
            let running = check_cs2_running_for(&current_name);
            if tx.send(running).is_err() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        });

        Self {
            output_path: "output".to_string(),
            process_name,
            process_name_shared,
            indent_size: 4,
            file_types: FileTypesState::default(),
            selected_tab: Tab::Dumper,
            cs2_running: false,
            cs2_status_receiver: rx,
            selected_offset_file: None,
            update_status: String::new(),
            update_details: String::new(),
            auto_update_on_dump: false,
            log_buffer,
            console_auto_scroll: true,
        }
    }

    fn get_selected_file_types(&self) -> Vec<String> {
        let mut types = Vec::new();
        if self.file_types.cs   { types.push("cs".to_string()); }
        if self.file_types.hpp  { types.push("hpp".to_string()); }
        if self.file_types.json { types.push("json".to_string()); }
        if self.file_types.rs   { types.push("rs".to_string()); }
        if self.file_types.zig  { types.push("zig".to_string()); }
        types
    }
}

fn check_cs2_running_for(process_name: &str) -> bool {
    use memflow::prelude::v1::*;
    #[cfg(windows)]
    {
        match memflow_native::create_os(&OsArgs::default(), LibArc::default()) {
            Ok(mut os) => os.process_by_name(process_name).is_ok(),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = process_name;
        false
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(status) = self.cs2_status_receiver.try_recv() {
            self.cs2_running = status;
        }

        {
            let mut shared = self.process_name_shared.lock().unwrap();
            if *shared != self.process_name {
                *shared = self.process_name.clone();
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("CS2 Dumper");

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, Tab::Dumper,  "Dumper");
                ui.selectable_value(&mut self.selected_tab, Tab::Update,  "Update");
                ui.selectable_value(&mut self.selected_tab, Tab::Console, "Console");
                ui.selectable_value(&mut self.selected_tab, Tab::Credits, "Credits");
            });

            ui.separator();

            match self.selected_tab {
                Tab::Dumper  => egui::ScrollArea::vertical().show(ui, |ui| self.dump_tab_ui(ui)),
                Tab::Update  => egui::ScrollArea::vertical().show(ui, |ui| self.update_tab_ui(ui)),
                Tab::Console => egui::ScrollArea::vertical().show(ui, |ui| self.console_tab_ui(ui)),
                Tab::Credits => egui::ScrollArea::vertical().show(ui, |ui| self.credits_tab_ui(ui)),
            };
        });

        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

impl UiApp {
    fn dump_tab_ui(&mut self, ui: &mut egui::Ui) {
        if self.cs2_running {
            ui.colored_label(egui::Color32::GREEN, "Counter-Strike 2 is running");
        } else {
            ui.colored_label(egui::Color32::RED, "Counter-Strike 2 is NOT running");
        }

        ui.separator();
        ui.label("Output Configuration");
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            ui.label("Output Directory:");
            let browse_width = 70.0 + ui.spacing().item_spacing.x;
            let text_width = ui.available_width() - browse_width;
            ui.add(egui::TextEdit::singleline(&mut self.output_path)
                .desired_width(text_width));
            if ui.button("Browse...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.output_path = path.to_string_lossy().to_string();
                }
            }
        });

        ui.separator();
        ui.label("Process Configuration");
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            ui.label("Process Name:");
            ui.add(egui::TextEdit::singleline(&mut self.process_name)
                .desired_width(f32::INFINITY));
        });

        ui.separator();
        ui.label("File Types to Generate");
        ui.add_space(2.0);

        ui.checkbox(&mut self.file_types.cs,   "C# (.cs)");
        ui.checkbox(&mut self.file_types.hpp,  "C++ Header (.hpp)");
        ui.checkbox(&mut self.file_types.json, "JSON (.json)");
        ui.checkbox(&mut self.file_types.rs,   "Rust (.rs)");
        ui.checkbox(&mut self.file_types.zig,  "Zig (.zig)");

        ui.separator();

        ui.add_enabled_ui(self.cs2_running, |ui| {
            if ui.button("Start Dump").clicked() {
                let config = DumpConfig {
                    connector: None,
                    connector_args: None,
                    file_types: self.get_selected_file_types(),
                    indent_size: self.indent_size as usize,
                    output: PathBuf::from(&self.output_path),
                    process_name: self.process_name.clone(),
                };

                match execute_dump(config) {
                    Ok(_) => {
                        info!("Dump completed successfully");

                        if self.auto_update_on_dump {
                            if let Some(user_file) = &self.selected_offset_file.clone() {
                                let output_dir = PathBuf::from(&self.output_path);
                                match parser::scan_output_directory(&output_dir) {
                                    Ok(dump_offsets) => {
                                        match parser::update_offsets_in_file(user_file, &dump_offsets) {
                                            Ok((summary, _stats)) => {
                                                info!("Auto-update: {}", summary);
                                            }
                                            Err(e) => {
                                                error!("Auto-update failed: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Scan failed: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Dump failed: {}", e);
                    }
                }
            }
        });
    }

    fn update_tab_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Update Offsets");
        ui.separator();

        ui.label("Select your header file to update:");
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            let path_text = self
                .selected_offset_file
                .as_ref()
                .map(|p| p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.to_string_lossy().to_string()))
                .unwrap_or_else(|| "No file selected".to_string());

            ui.label(path_text);

            if ui.button("Browse...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("HPP Files", &["hpp"])
                    .pick_file()
                {
                    self.selected_offset_file = Some(path);
                    self.update_status.clear();
                    self.update_details.clear();
                }
            }
        });

        ui.add_space(4.0);

        let enabled = self.selected_offset_file.is_some();

        ui.add_enabled_ui(enabled, |ui| {
            if ui.button("Update Offsets").clicked() {
                if let Some(user_file) = &self.selected_offset_file.clone() {
                    let output_dir = PathBuf::from(&self.output_path);
                    match parser::scan_output_directory(&output_dir) {
                        Ok(dump_offsets) => {
                            match parser::update_offsets_in_file(user_file, &dump_offsets) {
                                Ok((summary, stats)) => {
                                    self.update_status =
                                        "[SUCCESS] Update complete".to_string();
                                    self.update_details = format!(
                                        "{}\n\nDetails:\nTotal offsets: {}",
                                        summary, stats.total
                                    );
                                    info!("Offsets updated: {}", summary);
                                }
                                Err(e) => {
                                    self.update_status = "[ERROR] Update Failed".to_string();
                                    self.update_details = format!("Error: {}", e);
                                    error!("Update failed: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            self.update_status = "[ERROR] Scan Failed".to_string();
                            self.update_details = format!("Error: {}", e);
                            error!("Scan failed: {}", e);
                        }
                    }
                }
            }
        });

        if !enabled {
            ui.label("Please select a file to enable the Update button");
        }

        ui.separator();
        ui.label("Auto-Update Settings");
        ui.add_space(2.0);

        ui.checkbox(
            &mut self.auto_update_on_dump,
            "Update on Dump - Automatically update offsets after dump",
        );

        if !self.update_status.is_empty() {
            ui.separator();
            ui.strong(&*self.update_status);
            if !self.update_details.is_empty() {
                ui.label(&*self.update_details);
            }
        }
    }

    fn console_tab_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.console_auto_scroll, "Auto-scroll");

            if ui.button("Clear").clicked() {
                if let Ok(mut buf) = self.log_buffer.lock() {
                    buf.clear();
                }
            }
        });

        ui.separator();

        let available = ui.available_size();
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());

        let scroll = egui::ScrollArea::vertical()
            .id_source("console_scroll")
            .max_height(available.y - 4.0)
            .stick_to_bottom(self.console_auto_scroll);

        scroll.show(ui, |ui| {
            let bg_rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(bg_rect, 2.0, egui::Color32::from_rgb(18, 18, 18));

            ui.style_mut().spacing.item_spacing.y = 1.0;

            let lines = self.log_buffer.lock().unwrap().clone();

            if lines.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::from_rgb(100, 100, 100),
                    "  No log output yet.",
                );
            } else {
                for line in &lines {
                    let color = match line.level {
                        Level::Error => egui::Color32::from_rgb(255,  80,  80),
                        Level::Warn  => egui::Color32::from_rgb(255, 200,  50),
                        Level::Info  => egui::Color32::from_rgb(100, 220, 120),
                        Level::Debug => egui::Color32::from_rgb(130, 180, 255),
                        Level::Trace => egui::Color32::from_rgb(160, 160, 160),
                    };

                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&line.message)
                                .font(font_id.clone())
                                .color(color),
                        )
                        .wrap(false),
                    );
                }
            }
        });
    }

    fn credits_tab_ui(&self, ui: &mut egui::Ui) {
        ui.heading("Credits");
        ui.separator();

        ui.add_space(4.0);
        ui.label("Dumper");
        ui.add_space(2.0);
        ui.label("a2x — original CS2 dumper");
        ui.hyperlink_to("github.com/a2x/cs2-dumper", "https://github.com/a2x/cs2-dumper");

        ui.add_space(12.0);

        ui.label("parser, updater, and dumper modifications");
        ui.add_space(2.0);
        ui.label("1dpc / glowie");
        ui.hyperlink_to("github.com/1dpcc/Modified-A2x-Dumper", "https://github.com/1dpcc/Modified-A2x-Dumper");
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);
        ui.weak("This tool is a modified version of a2x's cs2-dumper.");
    }
}


pub fn run_ui() -> Result<(), Box<dyn std::error::Error>> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])   
            .with_title("CS2 Dumper")
            .with_resizable(true),             
        ..Default::default()
    };

    eframe::run_native(
        "CS2 Dumper",
        options,
        Box::new(|_cc| Box::new(UiApp::new_with_logger())),
    )?;

    Ok(())
}
