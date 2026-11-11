//! Patient timeline: every exam of the patient, on this computer and on the
//! PACS, as dots on a line by date. Exams of the same body region (or the same
//! modality) as the one being read stand out, and a click compares them.

use super::theme::*;
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use crate::dicom::model::{format_date, relative_date, StudyHeader, StudyInfo};
use crate::i18n::Tr;
use crate::pacs::scu::StudyResult;
use egui::{pos2, vec2, Align2, Rect, RichText, Sense, Stroke, Ui};

/// The patient's exams on the PACS, asked once per patient.
pub enum Priors {
    Pending,
    Done(Vec<StudyResult>),
    Failed(String),
}

pub struct TimelineItem {
    pub date: String,
    pub description: String,
    pub modalities: Vec<String>,
    /// Study UID when the exam is on this computer.
    pub local: Option<String>,
    /// PACS entry when it is not.
    pub remote: Option<StudyResult>,
    /// The exam being read (or the newest one).
    pub current: bool,
    /// Why it matters for the current exam ("mesma região (tórax)").
    pub relevant: Option<String>,
    pub series: usize,
    pub images: usize,
}

pub enum TimelineAction {
    Compare(String),
    Open(String),
    /// Download from the PACS, then compare (`true`) or open.
    Retrieve(StudyResult, bool),
    Download(StudyResult),
}

/// Body regions recognized in descriptions and Body Part Examined.
const REGIONS: &[(Tr, &[&str])] = &[
    (Tr("crânio", "head"), &["cranio", "crânio", "head", "brain", "encefalo", "encéfalo", "cerebro", "cérebro", "skull", "sela", "orbita", "órbita", "orbitas", "órbitas", "face", "seios", "sinus", "mastoide", "mastoides", "iam"]),
    (Tr("pescoço", "neck"), &["pescoco", "pescoço", "neck", "tireoide", "tireóide", "laringe", "cervicais"]),
    (Tr("tórax", "chest"), &["torax", "tórax", "chest", "thorax", "pulmao", "pulmão", "pulmoes", "pulmões", "lung", "lungs", "mediastino", "tep", "pulmonar"]),
    (Tr("abdome", "abdomen"), &["abdome", "abdomen", "abdômen", "abd", "abdominal", "figado", "fígado", "liver", "renal", "rins", "rim", "kidney", "pancreas", "pâncreas", "urotc", "urotomografia", "enterotc", "biliar"]),
    (Tr("pelve", "pelvis"), &["pelve", "pelvis", "pélvis", "pelvic", "prostata", "próstata", "bexiga", "utero", "útero", "reto"]),
    (Tr("coluna", "spine"), &["coluna", "spine", "lombar", "lumbar", "lspine", "cspine", "tspine", "dorsal", "toracica", "torácica", "sacro", "sacral", "cervical"]),
    (Tr("membro superior", "upper limb"), &["ombro", "shoulder", "cotovelo", "elbow", "punho", "wrist", "mao", "mão", "hand", "braco", "braço", "umero", "úmero", "antebraco", "antebraço"]),
    (Tr("membro inferior", "lower limb"), &["joelho", "knee", "quadril", "hip", "tornozelo", "ankle", "foot", "femur", "fêmur", "perna", "tibia", "tíbia", "coxa"]),
    (Tr("mama", "breast"), &["mama", "mamas", "breast", "mamografia", "mammo", "mammography"]),
    (Tr("coração", "heart"), &["coracao", "coração", "cardiac", "heart", "coronaria", "coronária", "coronarias", "coronárias", "cardio", "cardiaca", "cardíaca", "calcium"]),
];

/// Image modalities (reports, presentation states and key objects are left out).
fn image_modalities(list: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in list {
        let m = m.trim().to_ascii_uppercase();
        if !m.is_empty() && !["SR", "PR", "KO", "OT", "SEG", "DOC", "REG"].contains(&m.as_str()) && !out.contains(&m) {
            out.push(m);
        }
    }
    out
}

/// Regions named in `text` (bit `i` = `REGIONS[i]`).
fn regions(text: &str) -> u16 {
    let lower = text.to_lowercase();
    let mut bits = 0;
    for word in lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        for (i, (_, words)) in REGIONS.iter().enumerate() {
            if words.contains(&word) {
                bits |= 1 << i;
            }
        }
    }
    bits
}

fn region_names(bits: u16) -> String {
    REGIONS.iter().enumerate().filter(|(i, _)| bits & (1 << i) != 0).map(|(_, r)| r.0.get()).collect::<Vec<_>>().join(", ")
}

