//! Cross-platform control window (egui/eframe).
//!
//! The window is only a front-end: `honrp run` is a separate daemon process
//! that keeps running when the window is closed.  The window starts the daemon
//! if it is not already up, polls it over a loopback control socket
//! (`control::status`), and can stop it explicitly after a warning.
//!
//! Closing the window (X) never stops Discord Rich Presence.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

use crate::discord_state::DiscordState;

const HELP_URL: &str = "https://github.com/NasirA1991/honrp#help--troubleshooting";

const WINDOW_W: f32 = 480.0;
const MAIN_BASE: f32 = 192.0;
const EXTRA_ROW: f32 = 24.0;
const OVERLAY_H: f32 = 236.0;

fn tall_window_requested() -> bool {
    let preview = std::env::var("HONRP_UI_PREVIEW").unwrap_or_default();
    !notice_shown()
        || std::env::var_os("HONRP_CONFIG_UI").is_some()
        || matches!(preview.as_str(), "setup" | "stop" | "details")
}

/// 64x64 RGBA, generated from assets/icon.png (no PNG decoder dependency).
const ICON_RGBA: &[u8] = include_bytes!("../assets/icon-64.rgba");

fn window_icon() -> egui::IconData {
    egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    }
}

struct Palette {
    ok: egui::Color32,
    ok_soft: egui::Color32,
    warn: egui::Color32,
    danger_button: egui::Color32,
    muted: egui::Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            ok: egui::Color32::from_rgb(0x5f, 0xc1, 0x63),
            ok_soft: egui::Color32::from_rgb(0x8a, 0xc0, 0x8f),
            warn: egui::Color32::from_rgb(0xe0, 0xa3, 0x3a),
            danger_button: egui::Color32::from_rgb(0x8f, 0x3a, 0x33),
            muted: egui::Color32::from_rgb(0xc2, 0xc2, 0xc2),
        }
    } else {
        Palette {
            ok: egui::Color32::from_rgb(0x1e, 0x7e, 0x34),
            ok_soft: egui::Color32::from_rgb(0x4f, 0x7a, 0x55),
            warn: egui::Color32::from_rgb(0x8a, 0x5a, 0x00),
            danger_button: egui::Color32::from_rgb(0x9c, 0x4a, 0x42),
            muted: egui::Color32::from_rgb(0x3c, 0x3f, 0x44),
        }
    }
}

fn shorten(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - (max - 3)).collect();
    format!("...{tail}")
}

fn faint_separator(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, color));
}

fn toggle_row(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(34.0, 18.0), egui::Sense::click());
        if resp.clicked() {
            *value = !*value;
            changed = true;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let on = *value;
        let (track, border) = if on {
            (ui.visuals().selection.bg_fill, ui.visuals().selection.bg_fill)
        } else if ui.visuals().dark_mode {
            (
                egui::Color32::from_rgb(0x3a, 0x3d, 0x44),
                egui::Color32::from_rgb(0x6d, 0x72, 0x7a),
            )
        } else {
            (
                egui::Color32::from_rgb(0xc9, 0xcc, 0xd1),
                egui::Color32::from_rgb(0x8a, 0x8f, 0x96),
            )
        };
        ui.painter().rect_filled(rect, 9.0, track);
        ui.painter().rect_stroke(
            rect,
            9.0,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        let knob_x = if on {
            rect.right() - 9.0
        } else {
            rect.left() + 9.0
        };
        let knob = egui::pos2(knob_x, rect.center().y);
        ui.painter().circle_filled(knob, 7.0, egui::Color32::WHITE);
        ui.painter()
            .circle_stroke(knob, 7.0, egui::Stroke::new(1.0, border));
        let text = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
        if text.clicked() {
            *value = !*value;
            changed = true;
        }
        if text.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    });
    changed
}

