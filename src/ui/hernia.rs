//! Incisional hernia calculator: hernia sac and abdominal cavity volumes
//! (ellipsoid from three diameters), Tanaka index, Sabbagh ratio and the EHS
//! width class of the fascial defect.

use super::icons;
use super::theme::*;
use super::viewport::{MeasureKind, Tool, Viewport};
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use egui::{Align, RichText, Ui};

/// Measured diameters (mm): sac T/AP/CC, cavity T/AP/CC, defect width.
const SLOTS: [(&str, &str); 7] = [
    ("Saco", "transverso"),
    ("Saco", "anteroposterior"),
    ("Saco", "craniocaudal"),
    ("Cavidade", "transverso"),
    ("Cavidade", "anteroposterior"),
    ("Cavidade", "craniocaudal"),
    ("Defeito", "largura"),
];

#[derive(Default)]
pub struct Hernia {
    pub open: bool,
    pub mm: [Option<f32>; 7],
    /// Slot waiting for the next distance measurement: `(slot, view id, measurements before)`.
    pub measuring: Option<(usize, u64, usize)>,
}

/// Ellipsoid volume (mL) from three diameters in mm: π/6 · a · b · c.
pub fn ellipsoid_ml(d: [f32; 3]) -> f64 {
    std::f64::consts::PI / 6.0 * d.iter().map(|&x| x as f64).product::<f64>() / 1000.0
}

/// European Hernia Society width class of the defect.
pub fn ehs_width(mm: f32) -> &'static str {
    if mm < 40.0 {
        "W1 (< 4 cm)"
    } else if mm <= 100.0 {
        "W2 (4–10 cm)"
    } else {
        "W3 (> 10 cm)"
    }
}

impl Hernia {
    fn volumes(&self) -> (Option<f64>, Option<f64>) {
        let vol = |r: std::ops::Range<usize>| -> Option<f64> {
            let d: Vec<f32> = self.mm[r].iter().map(|v| v.filter(|x| *x > 0.0)).collect::<Option<Vec<_>>>()?;
            Some(ellipsoid_ml([d[0], d[1], d[2]]))
        };
        (vol(0..3), vol(3..6))
    }

    /// Text summary for the report.
    pub fn summary(&self) -> String {
        let (sac, cav) = self.volumes();
        let mut lines = vec!["Hérnia incisional — volumetria por TC (elipsoide)".to_string()];
        let d = |i: usize| self.mm[i].map(|v| format!("{:.1}", v / 10.0)).unwrap_or_else(|| "—".into());
        lines.push(format!("Saco herniário: {} × {} × {} cm{}", d(0), d(1), d(2), sac.map(|v| format!(" = {v:.0} mL")).unwrap_or_default()));
        lines.push(format!("Cavidade abdominal: {} × {} × {} cm{}", d(3), d(4), d(5), cav.map(|v| format!(" = {v:.0} mL")).unwrap_or_default()));
        if let Some(w) = self.mm[6] {
            lines.push(format!("Defeito aponeurótico: {:.1} cm ({})", w / 10.0, ehs_width(w)));
        }
        if let (Some(s), Some(c)) = (sac, cav) {
            lines.push(format!("Índice de Tanaka (VSH/VCA): {:.1}%", s / c * 100.0));
            lines.push(format!("Razão de Sabbagh (VSH/(VSH+VCA)): {:.1}%", s / (s + c) * 100.0));
        }
        lines.join("\n")
    }
}

impl SlycedApp {
    /// Opens the calculator next to an MPR of the active CT (axial and sagittal planes for the diameters).
    pub(crate) fn open_hernia(&mut self) {
        self.hernia.open = true;
        self.right_open = true;
        if let Some(s) = self.active_series().filter(|s| s.volume.is_some()) {
            if self.workspace != crate::app::Workspace::Mpr {
                self.open_series(&s.uid.clone(), crate::app::Target::Mpr);
            }
            let w = crate::imaging::lut::Window::new(50.0, 400.0);
            for v in self.mpr_views.iter_mut().take(3) {
                v.window = w;
            }
        }
        self.tool = Tool::Length;
    }

    /// Takes the distance just drawn for the slot being measured.
    pub(crate) fn hernia_poll(&mut self) {
        let Some((slot, view_id, before)) = self.hernia.measuring else { return };
        let views = self.views.iter_mut().chain(self.mpr_views.iter_mut()).chain(self.cmp_views.iter_mut());
        let Some(v): Option<&mut Viewport> = views.into_iter().find(|v| v.id == view_id) else {
            self.hernia.measuring = None;
            return;
        };
        if v.measurements.len() <= before {
            // Measurements removed meanwhile (Del, Restaurar): wait for the next new one.
            if v.measurements.len() < before {
                self.hernia.measuring = Some((slot, view_id, v.measurements.len()));
            }
            return;
        }
        let index = v.measurements.len() - 1;
        let Some(value) = v.values.iter().find(|m| m.index == index && m.kind == MeasureKind::Length).map(|m| m.value) else { return };
        let (group, label) = SLOTS[slot];
        v.measurements[index].text = format!("{group} {label}");
        self.hernia.mm[slot] = Some(value);
        self.hernia.measuring = None;
        self.toast(ToastKind::Info, format!("{group} · {label}: {:.1} cm", value / 10.0));
    }

