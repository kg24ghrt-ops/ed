use crate::token_manager::TokenManager;
use egui_code_editor::CodeEditor;
use eframe::{egui, App, Frame};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Serialize)]
struct RunRequest {
    code: String,
}

#[derive(Deserialize)]
struct RunResponse {
    stdout: Option<String>,
    stderr: Option<String>,
}

const API_BASE: &str = "https://novacibes-python-running-api.hf.space";

#[derive(Debug)]
pub struct EditorTab {
    pub path: Option<PathBuf>,
    pub code: String,
    pub modified: bool,
}

impl EditorTab {
    pub fn new_empty() -> Self {
        Self {
            path: None,
            code: String::new(),
            modified: false,
        }
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "Untitled".to_owned());
        if self.modified {
            format!("{} *", name)
        } else {
            name
        }
    }
}
pub struct AppState {
    pub open_files: Vec<EditorTab>,
    pub active_tab: usize,
    pub output_text: String,
    pub status_message: String,
    pub running: bool,
    pub api_token: Option<String>, // Store token in memory after loading from keyring
    pub token_prompt_open: bool,
    pub temp_token: String, // Used only for the input field in the UI
    pub run_all_tabs: bool,
    pub rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl AppState {
    pub fn new() -> Self {
        // Attempt to load the token from the secure keyring
        let token = TokenManager::load_token();
        // Determine if the prompt should be shown based on whether the token was successfully loaded
        let prompt_open = token.is_none();

        Self {
            open_files: vec![EditorTab::new_empty()],
            active_tab: 0,
            output_text: String::new(),
            status_message: "Idle".into(),
            running: false,
            api_token: token, // Store the loaded token
            token_prompt_open: prompt_open,
            temp_token: String::new(),
            run_all_tabs: false,
            rx: None,
        }
    }

    // Called when the user clicks 'OK' in the token prompt
    pub fn confirm_token_input(&mut self) {
        if !self.temp_token.trim().is_empty() {
            let token_to_save = self.temp_token.trim().to_string();
            self.api_token = Some(token_to_save.clone()); // Update state
            TokenManager::save_token(&token_to_save); // Save securely to keyring
            self.temp_token.clear(); // Clear the input field
            self.token_prompt_open = false; // Close the prompt
        }
    }

    pub fn run_code(&mut self) {
        // Check for token presence
        if self.api_token.is_none() {
            self.output_text = "Error: No API token found. Please enter it in the prompt.\n".into();
            self.status_message = "No token!".into(); // Update status            return; // Stop execution
        }

        // Prevent multiple simultaneous runs
        if self.running {
            return;
        }

        // Get the token for the spawned task
        let token = self.api_token.clone().unwrap(); // Safe due to check above

        // Determine jobs based on run_all_tabs flag
        let jobs: Vec<(String, String)> = if self.run_all_tabs {
            self.open_files
                .iter()
                .map(|t| (t.title(), t.code.clone()))
                .collect()
        } else {
            let t = &self.open_files[self.active_tab];
            vec![(t.title(), t.code.clone())]
        };

        // Set UI state to running
        self.running = true;
        self.status_message = if jobs.len() == 1 {
            format!("Running {}...", jobs[0].0)
        } else {
            format!("Running {} files...", jobs.len())
        };

        // Set up channel for result communication
        let (tx, rx) = mpsc::unbounded_channel();
        self.rx = Some(rx);

        // Spawn the async task using the runtime initialized by #[tokio::main]
        tokio::task::spawn(async move {
            let client = reqwest::Client::new();
            let mut results = Vec::new();

            for (title, code) in &jobs {
                let req = RunRequest {
                    code: code.clone(), // Clone code for the request payload
                };

                // Perform the HTTP request
                let resp = client
                    .post(format!("{}/run", API_BASE))
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .json(&req)                    .send()
                    .await;

                // Process the response
                let block = match resp {
                    Ok(r) => {
                        let status = r.status();
                        let body = r.text().await.unwrap_or_default();

                        if status.is_success() {
                            // Try to deserialize the successful response
                            if let Ok(parsed) = serde_json::from_str::<RunResponse>(&body) {
                                let out = parsed.stdout.unwrap_or_default();
                                let err = parsed.stderr.unwrap_or_default();
                                format!("{}{}", out, err) // Combine stdout and stderr
                            } else {
                                // If deserialization fails, return the raw body
                                body
                            }
                        } else {
                            // Handle non-successful HTTP status codes
                            format!("HTTP {}: {}", status.as_u16(), body)
                        }
                    }
                    Err(e) => {
                        // Handle request errors (network, timeout, etc.)
                        format!("Request failed: {}", e)
                    }
                };

                // Format the result block
                if jobs.len() > 1 {
                    results.push(format!("===== {} =====\n{}", title, block));
                } else {
                    results.push(block);
                }
            }

            // Send the final combined results back to the main UI thread
            let _ = tx.send(results.join("\n\n")); // Join with double newline for readability
        });
    }

    pub fn save_active(&mut self) {
        if self.active_tab >= self.open_files.len() {
            return;
        }
        let tab = &mut self.open_files[self.active_tab];
        if let Some(path) = &tab.path {
            if std::fs::write(path, &tab.code).is_ok() {                tab.modified = false;
                self.status_message = format!("Saved {}", tab.title());
            } else {
                self.status_message = "Error saving".into();
            }
        } else {
            drop(tab); // Explicitly drop the mutable borrow before calling save_as
            self.save_active_as();
        }
    }

    pub fn save_active_as(&mut self) {
        if self.active_tab >= self.open_files.len() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Python", &["py"])
            .save_file()
        {
            let tab = &mut self.open_files[self.active_tab];
            if std::fs::write(&path, &tab.code).is_ok() {
                tab.path = Some(path);
                tab.modified = false;
                self.status_message = format!("Saved {}", tab.title());
            } else {
                self.status_message = "Save failed".into();
            }
        }
    }

    pub fn close_active_tab(&mut self) {
        if self.open_files.len() <= 1 {
            self.open_files = vec![EditorTab::new_empty()];
            self.active_tab = 0;
            return;
        }
        self.open_files.remove(self.active_tab);
        if self.active_tab >= self.open_files.len() {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
    }
}

impl App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // --- API token prompt ---
        if self.token_prompt_open {
            egui::Window::new("Enter Hugging Face API Token")                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Paste your personal access token:");
                    ui.text_edit_singleline(&mut self.temp_token);
                    if ui.button("OK").clicked() && !self.temp_token.trim().is_empty() {
                        self.confirm_token_input(); // Use the new dedicated function
                    }
                });
        }

        // --- Menu bar ---
        egui::Panel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Tab").clicked() {
                        self.open_files.push(EditorTab::new_empty());
                        self.active_tab = self.open_files.len() - 1;
                        ui.close();
                    }
                    if ui.button("Open…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Python", &["py"])
                            .pick_file()
                        {
                            if let Ok(code) = std::fs::read_to_string(&path) {
                                let cur = &self.open_files[self.active_tab];
                                if !cur.modified && cur.code.is_empty() && cur.path.is_none() {
                                    let tab = &mut self.open_files[self.active_tab];
                                    tab.code = code;
                                    tab.path = Some(path);
                                    tab.modified = false;
                                } else {
                                    self.open_files.push(EditorTab {
                                        path: Some(path),
                                        code,
                                        modified: false,
                                    });
                                    self.active_tab = self.open_files.len() - 1;
                                }
                            }
                        }
                        ui.close();
                    }
                    if ui.button("Save").clicked() {
                        self.save_active();
                        ui.close();
                    }
                    if ui.button("Save As…").clicked() {                        self.save_active_as();
                        ui.close();
                    }
                    if ui.button("Close Tab").clicked() {
                        self.close_active_tab();
                        ui.close();
                    }
                });
            });
        });

        // --- Tab bar ---
        egui::Panel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut close_idx = None;
                for (i, tab) in self.open_files.iter().enumerate() {
                    let fill = if i == self.active_tab {
                        egui::Color32::from_rgb(60, 60, 60)
                    } else {
                        egui::Color32::from_rgb(40, 40, 40)
                    };
                    if ui.add(egui::Button::new(tab.title()).fill(fill)).clicked() {
                        self.active_tab = i;
                    }
                    if ui.small_button("x").clicked() {
                        close_idx = Some(i);
                    }
                }
                if ui.small_button("+").clicked() {
                    self.open_files.push(EditorTab::new_empty());
                    self.active_tab = self.open_files.len() - 1;
                }
                if let Some(i) = close_idx {
                    if self.open_files.len() <= 1 {
                        self.open_files = vec![EditorTab::new_empty()];
                        self.active_tab = 0;
                    } else {
                        self.open_files.remove(i);
                        if self.active_tab >= self.open_files.len() {
                            self.active_tab = self.active_tab.saturating_sub(1);
                        }
                    }
                }
            });
        });

        // --- Output panel (right side) ---
        egui::Panel::right("output_panel")
            .resizable(true)
            .default_size([300.0, 0.0])            .show(ctx, |ui| {
                ui.heading("Output");
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.output_text.as_str())
                                .font(egui::FontId::monospace(13.0))
                                .interactive(false)
                                .desired_width(f32::INFINITY),
                        );
                    });
                if ui.button("Clear Output").clicked() {
                    self.output_text.clear();
                }
            });

        // --- Central editor area ---
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.active_tab < self.open_files.len() {
                let tab = &mut self.open_files[self.active_tab];
                let mut editor = CodeEditor::default()
                    .with_syntax(egui_code_editor::Syntax::python())
                    .with_theme(egui_code_editor::ColorTheme::MONOKAI)
                    .with_rows(25)
                    .with_fontsize(14.0)
                    .with_id_source(format!("tab_{}", self.active_tab));
                editor.show(ui, &mut tab.code);
            }
        });

        // --- Bottom bar (Run button + status) ---
        egui::Panel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let can_run = !self.running && self.api_token.is_some();
                if ui.add_enabled(can_run, egui::Button::new("▶ Run")).clicked() {
                    self.run_code();
                }
                ui.checkbox(&mut self.run_all_tabs, "Run all open files");
                if self.running {
                    ui.add(egui::Spinner::new());
                }
                ui.label(&self.status_message);
            });
        });

        // --- Poll the background channel for results ---
        if let Some(rx) = &mut self.rx {
            if let Ok(result) = rx.try_recv() {                self.output_text = result;
                self.running = false;
                self.status_message = "Idle".into();
                self.rx = None; // Clear the receiver handle
            }
        }

        // Request repaint while running to keep the spinner animating
        if self.running {
            ctx.request_repaint();
        }
    }
}