fn current_status() -> Option<crate::control::DaemonStatus> {
    let status = crate::control::latest();
    let Ok(mode) = std::env::var("HONRP_DISCORD_STATE") else {
        return status;
    };
    // Dev aid for screenshots of states that need a live game/handshake.
    let mut s = crate::control::DaemonStatus {
        daemon: true,
        ..Default::default()
    };
    match mode.as_str() {
        "off" => return None,
        "unknown" => {
            s.discord = false;
            s.discord_found = None;
        }
        "not-running" => {
            s.game_pid = Some(4242);
            s.in_game = true;
            s.details = "Ranked \u{b7} FoC \u{b7} All Pick".into();
            s.discord_found = Some(false);
        }
        "ready" => {
            s.discord_found = Some(true);
        }
        "unavailable" => {
            s.game_pid = Some(4242);
            s.in_game = true;
            s.details = "Ranked \u{b7} FoC \u{b7} All Pick".into();
            s.discord_found = Some(true);
        }
        "live" => {
            s.discord = true;
            s.discord_found = Some(true);
            s.game_pid = Some(4242);
            s.in_game = true;
            s.details = "Ranked \u{b7} FoC \u{b7} All Pick".into();
            s.state = "Pyromancer \u{b7} Lv 12 \u{b7} 5/2/8".into();
        }
        _ => return status,
    }
    Some(s)
}

#[derive(Default)]
struct UpdateUi {
    text: String,
    release: Option<crate::update::Release>,
}

