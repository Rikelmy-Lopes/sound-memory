//! The devices window itself: a compact dashboard with saved and live levels
//! plus command buttons.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Stroke};

use super::fonts;
use super::{AUDIO, COMMANDS, CONTEXT, OPEN, SHARED, UNAVAILABLE};
use crate::commands::ids;
use crate::core::command::{CommandContext, CommandRegistry};
use crate::core::state::{DeviceStatus, SavedDevice, Shared};
use crate::core::util;
use crate::features::audio::AudioRequest;
use crate::features::store;

const WINDOW_TITLE: &str = "Sound Memory";

const BG: Color32 = Color32::from_rgb(0x0d, 0x11, 0x17);
const CARD: Color32 = Color32::from_rgb(0x17, 0x1d, 0x27);
const CARD_HOVER: Color32 = Color32::from_rgb(0x21, 0x29, 0x36);
const BORDER: Color32 = Color32::from_rgb(0x2a, 0x33, 0x41);
const TEXT: Color32 = Color32::from_rgb(0xe6, 0xea, 0xf1);
const MUTED: Color32 = Color32::from_rgb(0x94, 0x9e, 0xae);
const ACCENT: Color32 = Color32::from_rgb(0x60, 0xa5, 0xfa);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x83, 0xbb, 0xfc);
const ACCENT_ACTIVE: Color32 = Color32::from_rgb(0x3b, 0x82, 0xd4);
const GREEN: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);

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
    let (Some(shared), Some(audio), Some(commands)) = (SHARED.get(), AUDIO.get(), COMMANDS.get())
    else {
        return;
    };
    let shared = Arc::clone(shared);
    let audio = audio.clone();
    let commands = Arc::clone(commands);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("Sound Memory")
            .with_title(WINDOW_TITLE)
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([520.0, 340.0]),
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
            &format!("Sound Memory could not create its devices window:\n{err}"),
            "Sound Memory",
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

        apply_theme(&cc.egui_ctx);

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

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = BG;
    style.visuals.extreme_bg_color = CARD;
    style.visuals.faint_bg_color = CARD;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    style.visuals.selection.stroke = Stroke::new(1.0, TEXT);

    let widgets = &mut style.visuals.widgets;
    widgets.noninteractive.corner_radius = CornerRadius::same(8);
    widgets.noninteractive.bg_fill = CARD;
    widgets.noninteractive.weak_bg_fill = CARD;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);

    widgets.inactive.corner_radius = CornerRadius::same(8);
    widgets.inactive.bg_fill = CARD;
    widgets.inactive.weak_bg_fill = CARD;
    widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    widgets.hovered.corner_radius = CornerRadius::same(8);
    widgets.hovered.bg_fill = CARD_HOVER;
    widgets.hovered.weak_bg_fill = CARD_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    widgets.active.corner_radius = CornerRadius::same(8);
    widgets.active.bg_fill = ACCENT;
    widgets.active.weak_bg_fill = ACCENT;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.fg_stroke = Stroke::new(1.0, Color32::BLACK);

    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 30.0;

    ctx.set_style_of(egui::Theme::Dark, style);
}

fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
}

fn stat_card(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    value: &str,
    accent: Color32,
    value_size: f32,
) {
    card_frame().inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.set_width((width - 28.0).max(60.0));
        ui.set_min_height(42.0);
        ui.label(RichText::new(label).size(11.0).strong().color(MUTED));
        ui.add_space(3.0);
        ui.label(RichText::new(value).size(value_size).strong().color(accent));
    });
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 34);
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(color));
        });
}

fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let ink = Color32::from_rgb(0x08, 0x11, 0x1e);

    ui.scope(|ui| {
        {
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.inactive.bg_fill = ACCENT;
            widgets.inactive.weak_bg_fill = ACCENT;
            widgets.inactive.bg_stroke = Stroke::new(1.0, ACCENT);
            widgets.inactive.fg_stroke = Stroke::new(1.0, ink);

            widgets.hovered.bg_fill = ACCENT_HOVER;
            widgets.hovered.weak_bg_fill = ACCENT_HOVER;
            widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
            widgets.hovered.fg_stroke = Stroke::new(1.0, ink);

            widgets.active.bg_fill = ACCENT_ACTIVE;
            widgets.active.weak_bg_fill = ACCENT_ACTIVE;
            widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_ACTIVE);
            widgets.active.fg_stroke = Stroke::new(1.0, ink);
        }

        ui.add(
            egui::Button::new(RichText::new(text).strong())
                .frame(true)
                .corner_radius(CornerRadius::same(8))
                .min_size(egui::vec2(0.0, 33.0)),
        )
    })
    .inner
}

fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT))
            .frame(true)
            .corner_radius(CornerRadius::same(8))
            .min_size(egui::vec2(0.0, 33.0)),
    )
}

fn device_card(ui: &mut egui::Ui, device: &SavedDevice, saved_pct: u32, now_pct: Option<u32>) {
    card_frame().inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(util::truncate(&device.name, 44))
                    .size(15.0)
                    .strong()
                    .color(TEXT),
            )
            .on_hover_text(format!("{}\n{}", device.name, device.id));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match now_pct {
                Some(now) => pill(ui, &format!("Connected · {now}%"), GREEN),
                None => pill(ui, "Saved", MUTED),
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Saved level {saved_pct}%"))
                    .size(12.0)
                    .color(MUTED),
            );
            if let Some(now) = now_pct {
                ui.label(
                    RichText::new(format!("Current {now}%"))
                        .size(12.0)
                        .color(GREEN),
                );
            }
        });

        ui.add_space(6.0);
        ui.add(
            egui::ProgressBar::new(saved_pct as f32 / 100.0)
                .desired_width(ui.available_width())
                .desired_height(7.0)
                .corner_radius(CornerRadius::same(4))
                .fill(if now_pct.is_some() { GREEN } else { ACCENT }),
        );
    });
}

impl eframe::App for DevicesUi {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(250));
        self.load_fonts_if_needed(&ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(20)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Sound Memory")
                                .size(26.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.label(
                            RichText::new("Bluetooth volume memory")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if secondary_button(ui, "Hide").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });

                ui.add_space(14.0);

                let (saved, connected) = self.snapshot();
                let live = saved
                    .iter()
                    .filter(|device| row_for(device, &connected).1.is_some())
                    .count();

                let gap = ui.spacing().item_spacing.x;
                let card_width = ((ui.available_width() - gap * 2.0) / 3.0).max(120.0);
                ui.horizontal_top(|ui| {
                    stat_card(
                        ui,
                        card_width,
                        "SAVED DEVICES",
                        &saved.len().to_string(),
                        ACCENT,
                        21.0,
                    );
                    stat_card(ui, card_width, "CONNECTED", &live.to_string(), GREEN, 21.0);
                    stat_card(ui, card_width, "SAVE FILE", "devices.json", MUTED, 16.0);
                });

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Devices").size(16.0).strong().color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary_button(ui, "Restore all").clicked() {
                            self.execute(ids::RESTORE_ALL);
                        }
                    });
                });

                ui.add_space(6.0);

                let list_height = (ui.available_height() - 54.0).max(100.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(list_height)
                    .show(ui, |ui| {
                        if saved.is_empty() {
                            card_frame().inner_margin(Margin::same(24)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new("No devices recorded yet")
                                            .size(17.0)
                                            .strong()
                                            .color(TEXT),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(
                                            "Connect a Bluetooth headset once. Its current \
                                                 volume will be saved and restored the next time \
                                                 it connects.",
                                        )
                                        .size(12.5)
                                        .color(MUTED),
                                    );
                                });
                            });
                        } else {
                            for device in &saved {
                                let (saved_pct, now_pct) = row_for(device, &connected);
                                device_card(ui, device, saved_pct, now_pct);
                            }
                        }
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if secondary_button(ui, "Open data folder").clicked() {
                        self.execute(ids::OPEN_DATA_FOLDER);
                    }
                    if secondary_button(ui, "Open log").clicked() {
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
