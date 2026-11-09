//! Small previews that show an effect before it is chosen: the reading
//! highlights applied to the current image, and the 3D presets rendered (on the
//! CPU, at low resolution) from the open volume.

use super::review::Special;
use super::theme::*;
use super::viewport::Content;
use crate::app::{SlycedApp, Workspace};
use crate::dicom::series::{LoadedSeries, Slice};
use crate::imaging::highlight::{self, Highlights};
use crate::imaging::lut::Window;
use crate::imaging::volume::{Quant, Volume, OUTSIDE};
use crate::render3d::tf::{self, TfAdjust, TF_SIZE};
use egui::{pos2, vec2, Align2, Color32, ColorImage, CornerRadius, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui};
use std::sync::Arc;

/// Width of a highlight preview, in pixels.
const SPECIAL_W: usize = 128;
/// Side of a 3D preset preview, in pixels.
const VR_SIZE: usize = 96;

#[derive(Default)]
pub struct Previews {
    specials: Option<(u64, Vec<TextureHandle>)>,
    vr: Option<(u64, Vec<Option<TextureHandle>>)>,
}

/// Gray image of one slice: samples, scaling, width and height.
struct Plane<'a> {
    data: &'a [i16],
    q: Quant,
    w: usize,
    h: usize,
}

impl SlycedApp {
    /// The image the highlight previews are made from: the active stack image,
    /// or the axial slice of the volume at the MPR point.
    fn preview_plane<'a>(&self, series: &'a LoadedSeries) -> Option<(Plane<'a>, usize)> {
        let view = self.active_view()?;
        if let Content::Stack(_) = view.content {
            let k = view.slice.min(series.len().saturating_sub(1));
            let im = series.images.get(k)?;
            if let Some(Slice::Gray(data, q)) = series.slice(k) {
                return Some((Plane { data, q, w: im.width, h: im.height }, k));
            }
        }
        let vol = series.volume.as_ref()?;
        let point = match self.workspace {
            Workspace::Compare => self.cmp.mpr[0].point,
            _ => self.mpr.point,
        };
        let [nx, ny, nz] = vol.dims;
        let z = (vol.patient_to_voxel(point).z.round().max(0.0) as usize).min(nz.saturating_sub(1));
        Some((Plane { data: &vol.data[z * nx * ny..(z + 1) * nx * ny], q: vol.quant, w: nx, h: ny }, 100_000 + z))
    }

    /// Previews of gordura, gás and cálculos on the current image (cached per image).
    fn special_previews(&mut self, ctx: &egui::Context) -> Option<Vec<TextureHandle>> {
        let series = self.active_series()?;
        if series.info.modality != "CT" {
            return None;
        }
        let window = self.active_view().map(|v| v.restore.map(|r| r.0).unwrap_or(v.window)).unwrap_or(series.default_window);
        let (plane, k) = self.preview_plane(&series)?;
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (Arc::as_ptr(&series) as usize, k, self.dense_threshold.to_bits(), window.center.to_bits(), window.width.to_bits()).hash(&mut h);
            h.finish()
        };
        if let Some((cached, tex)) = &self.previews.specials
            && *cached == key {
                return Some(tex.clone());
            }
        let tex: Vec<TextureHandle> = Special::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (win, invert, set) = match s {
                    Special::Fat => (Window::new(-50.0, 250.0), false, Highlights { fat: true, ..Default::default() }),
                    Special::Gas => (Window::new(-600.0, 1000.0), true, Highlights { gas: true, ..Default::default() }),
                    Special::Dense => (window, false, Highlights { dense: Some(self.dense_threshold), ..Default::default() }),
                };
                ctx.load_texture(format!("special-preview-{i}"), special_image(&plane, win, invert, &set), TextureOptions::LINEAR)
            })
            .collect();
        self.previews.specials = Some((key, tex.clone()));
        Some(tex)
    }

    /// Three clickable previews (gordura, gás, cálculos); false when there is no image to show.
    pub(crate) fn special_preview_row(&mut self, ui: &mut Ui) -> bool {
        let Some(tex) = self.special_previews(ui.ctx()) else { return false };
        let gap = 6.0;
        let w = ((ui.available_width() - gap * 2.0) / 3.0).clamp(70.0, 130.0);
        let mut toggle = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (s, t) in Special::ALL.into_iter().zip(&tex) {
                let on = self.special_on(s);
                let img_h = w * t.size()[1] as f32 / t.size()[0].max(1) as f32;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, img_h.min(w) + 22.0), Sense::click());
                let p = ui.painter();
                let img = Rect::from_min_size(rect.min, vec2(w, img_h.min(w)));
                p.rect_filled(img, CornerRadius::same(7), Color32::BLACK);
                p.image(t.id(), img.shrink(1.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                let border = if on { pal().accent } else if resp.hovered() { pal().border_strong } else { pal().border };
                p.rect_stroke(img, CornerRadius::same(7), Stroke::new(if on { 2.0 } else { 1.0 }, border), StrokeKind::Inside);
                let label = format!("{}  {}", s.icon(), s.short());
                let color = if on { pal().accent } else if resp.hovered() { pal().strong } else { pal().text };
                p.text(pos2(rect.center().x, img.bottom() + 11.0), Align2::CENTER_CENTER, label, font(11.5), color);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.on_hover_text(s.help()).clicked() {
                    toggle = Some(s);
                }
            }
        });
        if let Some(s) = toggle {
            self.toggle_special(s);
        }
        true
    }

    /// Low-resolution front views of the volume in every 3D preset (cached per volume).
    pub(crate) fn vr_previews(&mut self, ctx: &egui::Context, series: &LoadedSeries) -> Vec<Option<TextureHandle>> {
        let Some(vol) = series.volume.as_ref() else { return Vec::new() };
        if let Some((id, tex)) = &self.previews.vr
            && *id == vol.id {
                return tex.clone();
            }
        let ct = series.info.modality == "CT";
        let tex: Vec<Option<TextureHandle>> = tf::PRESETS
            .iter()
            .enumerate()
            .map(|(i, p)| (!p.ct || ct).then(|| ctx.load_texture(format!("vr-preview-{i}"), vr_image(vol, p), TextureOptions::LINEAR)))
            .collect();
        self.previews.vr = Some((vol.id, tex.clone()));
        tex
    }
}

