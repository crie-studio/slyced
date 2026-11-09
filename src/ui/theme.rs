//! Visual identity: a deep graphite dark theme with an electric-blue accent
//! (plus light and high-contrast variants), Segoe UI typography (read from the
//! Windows font folder, never bundled) and Phosphor icons.

use crate::config::Theme;
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

// Fixed colors of the dark theme. Images, and everything drawn over them, use
// these in every theme; the interface chrome uses `pal()`.

pub const BG_APP: Color32 = Color32::from_rgb(9, 11, 15);
pub const BG_PANEL: Color32 = Color32::from_rgb(15, 18, 24);
pub const BG_RAISED: Color32 = Color32::from_rgb(22, 26, 34);
pub const BG_HOVER: Color32 = Color32::from_rgb(30, 35, 45);
pub const BG_ACTIVE: Color32 = Color32::from_rgb(37, 44, 57);
pub const BG_INPUT: Color32 = Color32::from_rgb(11, 13, 18);
pub const BORDER: Color32 = Color32::from_rgb(33, 38, 49);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(48, 56, 70);

pub const TEXT: Color32 = Color32::from_rgb(229, 233, 240);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 160, 177);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(98, 107, 124);

pub const ACCENT: Color32 = Color32::from_rgb(77, 141, 255);
pub const ACCENT_2: Color32 = Color32::from_rgb(132, 102, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(19, 35, 64, 64);
pub const SUCCESS: Color32 = Color32::from_rgb(61, 214, 140);
pub const WARNING: Color32 = Color32::from_rgb(245, 181, 68);
pub const DANGER: Color32 = Color32::from_rgb(242, 85, 90);
pub const MEASURE: Color32 = Color32::from_rgb(255, 214, 64);

pub const OVERLAY_TEXT: Color32 = Color32::from_rgb(214, 222, 235);

/// Colors of the interface chrome in the chosen theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg_app: Color32,
    pub bg_panel: Color32,
    pub bg_raised: Color32,
    pub bg_hover: Color32,
    pub bg_active: Color32,
    pub bg_input: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    /// Text of hovered or selected items.
    pub strong: Color32,
    pub accent: Color32,
    pub accent_2: Color32,
    pub accent_soft: Color32,
    /// Selected toggle buttons and cards.
    pub selected_bg: Color32,
    pub selected_border: Color32,
    /// Slider rails and checkbox boxes.
    pub widget_bg: Color32,
    pub faint_bg: Color32,
    pub selection_bg: Color32,
    /// Opacity of window and popup shadows.
    pub shadow: u8,
    pub dark: bool,
}

const DARK: Palette = Palette {
    bg_app: BG_APP,
    bg_panel: BG_PANEL,
    bg_raised: BG_RAISED,
    bg_hover: BG_HOVER,
    bg_active: BG_ACTIVE,
    bg_input: BG_INPUT,
    border: BORDER,
    border_strong: BORDER_STRONG,
    text: TEXT,
    text_dim: TEXT_DIM,
    text_faint: TEXT_FAINT,
    strong: Color32::WHITE,
    accent: ACCENT,
    accent_2: ACCENT_2,
    accent_soft: ACCENT_SOFT,
    selected_bg: Color32::from_rgb(28, 48, 86),
    selected_border: Color32::from_rgb(52, 88, 150),
    widget_bg: Color32::from_rgb(40, 47, 61),
    faint_bg: Color32::from_rgb(18, 21, 28),
    selection_bg: Color32::from_rgb(36, 70, 132),
    shadow: 140,
    dark: true,
};

const LIGHT: Palette = Palette {
    bg_app: Color32::from_rgb(232, 235, 241),
    bg_panel: Color32::from_rgb(246, 247, 250),
    bg_raised: Color32::from_rgb(255, 255, 255),
    bg_hover: Color32::from_rgb(230, 234, 242),
    bg_active: Color32::from_rgb(216, 224, 239),
    bg_input: Color32::from_rgb(250, 251, 253),
    border: Color32::from_rgb(218, 223, 232),
    border_strong: Color32::from_rgb(190, 198, 212),
    text: Color32::from_rgb(25, 30, 40),
    text_dim: Color32::from_rgb(80, 90, 108),
    text_faint: Color32::from_rgb(120, 129, 146),
    strong: Color32::from_rgb(8, 10, 14),
    accent: Color32::from_rgb(37, 99, 225),
    accent_2: Color32::from_rgb(108, 78, 228),
    accent_soft: Color32::from_rgba_premultiplied(30, 60, 120, 30),
    selected_bg: Color32::from_rgb(219, 231, 254),
    selected_border: Color32::from_rgb(132, 168, 240),
    widget_bg: Color32::from_rgb(208, 214, 226),
    faint_bg: Color32::from_rgb(240, 242, 246),
    selection_bg: Color32::from_rgb(186, 208, 250),
    shadow: 40,
    dark: false,
};

const HIGH_CONTRAST: Palette = Palette {
    bg_app: Color32::BLACK,
    bg_panel: Color32::BLACK,
    bg_raised: Color32::from_rgb(12, 12, 12),
    bg_hover: Color32::from_rgb(48, 48, 48),
    bg_active: Color32::from_rgb(64, 64, 64),
    bg_input: Color32::BLACK,
    border: Color32::from_rgb(170, 170, 170),
    border_strong: Color32::WHITE,
    text: Color32::WHITE,
    text_dim: Color32::from_rgb(235, 235, 235),
    text_faint: Color32::from_rgb(205, 205, 205),
    strong: Color32::from_rgb(255, 230, 0),
    accent: Color32::from_rgb(64, 160, 255),
    accent_2: Color32::from_rgb(150, 110, 255),
    accent_soft: Color32::from_rgba_premultiplied(20, 50, 90, 90),
    selected_bg: Color32::from_rgb(0, 58, 130),
    selected_border: Color32::from_rgb(255, 230, 0),
    widget_bg: Color32::from_rgb(90, 90, 90),
    faint_bg: Color32::from_rgb(8, 8, 8),
    selection_bg: Color32::from_rgb(0, 80, 170),
    shadow: 0,
    dark: true,
};