pub fn run() -> anyhow::Result<()> {
    if !single_instance() {
        // the control window is already open
        return Ok(());
    }
    hide_own_console();

    // adopt a running daemon, or start one in the background
    crate::control::start_poller();
    std::thread::spawn(|| {
        crate::control::ensure_daemon();
    });

    #[cfg(windows)]
    if autostart_enabled() {
        // normalise older entries ("--minimized") to the daemon form
        let _ = set_autostart(true);
    }

    let start_h = if tall_window_requested() {
        OVERLAY_H
    } else {
        MAIN_BASE
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_W, start_h])
            .with_min_inner_size([WINDOW_W, MAIN_BASE])
            .with_resizable(false)
            .with_title("honrp")
            .with_icon(window_icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    let app = HonrpApp::new();
    eframe::run_native(
        "honrp",
        options,
        Box::new(move |_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
    .map_err(|e| anyhow::anyhow!("could not open the control window: {e}"))
}

struct HonrpApp {
    notice_open: bool,
    config_open: bool,
    details_open: bool,
    linux_autostart_open: bool,
    confirm_stop: bool,
    stop_dialog_focus: bool,
    confirm_uninstall: bool,
    restart_needed: bool,
    message: Option<String>,
    theme_set: bool,
    checked_once: bool,
    daemon_was_running: bool,
    window_h: f32,
    update: Arc<Mutex<UpdateUi>>,
    last_dir: Option<String>,
    config_dir: String,
    config_ipc: String,
    #[cfg(windows)]
    autostart: bool,
}

impl HonrpApp {
    fn new() -> Self {
        let cfg = crate::config::Config::load(None);
        let preview = std::env::var("HONRP_UI_PREVIEW").unwrap_or_default();
        Self {
            notice_open: !notice_shown() || preview == "setup",
            config_open: std::env::var_os("HONRP_CONFIG_UI").is_some(),
            details_open: preview == "details",
            linux_autostart_open: false,
            confirm_stop: preview == "stop",
            stop_dialog_focus: false,
            confirm_uninstall: false,
            restart_needed: false,
            message: None,
            theme_set: false,
            checked_once: false,
            daemon_was_running: false,
            window_h: if tall_window_requested() {
                OVERLAY_H
            } else {
                MAIN_BASE
            },
            update: Arc::new(Mutex::new(UpdateUi::default())),
            last_dir: None,
            config_dir: cfg.user_dir,
            config_ipc: cfg.ipc_socket,
            #[cfg(windows)]
            autostart: autostart_enabled(),
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, status: &Option<crate::control::DaemonStatus>) {
        let p = palette(ui.visuals().dark_mode);
        ui.add_space(4.0);
        ui.heading("Settings");
        ui.add_space(6.0);

        ui.label(egui::RichText::new("Game folder").strong());
        ui.label(
            egui::RichText::new(
                "Heroes of Newerth documents folder (contains console.log and startup.lua).",
            )
            .small()
            .color(p.muted),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.config_dir)
                .desired_width(ui.available_width())
                .hint_text(r"C:\Users\you\Documents\Juvio\Heroes of Newerth"),
        );
        ui.horizontal(|ui| {
            if ui.button("Browse...").clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Select the Heroes of Newerth user folder")
                    .pick_folder()
                {
                    self.config_dir = dir.display().to_string();
                }
            }
            if !self.config_dir.trim().is_empty() && ui.button("Open folder").clicked() {
                open_path(Path::new(self.config_dir.trim()));
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("Discord IPC socket").strong());
        ui.label(
            egui::RichText::new(
                "Empty = auto-detect. honrp needs no Discord install path; set this only for a \
                 custom socket/pipe.",
            )
            .small()
            .color(p.muted),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.config_ipc)
                .desired_width(ui.available_width())
                .hint_text(r"\\.\pipe\discord-ipc-0"),
        );

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            let (save, back) = ui
                .horizontal(|ui| (ui.button("Save").clicked(), ui.button("Back").clicked()))
                .inner;
            if save {
                match save_config(&self.config_dir, &self.config_ipc) {
                    Ok(()) => {
                        self.config_open = false;
                        self.restart_needed = true;
                        if status.is_some() {
                            restart_daemon();
                            self.message = Some("Saved. Restarting the daemon...".to_string());
                        } else {
                            self.message = Some("Saved. Start the daemon to apply.".to_string());
                        }
                    }
                    Err(e) => self.message = Some(format!("Could not save: {e}")),
                }
            } else if back {
                self.config_open = false;
            }
        });
    }

    fn update_snapshot(&self) -> (String, Option<crate::update::Release>) {
        match self.update.lock() {
            Ok(g) => (g.text.clone(), g.release.clone()),
            Err(_) => (String::new(), None),
        }
    }

    fn open_config(&mut self, status: &Option<crate::control::DaemonStatus>) {
        let cfg = crate::config::Config::load(None);
        self.config_dir = status
            .as_ref()
            .and_then(|s| s.user_dir.clone())
            .or_else(|| self.last_dir.clone())
            .or_else(|| {
                if cfg.user_dir.trim().is_empty() {
                    None
                } else {
                    Some(cfg.user_dir.clone())
                }
            })
            .unwrap_or_default();
        self.config_ipc = cfg.ipc_socket;
        self.config_open = true;
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        ui.set_min_width(210.0);
        if ui.button("Help / Troubleshooting").clicked() {
            open_url(HELP_URL);
        }
        if ui.button("Discord setup...").clicked() {
            self.notice_open = true;
        }
        if ui.button("Details...").clicked() {
            self.details_open = true;
        }
        ui.separator();
        if ui.button("Uninstall...").clicked() {
            self.confirm_uninstall = true;
        }
        ui.separator();
        let bridge = crate::mod_build_from(crate::MOD_SRC).unwrap_or(0);
        ui.label(
            egui::RichText::new(format!(
                "honrp {}  \u{b7}  bridge {}",
                env!("CARGO_PKG_VERSION"),
                bridge
            ))
            .small()
            .color(p.muted),
        );
    }
}