/// The slice in `window`, downsampled, with the highlight colors on top.
fn special_image(plane: &Plane, window: Window, invert: bool, set: &Highlights) -> ColorImage {
    let step = (plane.w as f32 / SPECIAL_W as f32).max(1.0);
    let (tw, th) = (((plane.w as f32 / step) as usize).max(1), ((plane.h as f32 / step) as usize).max(1));
    let overlay = highlight::overlay(plane.data, plane.q, plane.w, plane.h, set);
    let (lo, span) = (window.center - window.width * 0.5, window.width.max(1.0));
    let mut pixels = Vec::with_capacity(tw * th);
    for ty in 0..th {
        for tx in 0..tw {
            let (x, y) = (((tx as f32 + 0.5) * step) as usize, ((ty as f32 + 0.5) * step) as usize);
            let i = y.min(plane.h - 1) * plane.w + x.min(plane.w - 1);
            let s = plane.data[i];
            let mut g = if s == OUTSIDE { 0.0 } else { ((plane.q.real(s) - lo) / span).clamp(0.0, 1.0) };
            if invert {
                g = 1.0 - g;
            }
            let base = (g * 255.0) as u8;
            let c = match overlay.as_ref().map(|o| o[i]) {
                Some(o) if o.a() > 0 => {
                    let a = o.a() as f32 / 255.0;
                    let mix = |c: u8| (c as f32 + base as f32 * (1.0 - a)).min(255.0) as u8;
                    Color32::from_rgb(mix(o.r()), mix(o.g()), mix(o.b()))
                }
                _ => Color32::from_gray(base),
            };
            pixels.push(c);
        }
    }
    ColorImage::new([tw, th], pixels)
}