static THEME: AtomicU8 = AtomicU8::new(0);

/// Colors of the interface in the current theme.
pub fn pal() -> &'static Palette {
    match THEME.load(Ordering::Relaxed) {
        1 => &LIGHT,
        2 => &HIGH_CONTRAST,
        _ => &DARK,
    }
}

/// Switches the interface theme (takes effect on the next frame).
pub fn set_theme(ctx: &egui::Context, theme: Theme) {
    THEME.store(theme as u8, Ordering::Relaxed);
    apply_visuals(ctx);
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn system_font(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
    std::fs::read(dir.join("Fonts").join(name)).ok()
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let icons = Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Phosphor.ttf")).tweak(
        egui::FontTweak { y_offset_factor: 0.08, ..Default::default() },
    ));
    fonts.font_data.insert("phosphor".into(), icons);

    let mut prop: Vec<String> = Vec::new();
    let mut bold: Vec<String> = Vec::new();
    if let Some(d) = system_font("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(d)));
        prop.push("segoe".into());
    }
    if let Some(d) = system_font("seguisb.ttf") {
        fonts.font_data.insert("segoe-sb".into(), Arc::new(FontData::from_owned(d)));
        bold.push("segoe-sb".into());
    }
    if let Some(d) = system_font("consola.ttf") {
        fonts.font_data.insert("consolas".into(), Arc::new(FontData::from_owned(d)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    prop.push("phosphor".into());
    prop.extend(defaults.iter().cloned());
    if bold.is_empty() {
        bold.extend(prop.iter().cloned());
    } else {
        bold.push("phosphor".into());
        bold.extend(defaults.iter().cloned());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(semibold(), bold);
    fonts.families.entry(FontFamily::Monospace).or_default().push("phosphor".into());
    ctx.set_fonts(fonts);
    // The interface size is a setting (Configurações); Ctrl+/- would fight it.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);

    ctx.set_theme(egui::Theme::Dark);
    apply_visuals(ctx);
}

/// Text sizes, spacing and colors of egui's own widgets, from the current palette.
fn apply_visuals(ctx: &egui::Context) {
    let p = pal();
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, font_bold(17.0)),
            (TextStyle::Body, font(13.5)),
            (TextStyle::Button, font(13.5)),
            (TextStyle::Small, font(11.5)),
            (TextStyle::Monospace, mono(12.5)),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.interact_size = egui::vec2(28.0, 26.0);
        s.window_margin = Margin::same(14);
        s.menu_margin = Margin::same(6);
        s.slider_width = 150.0;
        s.combo_height = 320.0;
        s.scroll.bar_width = 8.0;
        s.scroll.floating = true;

        let v = &mut style.visuals;
        v.dark_mode = p.dark;
        v.panel_fill = p.bg_panel;
        v.window_fill = p.bg_raised;
        v.window_stroke = Stroke::new(1.0, p.border_strong);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_shadow = Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(p.shadow) };
        v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(p.shadow.saturating_sub(20)) };
        v.extreme_bg_color = p.bg_input;
        v.text_edit_bg_color = Some(p.bg_input);
        v.faint_bg_color = p.faint_bg;
        v.code_bg_color = p.bg_raised;
        v.hyperlink_color = p.accent;
        v.warn_fg_color = WARNING;
        v.error_fg_color = DANGER;
        v.selection.bg_fill = p.selection_bg;
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.override_text_color = None;
        v.slider_trailing_fill = true;
        v.indent_has_left_vline = false;
        v.striped = false;

        let radius = CornerRadius::same(7);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = p.bg_panel;
        w.noninteractive.weak_bg_fill = p.bg_panel;
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_dim);
        w.noninteractive.corner_radius = radius;

        // bg_fill also paints slider rails and checkbox boxes, which sit on raised cards.
        w.inactive.bg_fill = p.widget_bg;
        w.inactive.weak_bg_fill = p.bg_raised;
        w.inactive.bg_stroke = Stroke::new(1.0, p.border);
        w.inactive.fg_stroke = Stroke::new(1.0, p.text);
        w.inactive.corner_radius = radius;
        w.inactive.expansion = 0.0;

        w.hovered.bg_fill = p.bg_hover;
        w.hovered.weak_bg_fill = p.bg_hover;
        w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
        w.hovered.fg_stroke = Stroke::new(1.0, p.strong);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = p.bg_active;
        w.active.weak_bg_fill = p.bg_active;
        w.active.bg_stroke = Stroke::new(1.0, p.accent);
        w.active.fg_stroke = Stroke::new(1.0, p.strong);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open = w.active;
        style.animation_time = 0.12;
    });
}

/// Paints the Windows 11 title bar in the app's colors (no-op elsewhere).
#[cfg(windows)]
pub fn style_title_bar(handle: &impl raw_window_handle::HasWindowHandle) {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let Ok(h) = handle.window_handle() else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else { return };
    let hwnd = w.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let dark: i32 = pal().dark as i32;
    // COLORREF is 0x00BBGGRR.
    let bg = pal().bg_panel;
    let caption: u32 = (bg.b() as u32) << 16 | (bg.g() as u32) << 8 | bg.r() as u32;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&dark as *const i32).cast(), 4);
        DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as u32, (&caption as *const u32).cast(), 4);
    }
}
