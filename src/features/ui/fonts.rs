//! Fallback fonts for device names that contain CJK characters.

use eframe::egui;

use crate::core::util;

/// Fonts tried, in order, when a device name is not plain ASCII.  Windows ships
/// Microsoft YaHei and SimSun as TrueType collections.
const FONT_CANDIDATES: &[(&str, u32)] = &[
    (r"C:\Windows\Fonts\msyh.ttc", 0),
    (r"C:\Windows\Fonts\msyh.ttf", 0),
    (r"C:\Windows\Fonts\simhei.ttf", 0),
    (r"C:\Windows\Fonts\simsun.ttc", 0),
    (r"C:\Windows\Fonts\segoeui.ttf", 0),
];

/// Loads the first available system font as a fallback for both families.
pub(super) fn load_fallback(ctx: &egui::Context) {
    let Some((path, index)) = FONT_CANDIDATES
        .iter()
        .find(|(path, _)| std::path::Path::new(path).exists())
    else {
        util::log("no system font found for non-ASCII device names");
        return;
    };

    let Ok(bytes) = std::fs::read(path) else {
        util::log(&format!("could not read font {path}"));
        return;
    };

    let mut data = egui::FontData::from_owned(bytes);
    data.index = *index;

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("system-cjk".to_owned(), std::sync::Arc::new(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system-cjk".to_owned());
    }

    ctx.set_fonts(fonts);
    util::log(&format!("loaded fallback font {path} for device names"));
}
