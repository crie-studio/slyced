//! Patient / study / series / instance index built from scanned headers.

use glam::DVec3;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct InstanceInfo {
    pub path: PathBuf,
    pub sop_instance_uid: String,
    pub instance_number: i32,
    pub frames: u32,
    pub rows: u32,
    pub cols: u32,
    pub position: Option<DVec3>,
    pub orientation: Option<(DVec3, DVec3)>,
    /// `(dx, dy)`: column spacing and row spacing in mm.
    pub pixel_spacing: Option<(f64, f64)>,
    pub slice_thickness: Option<f64>,
    pub has_pixels: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SeriesInfo {
    pub uid: String,
    pub number: i32,
    pub description: String,
    pub modality: String,
    pub body_part: String,
    /// Frame of Reference UID: series sharing it share patient coordinates.
    pub frame_of_reference: String,
    /// Contrast/bolus agent, empty for exams without contrast.
    pub contrast: String,
    pub date: String,
    pub time: String,
    pub instances: Vec<InstanceInfo>,
}

impl SeriesInfo {
    pub fn image_count(&self) -> usize {
        self.instances.iter().filter(|i| i.has_pixels).map(|i| i.frames.max(1) as usize).sum()
    }

    pub fn title(&self) -> String {
        if self.description.trim().is_empty() {
            format!("Série {}", self.number)
        } else {
            self.description.trim().to_string()
        }
    }
}

/// Patient and study level attributes shown in overlays and the browser.
#[derive(Clone, Debug, Default)]
pub struct StudyHeader {
    pub uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub birth_date: String,
    pub sex: String,
    pub age: String,
    pub date: String,
    pub time: String,
    pub description: String,
    pub accession: String,
    pub institution: String,
    pub referring: String,
}

#[derive(Clone, Debug, Default)]
pub struct StudyInfo {
    pub header: StudyHeader,
    pub series: Vec<SeriesInfo>,
}

impl StudyInfo {
    pub fn modalities(&self) -> Vec<String> {
        let mut m: Vec<String> = Vec::new();
        for s in &self.series {
            if !s.modality.is_empty() && !m.contains(&s.modality) {
                m.push(s.modality.clone());
            }
        }
        m
    }

    pub fn image_count(&self) -> usize {
        self.series.iter().map(|s| s.image_count()).sum()
    }
}

/// One parsed file, before it is merged into the [`Library`].
#[derive(Clone, Debug)]
pub struct ScannedFile {
    pub study: StudyHeader,
    pub series_uid: String,
    pub series_number: i32,
    pub series_description: String,
    pub modality: String,
    pub body_part: String,
    pub frame_of_reference: String,
    pub contrast: String,
    pub series_date: String,
    pub series_time: String,
    pub instance: InstanceInfo,
}

#[derive(Default)]
pub struct Library {
    pub studies: Vec<StudyInfo>,
    /// Bumped on every change so views can refresh cached derived data.
    pub revision: u64,
}

impl Library {
    pub fn merge(&mut self, files: Vec<ScannedFile>) -> usize {
        let mut study_idx: HashMap<String, usize> =
            self.studies.iter().enumerate().map(|(i, s)| (s.header.uid.clone(), i)).collect();
        let mut added = 0;
        for f in files {
            let si = *study_idx.entry(f.study.uid.clone()).or_insert_with(|| {
                self.studies.push(StudyInfo { header: f.study.clone(), series: Vec::new() });
                self.studies.len() - 1
            });
            let study = &mut self.studies[si];
            let series = match study.series.iter_mut().position(|s| s.uid == f.series_uid) {
                Some(i) => &mut study.series[i],
                None => {
                    study.series.push(SeriesInfo {
                        uid: f.series_uid.clone(),
                        number: f.series_number,
                        description: f.series_description.clone(),
                        modality: f.modality.clone(),
                        body_part: f.body_part.clone(),
                        frame_of_reference: f.frame_of_reference.clone(),
                        contrast: f.contrast.clone(),
                        date: f.series_date.clone(),
                        time: f.series_time.clone(),
                        instances: Vec::new(),
                    });
                    study.series.last_mut().unwrap()
                }
            };
            let dup = !f.instance.sop_instance_uid.is_empty()
                && series.instances.iter().any(|i| i.sop_instance_uid == f.instance.sop_instance_uid);
            if !dup {
                series.instances.push(f.instance);
                added += 1;
            }
        }
        for s in &mut self.studies {
            s.series.retain(|se| se.instances.iter().any(|i| i.has_pixels));
            s.series.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.time.cmp(&b.time)));
            for se in &mut s.series {
                se.instances.sort_by_key(|a| a.instance_number);
            }
        }
        self.studies.retain(|s| !s.series.is_empty());
        self.studies.sort_by(|a, b| {
            b.header.date.cmp(&a.header.date).then_with(|| a.header.patient_name.cmp(&b.header.patient_name))
        });
        self.revision += 1;
        added
    }

    pub fn find_series(&self, uid: &str) -> Option<(&StudyInfo, &SeriesInfo)> {
        self.studies.iter().find_map(|st| st.series.iter().find(|s| s.uid == uid).map(|s| (st, s)))
    }

    pub fn remove_study(&mut self, uid: &str) {
        self.take_study(uid);
    }

    /// Removes a study from the list and returns it (to put it back later).
    pub fn take_study(&mut self, uid: &str) -> Option<StudyInfo> {
        let i = self.studies.iter().position(|s| s.header.uid == uid)?;
        self.revision += 1;
        Some(self.studies.remove(i))
    }

    /// Puts back a study taken with [`Library::take_study`], in date order.
    pub fn restore_study(&mut self, study: StudyInfo) {
        if self.studies.iter().any(|s| s.header.uid == study.header.uid) {
            return;
        }
        self.studies.push(study);
        self.studies.sort_by(|a, b| {
            b.header.date.cmp(&a.header.date).then_with(|| a.header.patient_name.cmp(&b.header.patient_name))
        });
        self.revision += 1;
    }
}