/// Why `other` matters when reading `current`: the same body region or, when a
/// region is not known, the same modality.
fn relevance(current: (&str, &[String]), other: (&str, &[String])) -> Option<String> {
    let (a, b) = (regions(current.0), regions(other.0));
    if a & b != 0 {
        return Some(trf!("mesma região ({})", "same region ({})", region_names(a & b)));
    }
    if a != 0 && b != 0 {
        return None;
    }
    current.1.iter().find(|m| other.1.contains(m)).map(|m| trf!("mesma modalidade ({})", "same modality ({})", modality_name(m)))
}

fn modality_name(m: &str) -> &str {
    match m {
        "CT" => tr!("TC", "CT"),
        "MR" => tr!("RM", "MRI"),
        "US" => "US",
        "CR" | "DX" => tr!("RX", "X-ray"),
        "MG" => tr!("mamografia", "mammography"),
        "PT" => "PET",
        "NM" => tr!("medicina nuclear", "nuclear medicine"),
        _ => m,
    }
}

/// Text of an exam on this computer used to find its region.
fn study_text(st: &StudyInfo) -> String {
    let mut t = st.header.description.clone();
    for s in &st.series {
        t.push(' ');
        t.push_str(&s.description);
        t.push(' ');
        t.push_str(&s.body_part);
    }
    t
}