/// Front (anterior) view of the volume in a preset: rays from the front, composited.
fn vr_image(vol: &Volume, preset: &tf::VrPreset) -> ColorImage {
    let range = vol.real_range();
    let (table, lo, hi) = tf::bake(preset, TfAdjust::default(), range);
    let [nx, ny, nz] = vol.dims;
    let (wx, wz) = (nx as f64 * vol.spacing.x, nz as f64 * vol.spacing.z);
    let scale = VR_SIZE as f64 / wx.max(wz);
    let (w, h) = (((wx * scale) as usize).clamp(8, VR_SIZE), ((wz * scale) as usize).clamp(8, VR_SIZE));
    // Rays go from anterior to posterior; the head is at the top.
    let ap = vol.axes[1].y >= 0.0;
    let up = vol.axes[2].z >= 0.0;
    let steps = ny.min(160);
    let step_mm = ny as f64 * vol.spacing.y / steps as f64;
    let correction = (step_mm / 1.5) as f32;
    let mut pixels = vec![Color32::TRANSPARENT; w * h];
    for py in 0..h {
        let zf = (py as f64 + 0.5) / h as f64 * nz as f64;
        let z = if up { nz - 1 - (zf as usize).min(nz - 1) } else { (zf as usize).min(nz - 1) };
        for px in 0..w {
            let x = (((px as f64 + 0.5) / w as f64 * nx as f64) as usize).min(nx - 1);
            let (mut rgb, mut alpha) = ([0.0f32; 3], 0.0f32);
            for i in 0..steps {
                let yi = ((i as f64 + 0.5) / steps as f64 * ny as f64) as usize;
                let y = if ap { yi.min(ny - 1) } else { ny - 1 - yi.min(ny - 1) };
                let s = vol.data[(z * ny + y) * nx + x];
                if s == OUTSIDE {
                    continue;
                }
                let v = vol.quant.real(s);
                let t = (((v - lo) / (hi - lo).max(1e-3)) * (TF_SIZE - 1) as f32).clamp(0.0, (TF_SIZE - 1) as f32) as usize;
                let a0 = table[t * 4 + 3].to_f32();
                if a0 <= 0.001 {
                    continue;
                }
                let a = 1.0 - (1.0 - a0.min(0.999)).powf(correction);
                // Slight depth cue: deeper samples are darker.
                let shade = 1.0 - 0.45 * i as f32 / steps as f32;
                for c in 0..3 {
                    rgb[c] += (1.0 - alpha) * a * table[t * 4 + c].to_f32() * shade;
                }
                alpha += (1.0 - alpha) * a;
                if alpha > 0.97 {
                    break;
                }
            }
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
            pixels[py * w + px] = Color32::from_rgba_premultiplied(c(rgb[0]), c(rgb[1]), c(rgb[2]), c(alpha));
        }
    }
    ColorImage::new([w, h], pixels)
}

