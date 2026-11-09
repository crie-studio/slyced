//! Volume of any structure from three perpendicular diameters (ellipsoid,
//! π/6 · a · b · c), with the mean density inside it when the series forms a
//! volume. The diameters are drawn one after another with the distance tool.

use super::hernia::ellipsoid_ml;
use super::icons;
use super::theme::*;
use super::viewport::{MeasureKind, Tool, Viewport};
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use crate::dicom::series::LoadedSeries;
use egui::{Align, RichText, Ui};
use glam::DVec3;
use std::sync::Arc;

const NAMES: [&str; 3] = ["D1 · largura", "D2 · profundidade", "D3 · altura"];

/// One measured diameter: length (mm) and its end points in patient space.
#[derive(Clone, Copy)]
pub struct Diameter {
    pub mm: f32,
    pub ends: Option<(DVec3, DVec3)>,
}

#[derive(Default)]
pub struct EllipsoidCalc {
    pub open: bool,
    pub d: [Option<Diameter>; 3],
    /// Diameter waiting for the next distance drawn in any view.
    pub measuring: Option<usize>,
    /// Measurements per view when the wait began: `(view id, count)`.
    counts: Vec<(u64, usize)>,
    /// Mean density inside the ellipsoid.
    pub mean: Option<f32>,
    series: Option<Arc<LoadedSeries>>,
}

impl EllipsoidCalc {
    pub fn volume_ml(&self) -> Option<f64> {
        let [a, b, c] = self.d.map(|d| d.map(|d| d.mm).filter(|v| *v > 0.0));
        Some(ellipsoid_ml([a?, b?, c?]))
    }

    pub fn summary(&self) -> String {
        let cm = |i: usize| self.d[i].map(|d| format!("{:.1}", d.mm / 10.0)).unwrap_or_else(|| "—".into());
        let mut s = format!("Volume (elipsoide): {} × {} × {} cm", cm(0), cm(1), cm(2));
        if let Some(v) = self.volume_ml() {
            s += &format!(" = {v:.1} mL");
        }
        if let Some(m) = self.mean {
            s += &format!("; densidade média {m:.0} HU");
        }
        s
    }
}

/// Center and semi-axes of the ellipsoid spanned by three diameters (the
/// second and third are made perpendicular to the ones before).
pub fn ellipsoid_axes(ends: [(DVec3, DVec3); 3]) -> Option<(DVec3, [DVec3; 3])> {
    let center = ends.iter().map(|(a, b)| (*a + *b) * 0.5).sum::<DVec3>() / 3.0;
    let [d1, d2, d3] = ends.map(|(a, b)| b - a);
    let u1 = d1.try_normalize()?;
    let u2 = (d2 - u1 * d2.dot(u1)).try_normalize()?;
    let u3 = u1.cross(u2);
    Some((center, [u1 * d1.length() * 0.5, u2 * d2.length() * 0.5, u3 * d3.length() * 0.5]))
}

impl SlycedApp {
    /// Opens the calculator and starts the first diameter.
    pub(crate) fn open_ellipsoid(&mut self) {
        self.ellipsoid.open = true;
        self.right_open = true;
        let next = self.ellipsoid.d.iter().position(|d| d.is_none()).unwrap_or(0);
        self.start_diameter(next);
    }

    fn all_views_mut(&mut self) -> impl Iterator<Item = &mut Viewport> {
        self.views.iter_mut().chain(self.mpr_views.iter_mut()).chain(self.cmp_views.iter_mut())
    }

    fn start_diameter(&mut self, slot: usize) {
        if let Some(series) = self.active_view().and_then(|v| v.content.series().cloned()) {
            self.ellipsoid.series = Some(series);
        }
        self.ellipsoid.counts = self.all_views_mut().map(|v| (v.id, v.measurements.len())).collect();
        self.ellipsoid.measuring = Some(slot);
        self.tool = Tool::Length;
        self.toast(ToastKind::Info, format!("Desenhe {} na imagem.", NAMES[slot]));
    }