/// `SOBRENOME^NOME^MEIO` -> `SOBRENOME, NOME MEIO`.
pub fn format_person_name(raw: &str) -> String {
    let first = raw.split('=').next().unwrap_or("");
    let parts: Vec<&str> = first.split('^').map(str::trim).filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        _ => format!("{}, {}", parts[0], parts[1..].join(" ")),
    }
}

/// `AAAAMMDD` -> `DD/MM/AAAA`.
pub fn format_date(raw: &str) -> String {
    let d = raw.trim();
    if d.len() >= 8 && d[..8].bytes().all(|b| b.is_ascii_digit()) {
        format!("{}/{}/{}", &d[6..8], &d[4..6], &d[0..4])
    } else {
        d.to_string()
    }
}

/// `HHMMSS.frac` -> `HH:MM:SS`.
pub fn format_time(raw: &str) -> String {
    let t = raw.trim().replace(':', "");
    if t.len() >= 4 && t[..4].bytes().all(|b| b.is_ascii_digit()) {
        let mut s = format!("{}:{}", &t[0..2], &t[2..4]);
        if t.len() >= 6 {
            s.push(':');
            s.push_str(&t[4..6]);
        }
        s
    } else {
        t
    }
}

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Days between a DICOM date (`AAAAMMDD`) and `today` (days since 1970).
fn days_between(raw: &str, today: i64) -> Option<i64> {
    let d: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() < 8 {
        return None;
    }
    let (y, m, day) = (d[0..4].parse().ok()?, d[4..6].parse().ok()?, d[6..8].parse().ok()?);
    Some(today - days_from_civil(y, m, day))
}

/// "hoje", "ontem", "há 3 dias", "há 2 meses", "há 1 ano"…
pub fn relative_date(raw: &str) -> String {
    let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86_400).unwrap_or(0);
    relative_to(raw, today)
}

fn relative_to(raw: &str, today: i64) -> String {
    match days_between(raw, today) {
        None => String::new(),
        Some(n) if n < 0 => String::new(),
        Some(0) => "hoje".into(),
        Some(1) => "ontem".into(),
        Some(n @ 2..=44) => format!("há {n} dias"),
        Some(n @ 45..=364) => match (n as f64 / 30.44).round() as i64 {
            1 => "há 1 mês".into(),
            m => format!("há {m} meses"),
        },
        Some(n) => match n / 365 {
            1 => "há 1 ano".into(),
            y => format!("há {y} anos"),
        },
    }
}

/// DICOM age string (`046Y`, `018M`…) -> "46 anos", "18 meses".
pub fn format_age(raw: &str) -> String {
    let t = raw.trim();
    let n: u32 = match t.trim_end_matches(char::is_alphabetic).parse() {
        Ok(n) => n,
        Err(_) => return t.to_string(),
    };
    let (one, many) = match t.chars().last() {
        Some('M') => ("mês", "meses"),
        Some('W') => ("semana", "semanas"),
        Some('D') => ("dia", "dias"),
        _ => ("ano", "anos"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_dates_and_ages() {
        let today = days_from_civil(2026, 11, 1);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(relative_to("20261101", today), "hoje");
        assert_eq!(relative_to("20261031", today), "ontem");
        assert_eq!(relative_to("20261020", today), "há 12 dias");
        assert_eq!(relative_to("20250310", today), "há 1 ano");
        assert_eq!(relative_to("20260601", today), "há 5 meses");
        assert_eq!(relative_to("", today), "");
        assert_eq!(format_age("046Y"), "46 anos");
        assert_eq!(format_age("001M"), "1 mês");
    }

    #[test]
    fn formatting() {
        assert_eq!(format_person_name("SILVA^JOAO^PEDRO"), "SILVA, JOAO PEDRO");
        assert_eq!(format_person_name("MARIA DA SILVA"), "MARIA DA SILVA");
        assert_eq!(format_date("20240115"), "15/01/2024");
        assert_eq!(format_time("143005.123"), "14:30:05");
    }
}
