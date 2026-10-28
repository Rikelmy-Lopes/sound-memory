//! The devices window itself: table of saved and live levels plus command
//! buttons.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};

use super::fonts;
use super::{AUDIO, COMMANDS, CONTEXT, OPEN, SHARED, UNAVAILABLE};
use crate::commands::ids;
use crate::core::command::{CommandContext, CommandRegistry};
use crate::core::state::{DeviceStatus, SavedDevice, Shared};
use crate::core::util;
use crate::features::audio::AudioRequest;
use crate::features::store;

const WINDOW_TITLE: &str = "sound-memory · devices";

/// Saved percentage plus the live percentage when the device is connected.
fn row_for(device: &SavedDevice, connected: &[DeviceStatus]) -> (u32, Option<u32>) {
    let saved = store::percent(device.units);
    let now = connected
        .iter()
        .find(|status| status.name.eq_ignore_ascii_case(&device.name))
        .map(|status| status.level_pct);
    (saved, now)
}

pub(super) fn run_window() {
    let (Some(shared), Some(audio), Some(commands)) =
        (SHARED.get(), AUDIO.get(), COMMANDS.get())
    else {
        return;
    };
    let shared = Arc::clone(shared);
    let audio = audio.clone();
    let commands = Arc::clone(commands);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("sound-memory")
            .with_title(WINDOW_TITLE)
            .with_inner_size([620.0, 420.0])
            .with_min_inner_size([420.0, 240.0]),
        ..Default::default()
    };

    OPEN.store(true, Ordering::SeqCst);
    let result = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(DevicesUi::new(cc, shared, audio, commands)))),
    );
    OPEN.store(false, Ordering::SeqCst);

    if let Ok(mut ctx) = CONTEXT.lock() {
        *ctx = None;
    }

    if let Err(err) = result {
        UNAVAILABLE.store(true, Ordering::SeqCst);
        util::log(&format!("devices window failed: {err}"));
        util::show_message(
            &format!("sound-memory could not create its devices window:\n{err}"),
            "sound-memory",
            true,
        );
    }
}

struct DevicesUi {
    shared: Arc<Shared>,
    audio: Sender<AudioRequest>,
    commands: Arc<CommandRegistry>,
    fonts_loaded: bool,
}

impl DevicesUi {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Shared>,
        audio: Sender<AudioRequest>,
        commands: Arc<CommandRegistry>,
    ) -> Self {
        if let Ok(mut slot) = CONTEXT.lock() {
            *slot = Some(cc.egui_ctx.clone());
        }

        let mut app = Self {
            shared,
            audio,
            commands,
            fonts_loaded: false,
        };
        // Latin-only stores keep egui's bundled font; CJK names need a system font.
        app.load_fonts_if_needed(&cc.egui_ctx);
        app
    }

    fn load_fonts_if_needed(&mut self, ctx: &egui::Context) {
        if self.fonts_loaded || !self.names_need_cjk() {
            return;
        }

        fonts::load_fallback(ctx);
        self.fonts_loaded = true;
    }

    fn names_need_cjk(&self) -> bool {
        let saved = self.shared.saved_devices();
        let connected = self.connected();
        saved
            .iter()
            .map(|device| device.name.as_str())
            .chain(connected.iter().map(|device| device.name.as_str()))
            .any(|name| !name.is_ascii())
    }

    fn connected(&self) -> Vec<DeviceStatus> {
        self.shared
            .devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn snapshot(&self) -> (Vec<SavedDevice>, Vec<DeviceStatus>) {
        (self.shared.saved_devices(), self.connected())
    }

    fn execute(&self, id: &'static str) {
        let ctx = CommandContext::new(Arc::clone(&self.shared), self.audio.clone());
        let _ = self.commands.run(id, &ctx);
    }
}

impl eframe::App for DevicesUi {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(250));
        self.load_fonts_if_needed(&ctx);

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Bluetooth volumes");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Hide").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
            ui.label(
                RichText::new("One saved level per Bluetooth audio endpoint.")
                    .weak()
                    .small(),
            );
            ui.add_space(8.0);

            let (saved, connected) = self.snapshot();

            if saved.is_empty() {
                ui.label(
                    "No device recorded yet. Connect a Bluetooth headset once and its current \
                     volume will be remembered.",
                );
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - 44.0)
                    .show(ui, |ui| {
                        egui::Grid::new("saved-devices")
                            .num_columns(4)
                            .striped(true)
                            .spacing([14.0, 6.0])
                            .min_col_width(70.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new("Device").strong());
                                ui.label(RichText::new("Saved").strong());
                                ui.label(RichText::new("Now").strong());
                                ui.label(RichText::new("State").strong());
                                ui.end_row();

                                for device in &saved {
                                    let (saved_pct, now_pct) = row_for(device, &connected);

                                    ui.label(util::truncate(&device.name, 38))
                                        .on_hover_text(format!("{}\n{}", device.name, device.id));

                                    ui.label(format!("{saved_pct}%"));

                                    match now_pct {
                                        Some(now_pct) => {
                                            ui.label(format!("{now_pct}%"));
                                            ui.label(
                                                RichText::new("connected")
                                                    .color(Color32::from_rgb(0x4c, 0xaf, 0x50)),
                                            );
                                        }
                                        None => {
                                            ui.label(RichText::new("—").weak());
                                            ui.label(RichText::new("saved").weak());
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Restore saved volumes").clicked() {
                    self.execute(ids::RESTORE_ALL);
                }
                if ui.button("Open data folder").clicked() {
                    self.execute(ids::OPEN_DATA_FOLDER);
                }
                if ui.button("Open log").clicked() {
                    self.execute(ids::OPEN_LOG_FILE);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, units: u32) -> SavedDevice {
        SavedDevice {
            id: format!("id-{name}"),
            name: name.to_string(),
            units,
        }
    }

    fn status(name: &str, level_pct: u32) -> DeviceStatus {
        DeviceStatus {
            name: name.to_string(),
            level_pct,
        }
    }

    #[test]
    fn row_shows_saved_level_when_disconnected() {
        let saved = device("Headset", 9600);
        assert_eq!(row_for(&saved, &[]), (96, None));
    }

    #[test]
    fn row_matches_connected_device_by_name() {
        let saved = device("Headset (联想)", 4250);
        let connected = vec![status("headset (联想)", 80)];
        assert_eq!(row_for(&saved, &connected), (43, Some(80)));
    }

    #[test]
    fn row_ignores_other_devices() {
        let saved = device("Headset", 5000);
        let connected = vec![status("Speaker", 30)];
        assert_eq!(row_for(&saved, &connected), (50, None));
    }
}