impl eframe::App for HonrpApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(500));

        if !self.theme_set {
            self.theme_set = true;
            let pref = match std::env::var("HONRP_THEME").ok().as_deref() {
                Some("light") => egui::ThemePreference::Light,
                Some("dark") => egui::ThemePreference::Dark,
                _ => egui::ThemePreference::System,
            };
            ctx.set_theme(pref);
        }
        if !self.checked_once {
            self.checked_once = true;
            check_updates(self.update.clone());
        }

        let status = current_status();
        let running = status.is_some();
        if running && !self.daemon_was_running {
            // A stale "stopped" notice must not outlive the stopped state.
            self.message = None;
        }
        self.daemon_was_running = running;
        if let Some(dir) = status.as_ref().and_then(|s| s.user_dir.as_deref()) {
            self.last_dir = Some(dir.to_string());
        }
        let (update_text, update_release) = self.update_snapshot();

        let game_running = status
            .as_ref()
            .map(|s| s.game_pid.is_some())
            .unwrap_or(false);
        let (connected, found) = status
            .as_ref()
            .map(|s| (s.discord, s.discord_found))
            .unwrap_or((false, None));
        let dstate = crate::discord_state::derive(status.is_some(), connected, found, game_running);
        let attention: Option<(&'static str, bool)> =
            if crate::discord_state::needs_attention(dstate, game_running) {
                match dstate {
                    DiscordState::Unknown => {
                        Some(("Restart honrp to refresh the Discord state.", false))
                    }
                    DiscordState::NotRunning => {
                        Some(("Start Discord to show your game status.", true))
                    }
                    DiscordState::Unavailable => Some((
                        "Discord is running, but honrp can't connect. Restart Discord.",
                        true,
                    )),
                    _ => None,
                }
            } else {
                None
            };

        let has_state = status
            .as_ref()
            .map(|s| s.in_game && !s.state.is_empty())
            .unwrap_or(false);

        let overlay_open = self.config_open
            || self.notice_open
            || self.confirm_stop
            || self.details_open
            || self.linux_autostart_open
            || self.confirm_uninstall;
        let desired_h = if overlay_open {
            OVERLAY_H
        } else {
            MAIN_BASE
                + if attention.is_some() {
                    EXTRA_ROW
                } else {
                    0.0
                }
                + if has_state { EXTRA_ROW } else { 0.0 }
        };
        if (self.window_h - desired_h).abs() > 0.5 {
            self.window_h = desired_h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                WINDOW_W, desired_h,
            )));
        }

        egui::CentralPanel::default().show(root, |ui| {
            if self.config_open {
                self.settings_ui(ui, &status);
                return;
            }
            let p = palette(ui.visuals().dark_mode);
            ui.add_space(4.0);

            // Rich Presence + Discord state, each with its own colour
            let dot = |ui: &mut egui::Ui, color: egui::Color32| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.8, color);
            };
            ui.horizontal(|ui| {
                let (rp_color, rp_text) = if status.is_some() {
                    (p.ok, "Rich Presence on")
                } else {
                    (p.muted, "Rich Presence off")
                };
                dot(ui, rp_color);
                ui.colored_label(
                    rp_color,
                    egui::RichText::new(rp_text).strong().size(14.0),
                )
                .on_hover_text(
                    "Backed by the honrp background process. Closing the window does not \
                     stop it.",
                );
                ui.label(egui::RichText::new(" \u{b7} ").color(p.muted));
                let (disc_color, disc_text, disc_tip) = match dstate {
                    DiscordState::Off => (
                        p.muted,
                        "Discord off",
                        "Rich Presence is off, so there is no Discord connection.",
                    ),
                    DiscordState::Unknown => (
                        p.muted,
                        "Discord state unknown",
                        "The daemon did not report whether Discord is reachable. Restart \
                         honrp to refresh it.",
                    ),
                    DiscordState::NotRunning => (
                        p.muted,
                        "Discord not running",
                        "Discord does not appear to be running.",
                    ),
                    DiscordState::Ready => (
                        p.ok_soft,
                        "Discord ready",
                        "Discord is reachable; honrp publishes when Heroes of Newerth starts.",
                    ),
                    DiscordState::Live => (
                        p.ok,
                        "Live on Discord",
                        "honrp is publishing your current Heroes of Newerth activity.",
                    ),
                    DiscordState::Unavailable => (
                        p.warn,
                        "Can't connect to Discord",
                        "Discord is running, but honrp could not open its IPC connection. It \
                         keeps retrying; restarting Discord usually fixes it.",
                    ),
                };
                dot(ui, disc_color);
                ui.colored_label(
                    disc_color,
                    egui::RichText::new(disc_text).strong().size(14.0),
                )
                .on_hover_text(disc_tip);
            });

            // The problem states are where the user must act; don't hide the
            // instruction in a tooltip only.
            if let Some((text, warn)) = attention {
                ui.label(
                    egui::RichText::new(text)
                        .size(11.5)
                        .color(if warn { p.warn } else { p.muted }),
                );
            }

            // card line; hero/level/KDA on hover
            let (game_text, tooltip) = match &status {
                Some(s) => {
                    let text = if s.game_pid.is_none() {
                        "Waiting for Heroes of Newerth".to_string()
                    } else if !s.details.is_empty() {
                        s.details.clone()
                    } else if s.in_game {
                        "In match".to_string()
                    } else if s.in_queue {
                        "Searching for a match".to_string()
                    } else {
                        "Heroes of Newerth running".to_string()
                    };
                    (text, s.state.clone())
                }
                None => ("Waiting for Heroes of Newerth".to_string(), String::new()),
            };
            let resp = ui.label(
                egui::RichText::new(game_text)
                    .size(16.0)
                    .color(ui.visuals().strong_text_color()),
            );
            if !tooltip.is_empty() {
                resp.on_hover_text(tooltip);
            }
            if let Some(s) = &status {
                if s.in_game && !s.state.is_empty() {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(&s.state).size(13.0).color(p.muted));
                }
            }

            ui.add_space(8.0);

            // primary action and the secondary controls share one row
            ui.horizontal(|ui| {
                let button = if status.is_some() {
                    egui::Button::new(
                        egui::RichText::new("Stop Rich Presence")
                            .strong()
                            .size(14.0),
                    )
                    .min_size(egui::vec2(0.0, 32.0))
                } else {
                    egui::Button::new(
                        egui::RichText::new("Start Rich Presence")
                            .strong()
                            .size(14.0)
                            .color(egui::Color32::WHITE),
                    )
                    .min_size(egui::vec2(0.0, 32.0))
                    .fill(p.ok)
                };
                if ui.add(button).clicked() {
                    if status.is_some() {
                        self.confirm_stop = true;
                        self.stop_dialog_focus = false;
                    } else {
                        std::thread::spawn(|| {
                            crate::control::ensure_daemon();
                        });
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("Menu", |ui| {
                        self.menu(ui);
                    })
                    .response
                    .on_hover_text("Help, Discord setup, Details, Uninstall");
                    if ui
                        .button("Config")
                        .on_hover_text("Game folder and Discord socket settings")
                        .clicked()
                    {
                        self.open_config(&status);
                    }
                });
            });

            ui.add_space(4.0);
            let hint = if status.is_some() {
                "Closing this window keeps Rich Presence running in the background."
            } else {
                "Rich Presence is off. Start it with the button above."
            };
            ui.label(egui::RichText::new(hint).size(12.0).color(p.muted));

            ui.add_space(8.0);
            #[cfg(windows)]
            {
                let mut auto = self.autostart;
                if toggle_row(ui, &mut auto, "Launch at startup") {
                    self.autostart = auto;
                    if let Err(e) = set_autostart(auto) {
                        self.message = Some(format!("Could not change autostart: {e}"));
                    }
                }
            }
            #[cfg(not(windows))]
            {
                if ui
                    .button("Launch at startup...")
                    .on_hover_text("How to enable launching honrp at startup")
                    .clicked()
                {
                    self.linux_autostart_open = true;
                }
            }

            let faint = egui::Color32::from_rgba_unmultiplied(
                p.muted.r(),
                p.muted.g(),
                p.muted.b(),
                46,
            );
            // The line belongs to the footer: more air above it than below.
            ui.add_space(10.0);
            faint_separator(ui, faint);
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                if let Some(msg) = &self.message {
                    ui.label(egui::RichText::new(msg).size(12.0).color(p.muted));
                } else {
                    let status_text = if update_text.is_empty() {
                        "check for updates"
                    } else {
                        update_text.as_str()
                    };
                    let line = format!(
                        "honrp {}  \u{b7}  {}",
                        env!("CARGO_PKG_VERSION"),
                        status_text
                    );
                    if ui
                        .link(egui::RichText::new(line).size(12.0).color(p.muted))
                        .on_hover_text("Check for updates")
                        .clicked()
                    {
                        check_updates(self.update.clone());
                    }
                }
                if let Some(release) = &update_release {
                    if ui
                        .button(format!("Install {}", release.version))
                        .clicked()
                    {
                        install_update(self.update.clone(), release.clone());
                    }
                }
            });
        });

        if self.linux_autostart_open {
            #[allow(deprecated)]
            let modal = egui::Modal::new(egui::Id::new("linux_autostart")).show(&ctx, |ui| {
                let p = palette(ui.visuals().dark_mode);
                ui.set_max_width(450.0);
                ui.heading("Launch at startup");
                ui.add_space(6.0);
                ui.label("honrp uses a systemd user service. Run this in a terminal:");
                ui.add_space(4.0);
                ui.monospace("honrp install");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("To remove it again: honrp uninstall").color(p.muted),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Copy command").clicked() {
                        ui.ctx().copy_text("honrp install".to_string());
                    }
                    let close = ui.button("Close").clicked();
                    close
                })
                .inner
            });
            if modal.inner || modal.should_close() {
                self.linux_autostart_open = false;
            }
        }

        if self.notice_open {
            #[allow(deprecated)]
            let modal = egui::Modal::new(egui::Id::new("discord_setup")).show(&ctx, |ui| {
                let p = palette(ui.visuals().dark_mode);
                ui.set_max_width(450.0);
                ui.heading("Discord setup");
                ui.add_space(4.0);
                ui.label(
                    "Discord also detects Heroes of Newerth (juvio.exe) as a separate game. \
                     That can create a duplicate activity card.",
                );
                ui.add_space(4.0);
                ui.label("To stop the duplicate, disable juvio.exe in Discord:");
                ui.monospace("Settings \u{2192} Game Activity \u{2192} Registered Games");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "honrp cannot change Discord's registered-games list; this is a one-time \
                         manual step.",
                    )
                    .color(p.muted),
                );
                ui.add_space(8.0);
                ui.button("Got it").clicked()
            });
            if modal.inner || modal.should_close() {
                self.notice_open = false;
                mark_notice();
            }
        }

        if self.confirm_stop {
            #[allow(deprecated)]
            let modal = egui::Modal::new(egui::Id::new("stop_daemon")).show(&ctx, |ui| {
                let p = palette(ui.visuals().dark_mode);
                ui.set_max_width(440.0);
                ui.heading("Stop Rich Presence?");
                ui.add_space(6.0);
                ui.label("Discord Rich Presence will stop. The window will remain open.");
                ui.add_space(4.0);
                ui.label(
                    "You can start it again at any time with Start Rich Presence. Closing the \
                     window does not stop it.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.button("Cancel");
                    if !self.stop_dialog_focus {
                        cancel.request_focus();
                        self.stop_dialog_focus = true;
                    }
                    let cancel_clicked = cancel.clicked();
                    let stop = ui.add(
                        egui::Button::new(
                            egui::RichText::new("Stop Rich Presence")
                                .color(egui::Color32::WHITE),
                        )
                        .fill(p.danger_button),
                    );
                    (stop.clicked(), cancel_clicked)
                })
                .inner
            });
            if modal.inner.0 {
                self.confirm_stop = false;
                self.stop_dialog_focus = false;
                self.message = Some(
                    "Rich Presence stopped. Start it again with Start Rich Presence.".into(),
                );
                std::thread::spawn(|| {
                    let _ = crate::control::stop();
                });
            } else if modal.inner.1 || modal.should_close() {
                self.confirm_stop = false;
                self.stop_dialog_focus = false;
            }
        }

        if self.details_open {
            #[allow(deprecated)]
            let modal = egui::Modal::new(egui::Id::new("details")).show(&ctx, |ui| {
                let p = palette(ui.visuals().dark_mode);
                ui.set_max_width(470.0);
                ui.heading("Details");
                ui.add_space(6.0);
                let bridge = crate::mod_build_from(crate::MOD_SRC).unwrap_or(0);
                let (daemon, discord, game) = match &status {
                    None => (
                        "stopped".to_string(),
                        "off".to_string(),
                        "not running".to_string(),
                    ),
                    Some(s) => {
                        let st = crate::discord_state::derive(
                            true,
                            s.discord,
                            s.discord_found,
                            s.game_pid.is_some(),
                        );
                        let discord = match st {
                            DiscordState::Live => "live on Discord",
                            DiscordState::Ready => "ready",
                            DiscordState::NotRunning => "not running",
                            DiscordState::Unavailable => "unavailable (IPC failed)",
                            DiscordState::Unknown => "unknown (daemon did not report)",
                            DiscordState::Off => "off",
                        }
                        .to_string();
                        (
                            "running".to_string(),
                            discord,
                            s.game_pid
                                .map(|pid| format!("pid {pid}"))
                                .unwrap_or_else(|| "not running".to_string()),
                        )
                    }
                };
                egui::Grid::new("details_grid")
                    .num_columns(2)
                    .spacing([18.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("honrp");
                        ui.label(env!("CARGO_PKG_VERSION"));
                        ui.end_row();
                        ui.label("Lua bridge");
                        ui.label(format!("build {bridge}"));
                        ui.end_row();
                        ui.label("Daemon");
                        ui.label(daemon);
                        ui.end_row();
                        ui.label("Discord");
                        ui.label(discord);
                        ui.end_row();
                        ui.label("Game");
                        ui.label(game);
                        ui.end_row();
                        ui.label("Game folder");
                        let dir = self.last_dir.as_deref().unwrap_or("not detected");
                        ui.add(egui::Label::new(
                            egui::RichText::new(shorten(dir, 34)).monospace(),
                        ))
                        .on_hover_text(dir);
                        ui.end_row();
                    });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "The daemon log is in %APPDATA%\\honrp\\daemon.log.",
                    )
                    .small()
                    .color(p.muted),
                );
                ui.add_space(8.0);
                ui.button("Close").clicked()
            });
            if modal.inner || modal.should_close() {
                self.details_open = false;
            }
        }

        if self.confirm_uninstall {
            #[allow(deprecated)]
            let modal = egui::Modal::new(egui::Id::new("uninstall")).show(&ctx, |ui| {
                ui.set_max_width(470.0);
                ui.heading("Uninstall honrp?");
                ui.add_space(6.0);
                ui.label(
                    "This stops the daemon and removes honrp: the program and shortcuts, the \
                     autostart entry, settings, and the Lua bridge from Heroes of Newerth.",
                );
                ui.add_space(4.0);
                ui.label("Discord Rich Presence will stop working.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let go = ui.button("Uninstall").clicked();
                    let cancel = ui.button("Cancel").clicked();
                    (go, cancel)
                })
                .inner
            });
            if modal.inner.0 {
                self.confirm_uninstall = false;
                let _ = crate::control::stop();
                if let Some(unins) = uninstaller_path() {
                    // let this process exit before the uninstaller deletes it
                    spawn_delayed_uninstall(&unins);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    match crate::uninstall(None, false) {
                        Ok(()) => {
                            self.message = Some(
                                "Removed the Lua bridge and service/autostart. Settings were kept; \
                                 delete them with `honrp uninstall --purge` if you want."
                                    .to_string(),
                            );
                        }
                        Err(e) => self.message = Some(format!("Uninstall failed: {e}")),
                    }
                }
            } else if modal.inner.1 || modal.should_close() {
                self.confirm_uninstall = false;
            }
        }
    }
}