/// Letters of a name without separators or accents, for comparing names
/// written with different character sets ("JOÃO" and "JOAO" match).
fn name_key(name: &str) -> String {
    name.chars()
        .flat_map(char::to_uppercase)
        .map(|c| match c {
            'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            'Ñ' => 'N',
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// A PACS entry with the same patient ID belongs to the same person when the
/// birth date (or, without it, the name) also matches.
fn same_person(h: &StudyHeader, r: &StudyResult) -> bool {
    if !h.birth_date.trim().is_empty() && !r.birth_date.trim().is_empty() {
        return h.birth_date.trim() == r.birth_date.trim();
    }
    let (a, b) = (name_key(&h.patient_name), name_key(&r.patient_name));
    a.is_empty() || b.is_empty() || a == b
}

/// Days since 1970-01-01 of a `AAAAMMDD` date.
fn day_number(raw: &str) -> Option<i64> {
    let d = raw.trim();
    if d.len() < 8 || !d[..8].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, day) = (d[..4].parse::<i64>().ok()?, d[4..6].parse::<i64>().ok()?, d[6..8].parse::<i64>().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// "mar/2025" ("Mar 2025" in English).
fn month_year(raw: &str) -> String {
    const MONTHS: [Tr; 12] = [
        Tr("jan", "Jan"),
        Tr("fev", "Feb"),
        Tr("mar", "Mar"),
        Tr("abr", "Apr"),
        Tr("mai", "May"),
        Tr("jun", "Jun"),
        Tr("jul", "Jul"),
        Tr("ago", "Aug"),
        Tr("set", "Sep"),
        Tr("out", "Oct"),
        Tr("nov", "Nov"),
        Tr("dez", "Dec"),
    ];
    let d = raw.trim();
    match (d.get(..4), d.get(4..6).and_then(|m| m.parse::<usize>().ok())) {
        (Some(y), Some(m @ 1..=12)) => trf!("{}/{y}", "{} {y}", MONTHS[m - 1]),
        _ => String::new(),
    }
}

impl SlycedApp {
    /// Asks the PACS once for each patient's other exams (when turned on and a
    /// server is set up).
    pub(crate) fn request_priors(&mut self) {
        if !self.settings.timeline_pacs {
            return;
        }
        let Some(node) = self.settings.nodes.get(self.pacs.node).cloned() else { return };
        let mut ids: Vec<String> = Vec::new();
        for st in &self.library.studies {
            let id = st.header.patient_id.trim();
            if !id.is_empty() && !self.priors.contains_key(id) && !ids.iter().any(|i| i == id) {
                ids.push(id.to_string());
            }
        }
        for id in ids {
            self.priors.insert(id.clone(), Priors::Pending);
            self.jobs.find_priors(node.clone(), self.settings.local_ae.clone(), id);
        }
    }

    pub(crate) fn on_priors(&mut self, patient_id: String, result: Result<Vec<StudyResult>, String>) {
        let state = match result {
            Ok(list) => Priors::Done(list),
            Err(e) => {
                log::info!("linha do tempo: PACS indisponível para o paciente {patient_id}: {e}");
                Priors::Failed(e)
            }
        };
        self.priors.insert(patient_id, state);
    }

    /// The patient's exams, oldest first, for the studies `group` of the
    /// library (one patient, newest first). Also says whether the PACS answer
    /// is still awaited.
    pub(crate) fn timeline_items(&self, group: &[usize], current: &str) -> (Vec<TimelineItem>, bool) {
        let Some(first) = group.first().and_then(|&i| self.library.studies.get(i)) else { return (Vec::new(), false) };
        let mut items: Vec<(TimelineItem, String)> = group
            .iter()
            .filter_map(|&i| self.library.studies.get(i))
            .map(|st| {
                let item = TimelineItem {
                    date: st.header.date.clone(),
                    description: st.header.description.trim().to_string(),
                    modalities: image_modalities(st.modalities()),
                    local: Some(st.header.uid.clone()),
                    remote: None,
                    current: st.header.uid == current,
                    relevant: None,
                    series: st.series.len(),
                    images: st.image_count(),
                };
                (item, study_text(st))
            })
            .collect();
        let pending = match self.priors.get(first.header.patient_id.trim()) {
            Some(Priors::Done(list)) => {
                for r in list.iter().filter(|r| same_person(&first.header, r)) {
                    if items.iter().any(|(i, _)| i.local.as_deref() == Some(r.study_uid.as_str())) {
                        continue;
                    }
                    let item = TimelineItem {
                        date: r.date.clone(),
                        description: r.description.trim().to_string(),
                        modalities: image_modalities(r.modalities.split(['\\', ',', ' ']).map(String::from)),
                        local: None,
                        remote: Some(r.clone()),
                        current: false,
                        relevant: None,
                        series: r.series_count.unwrap_or(0).max(0) as usize,
                        images: r.instance_count.unwrap_or(0).max(0) as usize,
                    };
                    items.push((item, r.description.clone()));
                }
                false
            }
            Some(Priors::Pending) => true,
            _ => false,
        };
        if let Some(c) = items.iter().position(|(i, _)| i.current) {
            let (cur_text, cur_mods) = (items[c].1.clone(), items[c].0.modalities.clone());
            for (item, text) in items.iter_mut().filter(|(i, _)| !i.current) {
                item.relevant = relevance((&cur_text, &cur_mods), (text, &item.modalities));
            }
        }
        let mut out: Vec<TimelineItem> = items.into_iter().map(|(i, _)| i).collect();
        out.sort_by(|a, b| a.date.cmp(&b.date));
        (out, pending)
    }

    /// Downloads an exam from the timeline; when done it is compared with the
    /// exam being read (`compare`) or opened.
    pub(crate) fn retrieve_prior(&mut self, r: &StudyResult, then: Option<bool>) {
        let before = self.pacs.transfers.len();
        self.start_retrieve(r, None);
        let Some(t) = self.pacs.transfers.get_mut(before) else { return };
        t.open_when_done = then == Some(false);
        if then == Some(true) {
            self.compare_after_retrieve.insert(r.study_uid.clone());
        }
        let date = format_date(&r.date);
        let text = match then {
            Some(true) => trf!(
                "Baixando o exame de {date} do PACS; a comparação abre quando terminar.",
                "Downloading the {date} study from the PACS; the comparison opens when it's done."
            ),
            Some(false) => trf!(
                "Baixando o exame de {date} do PACS; ele abre quando terminar.",
                "Downloading the {date} study from the PACS; it opens when it's done."
            ),
            None => trf!("Baixando o exame de {date} do PACS.", "Downloading the {date} study from the PACS."),
        };
        self.toast(ToastKind::Info, text);
    }

    /// Exams downloaded from the timeline to be compared: done as soon as they are read.
    pub(crate) fn compare_downloaded(&mut self) {
        let ready: Vec<String> = self
            .compare_after_retrieve
            .iter()
            .filter(|uid| self.library.studies.iter().any(|s| &s.header.uid == *uid))
            .cloned()
            .collect();
        for uid in ready {
            self.compare_after_retrieve.remove(&uid);
            self.compare_with_study(&uid);
        }
    }

    pub(crate) fn timeline_action(&mut self, a: TimelineAction) {
        match a {
            TimelineAction::Compare(uid) => self.compare_with_study(&uid),
            TimelineAction::Open(uid) => self.open_local_study(&uid),
            TimelineAction::Retrieve(r, compare) => self.retrieve_prior(&r, Some(compare)),
            TimelineAction::Download(r) => self.retrieve_prior(&r, None),
        }
    }
}

/// Line with one dot per exam, spaced by date, inside `rect` (about 26 points
/// tall). `can_compare`: the exam being read is this patient's.
pub fn timeline_strip(ui: &mut Ui, rect: Rect, items: &[TimelineItem], pending: bool, can_compare: bool, time: f64) -> Option<TimelineAction> {
    let p = ui.painter().clone();
    let line_y = rect.top() + 8.0;
    let x0 = rect.left() + 10.0;
    let x1 = rect.right() - if pending { 22.0 } else { 10.0 };
    p.line_segment([pos2(x0 - 4.0, line_y), pos2(x1 + 4.0, line_y)], Stroke::new(1.0, pal().border_strong));
    if pending {
        spinner(&p, pos2(rect.right() - 9.0, line_y), 4.0, time, pal().text_faint);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    let days: Vec<Option<i64>> = items.iter().map(|i| day_number(&i.date)).collect();
    let known: Vec<i64> = days.iter().flatten().copied().collect();
    let (lo, hi) = (known.iter().copied().min().unwrap_or(0), known.iter().copied().max().unwrap_or(0));
    let mut xs: Vec<f32> = days
        .iter()
        .enumerate()
        .map(|(i, d)| match d {
            Some(d) if hi > lo => x0 + (*d - lo) as f32 / (hi - lo) as f32 * (x1 - x0),
            Some(_) => (x0 + x1) * 0.5,
            None => x0 + i as f32 * 12.0,
        })
        .collect();
    // Exams close in time keep a minimum gap so each dot can be pointed at.
    let gap = 12.0;
    for i in 1..xs.len() {
        xs[i] = xs[i].max(xs[i - 1] + gap);
    }
    if let Some(last) = xs.last().copied().filter(|l| *l > x1) {
        let shift = last - x1;
        xs.iter_mut().for_each(|x| *x -= shift);
        for i in (0..xs.len().saturating_sub(1)).rev() {
            xs[i] = xs[i].min(xs[i + 1] - gap);
        }
    }
    // Dates at both ends.
    let first = items.iter().find_map(|i| (!i.date.is_empty()).then(|| month_year(&i.date))).unwrap_or_default();
    let last = items.iter().rev().find_map(|i| (!i.date.is_empty()).then(|| month_year(&i.date))).unwrap_or_default();
    p.text(pos2(x0 - 4.0, line_y + 7.0), Align2::LEFT_TOP, &first, font(10.0), pal().text_faint);
    if last != first {
        p.text(pos2(x1 + 4.0, line_y + 7.0), Align2::RIGHT_TOP, &last, font(10.0), pal().text_faint);
    }

    let mut action = None;
    for (i, (item, x)) in items.iter().zip(&xs).enumerate() {
        let c = pos2(*x, line_y);
        let color = item.modalities.first().map(|m| modality_color(m)).unwrap_or(pal().text_faint);
        let resp = ui.interact(Rect::from_center_size(c, vec2(14.0, 16.0)), ui.scope_id().with(("timeline", i, item.date.as_str())), Sense::click());
        let hot = resp.hovered();
        let r = if hot { 5.5 } else { 4.5 };
        let alpha = if item.current || item.relevant.is_some() || hot { 255 } else { 120 };
        if item.relevant.is_some() {
            p.circle_stroke(c, r + 3.0, Stroke::new(1.5, with_alpha(pal().accent, 190)));
        }
        if item.local.is_some() {
            p.circle_filled(c, r, with_alpha(color, alpha));
        } else {
            // Still only on the PACS: hollow dot.
            p.circle_filled(c, r, pal().bg_raised);
            p.circle_stroke(c, r - 0.5, Stroke::new(1.5, with_alpha(color, alpha)));
        }
        if item.current {
            p.circle_stroke(c, r + 2.0, Stroke::new(1.5, pal().strong));
        }
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let compare = can_compare && !item.current;
        let resp = resp.on_hover_ui(|ui| {
            let desc = if item.description.is_empty() { tr!("Exame sem descrição", "Study without description") } else { &item.description };
            ui.label(RichText::new(format!("{} · {desc}", format_date(&item.date))).font(font_bold(13.0)).color(pal().text));
            let mut facts = Vec::new();
            let rel = relative_date(&item.date);
            if !rel.is_empty() {
                facts.push(rel);
            }
            if !item.modalities.is_empty() {
                facts.push(item.modalities.iter().map(|m| modality_name(m).to_string()).collect::<Vec<_>>().join(", "));
            }
            if item.series > 0 {
                let n = item.series;
                facts.push(if n == 1 { trf!("{n} série", "{n} series") } else { trf!("{n} séries", "{n} series") });
            }
            if item.images > 0 {
                let n = item.images;
                facts.push(if n == 1 { trf!("{n} imagem", "{n} image") } else { trf!("{n} imagens", "{n} images") });
            }
            ui.label(RichText::new(facts.join(" · ")).color(pal().text_dim));
            let place = if item.current {
                tr!("Exame aberto agora", "Study open now")
            } else if item.local.is_some() {
                tr!("Neste computador", "On this computer")
            } else {
                tr!("No PACS (ainda não baixado)", "On the PACS (not downloaded yet)")
            };
            ui.label(RichText::new(place).color(pal().text_dim));
            if let Some(why) = &item.relevant {
                ui.label(RichText::new(trf!("Relevante: {why}", "Relevant: {why}")).color(pal().accent));
            }
            let hint = match (item.current, compare, item.local.is_some()) {
                (true, _, _) => None,
                (_, true, true) => Some(tr!("Clique para comparar com o exame aberto", "Click to compare with the open study")),
                (_, true, false) => Some(tr!("Clique para baixar e comparar com o exame aberto", "Click to download and compare with the open study")),
                (_, false, true) => Some(tr!("Clique para abrir", "Click to open")),
                (_, false, false) => Some(tr!("Clique para baixar e abrir", "Click to download and open")),
            };
            if let Some(h) = hint {
                ui.label(RichText::new(h).small().color(pal().text_faint));
            }
        });
        if resp.clicked() && !item.current {
            action = match (&item.local, &item.remote) {
                (Some(uid), _) if compare => Some(TimelineAction::Compare(uid.clone())),
                (Some(uid), _) => Some(TimelineAction::Open(uid.clone())),
                (None, Some(r)) => Some(TimelineAction::Retrieve(r.clone(), compare)),
                _ => None,
            };
        }
        resp.context_menu(|ui| {
            if compare && ui.button(trf!("{}  Comparar com o exame aberto", "{}  Compare with the open study", super::icons::COLUMNS)).clicked() {
                action = match (&item.local, &item.remote) {
                    (Some(uid), _) => Some(TimelineAction::Compare(uid.clone())),
                    (None, Some(r)) => Some(TimelineAction::Retrieve(r.clone(), true)),
                    _ => None,
                };
            }
            match (&item.local, &item.remote) {
                (Some(uid), _) if !item.current => {
                    if ui.button(trf!("{}  Abrir", "{}  Open", super::icons::IMAGE)).clicked() {
                        action = Some(TimelineAction::Open(uid.clone()));
                    }
                }
                (None, Some(r)) => {
                    if ui.button(trf!("{}  Baixar e abrir", "{}  Download and open", super::icons::DOWNLOAD_SIMPLE)).clicked() {
                        action = Some(TimelineAction::Retrieve(r.clone(), false));
                    }
                    if ui.button(trf!("{}  Só baixar", "{}  Download only", super::icons::TRAY_ARROW_DOWN)).clicked() {
                        action = Some(TimelineAction::Download(r.clone()));
                    }
                }
                _ => {}
            }
        });
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_and_modality_relevance() {
        let ct = vec!["CT".to_string()];
        let mr = vec!["MR".to_string()];
        assert_eq!(relevance(("TC de Tórax", &ct), ("RM TORAX", &mr)).as_deref(), Some("mesma região (tórax)"));
        assert_eq!(relevance(("TC de crânio", &ct), ("TC abdome total", &ct)), None);
        assert_eq!(relevance(("TC de crânio", &ct), ("Exame externo", &ct)).as_deref(), Some("mesma modalidade (TC)"));
        assert_eq!(relevance(("", &ct), ("", &mr)), None);
        assert_ne!(regions("ABDOMEN PELVIS") & regions("TC de pelve"), 0);
    }

    #[test]
    fn dates_on_the_line() {
        assert_eq!(day_number("19700101"), Some(0));
        assert_eq!(day_number("20261104").zip(day_number("20251104")).map(|(a, b)| a - b), Some(365));
        assert_eq!(day_number("2026"), None);
        assert_eq!(month_year("20250310"), "mar/2025");
    }

    #[test]
    fn same_person_needs_birth_date_or_name() {
        let h = StudyHeader { patient_name: "SILVA^JOÃO".into(), birth_date: "19800215".into(), ..Default::default() };
        let mut r = StudyResult { patient_name: "SILVA^JOAO".into(), birth_date: "19800215".into(), ..Default::default() };
        assert!(same_person(&h, &r));
        r.birth_date = "19900101".into();
        assert!(!same_person(&h, &r));
        // Without birth dates the names decide, accents aside.
        let h = StudyHeader { patient_name: "SILVA^JOÃO".into(), ..Default::default() };
        r.birth_date.clear();
        assert!(same_person(&h, &r));
        r.patient_name = "SOUZA^MARIA".into();
        assert!(!same_person(&h, &r));
    }
}