    /// Takes the distance just drawn for the diameter being measured, then asks for the next one.
    pub(crate) fn ellipsoid_poll(&mut self) {
        let Some(slot) = self.ellipsoid.measuring else { return };
        let mut counts = std::mem::take(&mut self.ellipsoid.counts);
        let mut found = None;
        for v in self.all_views_mut() {
            let len = v.measurements.len();
            match counts.iter_mut().find(|c| c.0 == v.id) {
                // Measurements removed meanwhile (Del, Restaurar): wait for the next new one.
                Some(c) if len < c.1 => c.1 = len,
                Some(c) if len > c.1 && found.is_none() => found = Some(v.id),
                Some(_) => {}
                None => counts.push((v.id, len)),
            }
        }
        self.ellipsoid.counts = counts;
        let Some(id) = found else { return };
        let Some(v) = self.all_views_mut().find(|v| v.id == id) else { return };
        let index = v.measurements.len() - 1;
        let Some(mm) = v.values.iter().find(|m| m.index == index && m.kind == MeasureKind::Length).map(|m| m.value) else { return };
        let pts = v.measurements[index].pts.clone();
        let ends = match (v.pixel_patient(pts[0]), v.pixel_patient(pts[1])) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        };
        v.measurements[index].text = format!("D{}", slot + 1);
        self.ellipsoid.d[slot] = Some(Diameter { mm, ends });
        self.ellipsoid.measuring = None;
        match self.ellipsoid.d.iter().position(|d| d.is_none()) {
            Some(next) => self.start_diameter(next),
            None => {
                self.update_ellipsoid_mean();
                if let Some(ml) = self.ellipsoid.volume_ml() {
                    self.toast(ToastKind::Success, format!("Volume: {ml:.1} mL"));
                }
            }
        }
    }

    fn update_ellipsoid_mean(&mut self) {
        let ends: Option<Vec<(DVec3, DVec3)>> = self.ellipsoid.d.iter().map(|d| d.and_then(|d| d.ends)).collect();
        let vol = self.ellipsoid.series.as_ref().and_then(|s| s.volume.clone());
        self.ellipsoid.mean = match (ends, vol) {
            (Some(e), Some(vol)) => ellipsoid_axes([e[0], e[1], e[2]]).and_then(|(c, axes)| vol.mean_in_ellipsoid(c, axes)),
            _ => None,
        };
    }

    pub(crate) fn inspector_ellipsoid(&mut self, ui: &mut Ui) {
        if !self.ellipsoid.open {
            return;
        }
        ui.horizontal(|ui| {
            caption(ui, "Volume por 3 diâmetros");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if icon_button_sized(ui, icons::X, "Fechar", false, 22.0).clicked() {
                    self.ellipsoid.open = false;
                    self.ellipsoid.measuring = None;
                }
            });
        });
        let mut measure = None;
        card(ui, |ui| {
            ui.label(
                RichText::new("Meça os três maiores diâmetros perpendiculares: largura e profundidade no axial, altura no coronal ou sagital. Os diâmetros são pedidos um depois do outro.")
                    .size(11.5)
                    .color(pal().text_faint),
            );
            ui.add_space(4.0);
            for (i, name) in NAMES.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(*name).size(12.0).color(pal().text_dim));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let waiting = self.ellipsoid.measuring == Some(i);
                        if icon_button_sized(ui, icons::RULER, if waiting { "Desenhe a distância na imagem" } else { "Medir na imagem" }, waiting, 24.0).clicked() {
                            measure = Some(i);
                        }
                        ui.label(RichText::new("cm").size(11.5).color(pal().text_faint));
                        let mut cm = self.ellipsoid.d[i].map(|d| d.mm / 10.0).unwrap_or(0.0);
                        let r = ui.add(egui::DragValue::new(&mut cm).speed(0.1).range(0.0..=80.0).max_decimals(1).custom_formatter(|v, _| if v <= 0.0 { "—".into() } else { format!("{v:.1}") }));
                        if r.changed() {
                            // A typed value has no position: no mean density.
                            self.ellipsoid.d[i] = (cm > 0.0).then_some(Diameter { mm: cm * 10.0, ends: None });
                            self.ellipsoid.mean = None;
                        }
                    });
                });
            }
            ui.add_space(6.0);
            let vol = self.ellipsoid.volume_ml().map(|v| format!("{v:.1} mL")).unwrap_or_else(|| "—".into());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Volume").size(12.5).color(pal().text));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(vol).font(font_bold(15.0)).color(pal().accent));
                });
            });
            if let Some(m) = self.ellipsoid.mean {
                kv(ui, "Densidade média", &format!("{m:.0} HU"));
            }
            ui.label(RichText::new("Elipsoide: π/6 × D1 × D2 × D3.").size(11.0).color(pal().text_faint));
            ui.horizontal(|ui| {
                if ui.button(format!("{}  Copiar", icons::COPY)).clicked() {
                    ui.ctx().copy_text(self.ellipsoid.summary());
                    self.toast(ToastKind::Success, "Volume copiado.");
                }
                if ui.button(format!("{}  Nova medida", icons::ARROWS_CLOCKWISE)).clicked() {
                    self.ellipsoid.d = [None; 3];
                    self.ellipsoid.mean = None;
                    measure = Some(0);
                }
            });
        });
        if let Some(i) = measure {
            self.start_diameter(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_from_three_diameters() {
        // 40 mm along x, 30 mm along y (slightly tilted), 20 mm along z.
        let ends = [
            (DVec3::new(-20.0, 0.0, 0.0), DVec3::new(20.0, 0.0, 0.0)),
            (DVec3::new(1.0, -15.0, 0.0), DVec3::new(-1.0, 15.0, 0.0)),
            (DVec3::new(0.0, 0.0, -10.0), DVec3::new(0.0, 0.0, 10.0)),
        ];
        let (c, [a, b, z]) = ellipsoid_axes(ends).unwrap();
        assert!(c.length() < 1e-9);
        assert!((a.length() - 20.0).abs() < 1e-9 && a.dot(b).abs() < 1e-9 && a.dot(z).abs() < 1e-9);
        assert!((z.length() - 10.0).abs() < 1e-9);
        let calc = EllipsoidCalc { d: [40.0, 30.0, 20.0].map(|mm| Some(Diameter { mm, ends: None })), ..Default::default() };
        assert!((calc.volume_ml().unwrap() - 12.566).abs() < 0.01);
        assert!(calc.summary().contains("12.6 mL"));
    }
}
