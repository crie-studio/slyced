//! Incisional hernia calculator: hernia sac and abdominal cavity volumes
//! (ellipsoid from three diameters), Tanaka index, Sabbagh ratio and the EHS
//! width class of the fascial defect.

use super::icons;
use super::theme::*;
use super::viewport::{MeasureKind, Tool, Viewport};
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use crate::i18n::{num, Tr};
use egui::{Align, RichText, Ui};

const SAC: Tr = Tr("Saco", "Sac");
const CAVITY: Tr = Tr("Cavidade", "Cavity");

/// Measured diameters (mm): sac T/AP/CC, cavity T/AP/CC, defect width.
const SLOTS: [(Tr, Tr); 7] = [
    (SAC, Tr("transverso", "transverse")),
    (SAC, Tr("anteroposterior", "anteroposterior")),
    (SAC, Tr("craniocaudal", "craniocaudal")),
    (CAVITY, Tr("transverso", "transverse")),
    (CAVITY, Tr("anteroposterior", "anteroposterior")),
    (CAVITY, Tr("craniocaudal", "craniocaudal")),
    (Tr("Defeito", "Defect"), Tr("largura", "width")),
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
        let mut lines = vec![tr!("Hérnia incisional — volumetria por TC (elipsoide)", "Incisional hernia — CT volumetry (ellipsoid)").to_string()];
        let d = |i: usize| self.mm[i].map(|v| num(v as f64 / 10.0, 1)).unwrap_or_else(|| "—".into());
        let ml = |v: Option<f64>| v.map(|v| format!(" = {v:.0} mL")).unwrap_or_default();
        lines.push(trf!("Saco herniário: {} × {} × {} cm{}", "Hernia sac: {} × {} × {} cm{}", d(0), d(1), d(2), ml(sac)));
        lines.push(trf!("Cavidade abdominal: {} × {} × {} cm{}", "Abdominal cavity: {} × {} × {} cm{}", d(3), d(4), d(5), ml(cav)));
        if let Some(w) = self.mm[6] {
            lines.push(trf!("Defeito aponeurótico: {} cm ({})", "Fascial defect: {} cm ({})", num(w as f64 / 10.0, 1), ehs_width(w)));
        }
        if let (Some(s), Some(c)) = (sac, cav) {
            lines.push(trf!("Índice de Tanaka (VSH/VCA): {}%", "Tanaka index (HSV/ACV): {}%", num(s / c * 100.0, 1)));
            lines.push(trf!("Razão de Sabbagh (VSH/(VSH+VCA)): {}%", "Sabbagh ratio (HSV/(HSV+ACV)): {}%", num(s / (s + c) * 100.0, 1)));
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
        self.toast(ToastKind::Info, format!("{group} · {label}: {} cm", num(value as f64 / 10.0, 1)));
    }

    pub(crate) fn inspector_hernia(&mut self, ui: &mut Ui) {
        if !self.hernia.open {
            return;
        }
        ui.horizontal(|ui| {
            caption(ui, tr!("Hérnia incisional (Tanaka)", "Incisional hernia (Tanaka)"));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if icon_button_sized(ui, icons::X, tr!("Fechar a calculadora", "Close the calculator"), false, 22.0).clicked() {
                    self.hernia.open = false;
                    self.hernia.measuring = None;
                }
            });
        });
        let mut measure = None;
        card(ui, |ui| {
            ui.label(
                RichText::new(tr!(
                    "Meça três diâmetros do saco e da cavidade abdominal (transverso e AP no axial, craniocaudal no sagital) e a largura do defeito. Clique na régua ao lado do campo e desenhe a distância na imagem, ou digite o valor.",
                    "Measure three diameters of the hernia sac and of the abdominal cavity (transverse and AP on the axial, craniocaudal on the sagittal) and the width of the defect. Click the ruler next to the field and draw the distance on the image, or type the value."
                ))
                .size(11.5)
                .color(pal().text_faint),
            );
            let groups = [
                (tr!("Saco herniário", "Hernia sac"), 0..3),
                (tr!("Cavidade abdominal", "Abdominal cavity"), 3..6),
                (tr!("Defeito aponeurótico", "Fascial defect"), 6..7),
            ];
            for (title, range) in groups {
                ui.add_space(6.0);
                ui.label(RichText::new(title).font(font_bold(12.5)).color(pal().text));
                for i in range {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(SLOTS[i].1.get()).size(12.0).color(pal().text_dim));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let waiting = self.hernia.measuring.is_some_and(|m| m.0 == i);
                            let tip =
                                if waiting { tr!("Desenhe a distância na imagem", "Draw the distance on the image") } else { tr!("Medir na imagem", "Measure on the image") };
                            if icon_button_sized(ui, icons::RULER, tip, waiting, 24.0).clicked() {
                                measure = Some(i);
                            }
                            ui.label(RichText::new("cm").size(11.5).color(pal().text_faint));
                            let mut cm = self.hernia.mm[i].map(|v| v / 10.0).unwrap_or(0.0);
                            let r = ui.add(
                                egui::DragValue::new(&mut cm)
                                    .speed(0.1)
                                    .range(0.0..=80.0)
                                    .max_decimals(1)
                                    .custom_formatter(|v, _| if v <= 0.0 { "—".into() } else { num(v, 1) })
                                    .custom_parser(crate::i18n::parse_num),
                            );
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
            kv(ui, tr!("Volume do saco (VSH)", "Sac volume (HSV)"), &vol(sac));
            kv(ui, tr!("Volume da cavidade (VCA)", "Cavity volume (ACV)"), &vol(cav));
            if let Some(w) = self.hernia.mm[6] {
                kv(ui, tr!("Largura (EHS)", "Width (EHS)"), ehs_width(w));
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
                            ui.label(RichText::new(format!("{}%", num(v, 1))).font(font_bold(14.0)).color(if high { DANGER } else { SUCCESS }));
                        });
                    });
                };
                row(ui, tr!("Índice de Tanaka (≥ 25%)", "Tanaka index (≥ 25%)"), tanaka, 25.0);
                row(ui, tr!("Razão de Sabbagh (≥ 20%)", "Sabbagh ratio (≥ 20%)"), sabbagh, 20.0);
                let (icon, color, text) = if tanaka >= 25.0 || sabbagh >= 20.0 {
                    (
                        icons::WARNING,
                        WARNING,
                        tr!(
                            "Risco de perda de domicílio: considere preparo pré-operatório (toxina botulínica, pneumoperitônio progressivo) e separação de componentes.",
                            "Risk of loss of domain: consider preoperative preparation (botulinum toxin, progressive pneumoperitoneum) and component separation."
                        ),
                    )
                } else {
                    (
                        icons::CHECK_CIRCLE,
                        SUCCESS,
                        tr!(
                            "Abaixo dos limiares de perda de domicílio: fechamento primário do defeito tende a ser viável.",
                            "Below the loss-of-domain thresholds: primary closure of the defect is likely to be feasible."
                        ),
                    )
                };
                ui.label(RichText::new(format!("{icon}  {text}")).size(12.0).color(color));
            }
            ui.add_space(6.0);
            let note = tr!(
                "Volumes pelo elipsoide (π/6 × A × B × C). Apoio à decisão: confirme com a avaliação clínica.",
                "Ellipsoid volumes (π/6 × A × B × C). Decision support: confirm with clinical assessment."
            );
            ui.label(RichText::new(note).size(11.0).color(pal().text_faint));
            ui.horizontal(|ui| {
                if ui.button(trf!("{}  Copiar resumo", "{}  Copy summary", icons::COPY)).clicked() {
                    ui.ctx().copy_text(self.hernia.summary());
                    self.toast(ToastKind::Success, tr!("Resumo copiado.", "Summary copied."));
                }
                if ui.button(trf!("{}  Limpar", "{}  Clear", icons::TRASH)).clicked() {
                    self.hernia.mm = [None; 7];
                }
            });
        });
        if let Some(i) = measure
            && let Some(v) = self.active_view() {
                self.hernia.measuring = Some((i, v.id, v.measurements.len()));
                self.tool = Tool::Length;
                let (group, label) = SLOTS[i];
                self.toast(ToastKind::Info, trf!("Desenhe a distância: {group} {label}.", "Draw the distance: {group} {label}."));
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