/// Front projection of the volume, like a scout, head at the top (localizer of
/// axial images). CT is projected like a radiograph: attenuation summed along each
/// anterior–posterior line, with bone weighted up. Part of a wide blur is then taken
/// out (the slow change of body thickness), as digital radiographs are processed, so
/// bones, air spaces and contours stand out instead of a white silhouette.
pub(crate) fn scout_image(vol: &Volume) -> ColorImage {
    let [nx, ny, nz] = vol.dims;
    let (wx, wz) = (nx as f64 * vol.spacing.x, nz as f64 * vol.spacing.z);
    let h = 180usize;
    let w = ((h as f64 * wx / wz.max(1.0)) as usize).clamp(40, 360);
    let up = vol.axes[2].z >= 0.0;
    let ct = vol.real_range().0 < -500.0;
    let ystep = (ny / 128).max(1);
    let mut means = Vec::with_capacity(w * h);
    for py in 0..h {
        let zi = (((py as f64 + 0.5) / h as f64 * nz as f64) as usize).min(nz - 1);
        let z = if up { nz - 1 - zi } else { zi };
        for qx in 0..w {
            let x = (((qx as f64 + 0.5) / w as f64 * nx as f64) as usize).min(nx - 1);
            let (mut sum, mut n) = (0.0f32, 0u32);
            for y in (0..ny).step_by(ystep) {
                let s = vol.data[(z * ny + y) * nx + x];
                if s != OUTSIDE {
                    let v = vol.quant.real(s);
                    sum += if ct { ((v + 1000.0) / 1000.0).max(0.0) + ((v - 150.0) / 1000.0).max(0.0) * 1.5 } else { v };
                    n += 1;
                }
            }
            means.push(if n > 0 { sum / n as f32 } else { f32::NAN });
        }
    }
    // Lines that only cross air (or nothing) stay black.
    let body = |v: f32| v.is_finite() && (!ct || v > 0.03);
    let filled: Vec<f32> = means.iter().map(|v| if body(*v) { *v } else { 0.0 }).collect();
    let base = box_blur(&filled, w, h, (w.max(h) / 10).max(2));
    let detail: Vec<f32> = filled.iter().zip(&base).map(|(v, b)| v - 0.6 * b).collect();
    // Contrast from the spread of the values inside the body (works for CT and MR alike).
    let mut sorted: Vec<f32> = detail.iter().zip(&means).filter(|(_, m)| body(**m)).map(|(d, _)| *d).collect();
    sorted.sort_by(f32::total_cmp);
    let pick = |q: f32| sorted.get(((sorted.len().saturating_sub(1)) as f32 * q) as usize).copied().unwrap_or(0.0);
    let (lo, hi) = (pick(0.01), pick(0.995));
    let span = (hi - lo).max(1e-6);
    let pixels = detail
        .iter()
        .zip(&means)
        .map(|(d, m)| if body(*m) { Color32::from_gray((((d - lo) / span).clamp(0.0, 1.0).powf(1.1) * 255.0) as u8) } else { Color32::BLACK })
        .collect();
    ColorImage::new([w, h], pixels)
}

/// Two passes of a box blur of radius `r` (close to a gaussian).
fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut a = src.to_vec();
    let mut b = vec![0.0f32; src.len()];
    for _ in 0..2 {
        for y in 0..h {
            let row = &a[y * w..(y + 1) * w];
            for x in 0..w {
                let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
                b[y * w + x] = row[x0..=x1].iter().sum::<f32>() / (x1 - x0 + 1) as f32;
            }
        }
        for x in 0..w {
            for y in 0..h {
                let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
                a[y * w + x] = (y0..=y1).map(|k| b[k * w + x]).sum::<f32>() / (y1 - y0 + 1) as f32;
            }
        }
    }
    a
}

/// Gallery card of a 3D preset with its preview of the open volume.
pub(crate) fn preset_preview_card(ui: &mut Ui, name: &str, tex: &TextureHandle, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(124.0, 112.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(9), if resp.hovered() { pal().bg_hover } else { pal().bg_input });
    let area = Rect::from_min_max(rect.min + vec2(6.0, 6.0), pos2(rect.right() - 6.0, rect.bottom() - 26.0));
    p.rect_filled(area, CornerRadius::same(6), Color32::BLACK);
    let size = tex.size_vec2();
    let k = (area.width() / size.x).min(area.height() / size.y);
    let img = Rect::from_center_size(area.center(), size * k);
    p.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    p.text(pos2(rect.center().x, rect.bottom() - 13.0), Align2::CENTER_CENTER, name, font(12.5), if selected { pal().strong } else { pal().text_dim });
    p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { pal().accent } else { pal().border }), StrokeKind::Inside);
    resp
}