    pub(crate) fn inspector_hernia(&mut self, ui: &mut Ui) {
        if !self.hernia.open {
            return;
        }
        ui.horizontal(|ui| {
            caption(ui, "Hérnia incisional (Tanaka)");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if icon_button_sized(ui, icons::X, "Fechar a calculadora", false, 22.0).clicked() {
                    self.hernia.open = false;
                    self.hernia.measuring = None;
                }
            });
        });
        let mut measure = None;
        card(ui, |ui| {
            ui.label(
                RichText::new("Meça três diâmetros do saco e da cavidade abdominal (transverso e AP no axial, craniocaudal no sagital) e a largura do defeito. Clique na régua ao lado do campo e desenhe a distância na imagem, ou digite o valor.")
                    .size(11.5)
                    .color(pal().text_faint),
            );
            for (title, range) in [("Saco herniário", 0..3), ("Cavidade abdominal", 3..6), ("Defeito aponeurótico", 6..7)] {
                ui.add_space(6.0);
                ui.label(RichText::new(title).font(font_bold(12.5)).color(pal().text));
                for i in range {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(SLOTS[i].1).size(12.0).color(pal().text_dim));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let waiting = self.hernia.measuring.is_some_and(|m| m.0 == i);
                            if icon_button_sized(ui, icons::RULER, if waiting { "Desenhe a distância na imagem" } else { "Medir na imagem" }, waiting, 24.0).clicked() {
                                measure = Some(i);
                            }
                            ui.label(RichText::new("cm").size(11.5).color(pal().text_faint));
                            let mut cm = self.hernia.mm[i].map(|v| v / 10.0).unwrap_or(0.0);
                            let r = ui.add(egui::DragValue::new(&mut cm).speed(0.1).range(0.0..=80.0).max_decimals(1).custom_formatter(|v, _| if v <= 0.0 { "—".into() } else { format!("{v:.1}") }));
                            if r.changed() {
                                self.hernia.mm[i] = (cm > 0.0).then_some(cm * 10.0);
                            }
                        });
                    });
                }
            }
            ui.add_space(8.0);
            let (sac, cav) = self.hernia.volumes();
            let vol = |v: Option<f64>| v.map(|v| format!("{v:.0} mL")).unwrap_or_else(|| "—".into());
            kv(ui, "Volume do saco (VSH)", &vol(sac));
            kv(ui, "Volume da cavidade (VCA)", &vol(cav));
            if let Some(w) = self.hernia.mm[6] {
                kv(ui, "Largura (EHS)", ehs_width(w));
            }
            if let (Some(s), Some(c)) = (sac, cav) {
                let tanaka = s / c * 100.0;
                let sabbagh = s / (s + c) * 100.0;
                ui.add_space(4.0);
                let row = |ui: &mut Ui, name: &str, v: f64, limit: f64| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).size(12.5).color(pal().text));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let high = v >= limit;
                            ui.label(RichText::new(format!("{v:.1}%")).font(font_bold(14.0)).color(if high { DANGER } else { SUCCESS }));
                        });
                    });
                };
                row(ui, "Índice de Tanaka (≥ 25%)", tanaka, 25.0);
                row(ui, "Razão de Sabbagh (≥ 20%)", sabbagh, 20.0);
                let (icon, color, text) = if tanaka >= 25.0 || sabbagh >= 20.0 {
                    (
                        icons::WARNING,
                        WARNING,
                        "Risco de perda de domicílio: considere preparo pré-operatório (toxina botulínica, pneumoperitônio progressivo) e separação de componentes.",
                    )
                } else {
                    (icons::CHECK_CIRCLE, SUCCESS, "Abaixo dos limiares de perda de domicílio: fechamento primário do defeito tende a ser viável.")
                };
                ui.label(RichText::new(format!("{icon}  {text}")).size(12.0).color(color));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Volumes pelo elipsoide (π/6 × A × B × C). Apoio à decisão: confirme com a avaliação clínica.").size(11.0).color(pal().text_faint));
            ui.horizontal(|ui| {
                if ui.button(format!("{}  Copiar resumo", icons::COPY)).clicked() {
                    ui.ctx().copy_text(self.hernia.summary());
                    self.toast(ToastKind::Success, "Resumo copiado.");
                }
                if ui.button(format!("{}  Limpar", icons::TRASH)).clicked() {
                    self.hernia.mm = [None; 7];
                }
            });
        });
        if let Some(i) = measure
            && let Some(v) = self.active_view() {
                self.hernia.measuring = Some((i, v.id, v.measurements.len()));
                self.tool = Tool::Length;
                let (group, label) = SLOTS[i];
                self.toast(ToastKind::Info, format!("Desenhe a distância: {group} {label}."));
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tanaka_and_sabbagh() {
        let mut h = Hernia::default();
        // Sac 12 × 8 × 10 cm, cavity 30 × 20 × 25 cm.
        h.mm = [Some(120.0), Some(80.0), Some(100.0), Some(300.0), Some(200.0), Some(250.0), Some(85.0)];
        let (s, c) = h.volumes();
        let (s, c) = (s.unwrap(), c.unwrap());
        assert!((s - 502.65).abs() < 0.1, "{s}");
        assert!((c - 7853.98).abs() < 0.1, "{c}");
        assert!((s / c * 100.0 - 6.4).abs() < 0.01);
        assert_eq!(ehs_width(85.0), "W2 (4–10 cm)");
        assert!(h.summary().contains("Tanaka"));
        h.mm[2] = None;
        assert!(h.volumes().0.is_none());
    }
}