fn restart_daemon() {
    std::thread::spawn(|| {
        let _ = crate::control::stop();
        std::thread::sleep(Duration::from_millis(700));
        let _ = crate::control::ensure_daemon();
    });
}

fn save_config(user_dir: &str, ipc: &str) -> std::io::Result<()> {
    let path = crate::config::config_path()
        .ok_or_else(|| std::io::Error::other("no config directory"))?;
    let mut value: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    value["user_dir"] = serde_json::json!(user_dir.trim());
    value["ipc_socket"] = serde_json::json!(ipc.trim());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n",
    )
}

/// The Inno Setup uninstaller lives next to honrp.exe (`unins000.exe`).
fn uninstaller_path() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("unins") && name.ends_with(".exe") {
            return Some(entry.path());
        }
    }
    None
}

#[cfg(windows)]
fn spawn_delayed_uninstall(uninstaller: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let arg = format!(
        "timeout /t 2 /nobreak >nul & \"{}\" /SILENT /NORESTART /SUPPRESSMSGBOXES",
        uninstaller.display()
    );
    let _ = Command::new("cmd")
        .args(["/C", &arg])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

#[cfg(not(windows))]
fn spawn_delayed_uninstall(uninstaller: &Path) {
    let _ = Command::new(uninstaller).spawn();
}

fn check_updates(shared: Arc<Mutex<UpdateUi>>) {
    std::thread::spawn(move || {
        if let Ok(mut g) = shared.lock() {
            g.text = "Checking for updates...".to_string();
            g.release = None;
        }
        match crate::update::fetch_latest() {
            Ok(release) => {
                if crate::update::is_newer(&release.version, crate::update::current_version()) {
                    if let Ok(mut g) = shared.lock() {
                        g.text = format!("Update {} available", release.version);
                        g.release = Some(release);
                    }
                } else if let Ok(mut g) = shared.lock() {
                    g.text = "Up to date".to_string();
                    g.release = None;
                }
            }
            Err(e) => {
                if let Ok(mut g) = shared.lock() {
                    g.text = format!("Update check failed: {e}");
                    g.release = None;
                }
            }
        }
    });
}

fn install_update(shared: Arc<Mutex<UpdateUi>>, release: crate::update::Release) {
    std::thread::spawn(move || {
        if let Ok(mut g) = shared.lock() {
            g.text = "Downloading update...".to_string();
        }
        match crate::update::install(&release) {
            Ok(_) => {
                if let Ok(mut g) = shared.lock() {
                    g.text = "Updated - close this window and start honrp again".to_string();
                    g.release = None;
                }
            }
            Err(e) => {
                if let Ok(mut g) = shared.lock() {
                    g.text = format!("Update failed: {e}");
                    g.release = None;
                }
            }
        }
    });
}

#[cfg(windows)]
fn shell_open(target: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        // No cmd.exe / inherited stdio: works after FreeConsole() too.
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn open_path(path: &Path) {
    #[cfg(windows)]
    shell_open(&path.display().to_string());
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(path).spawn();
}

fn open_url(url: &str) {
    #[cfg(windows)]
    shell_open(url);
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
}

fn single_instance() -> bool {
    match std::net::TcpListener::bind(("127.0.0.1", 47831)) {
        Ok(listener) => {
            std::mem::forget(listener);
            true
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn hide_own_console() {
    use windows_sys::Win32::System::Console::{
        FreeConsole, GetConsoleProcessList, GetConsoleWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        let hwnd = GetConsoleWindow();
        if hwnd.is_null() {
            return;
        }
        let mut pids = [0u32; 2];
        let count = GetConsoleProcessList(pids.as_mut_ptr(), 2);
        if count <= 1 {
            ShowWindow(hwnd, SW_HIDE);
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
fn hide_own_console() {}

fn notice_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("honrp").join(".discord_notice_shown"))
}

fn notice_shown() -> bool {
    notice_path().map(|p| p.exists()).unwrap_or(true)
}

fn mark_notice() {
    if let Some(path) = notice_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, b"");
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
fn autostart_enabled() -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", "honrp"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn set_autostart(on: bool) -> std::io::Result<()> {
    if on {
        let exe = std::env::current_exe()?;
        let value = format!("\"{}\" run", exe.display());
        let status = Command::new("reg")
            .args([
                "add", RUN_KEY, "/v", "honrp", "/t", "REG_SZ", "/d", &value, "/f",
            ])
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other("reg add failed"));
        }
    } else {
        let _ = Command::new("reg")
            .args(["delete", RUN_KEY, "/v", "honrp", "/f"])
            .status()?;
    }
    Ok(())
}
