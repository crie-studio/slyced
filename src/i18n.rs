//! Interface language: Brazilian Portuguese (the default) or English.
//!
//! Every text the user sees is written in both languages where it is used:
//!
//! ```ignore
//! ui.button(tr!("Abrir pasta", "Open folder"));                 // &'static str
//! toast(trf!("{n} séries abertas", "{n} series opened"));       // String (format!)
//! const NAME: Tr = Tr("Pulmão", "Lung");                         // in consts and tables
//! ```
//!
//! Both texts are checked by the compiler: a missing translation does not
//! build, and `trf!` fails when one language leaves out an argument. The
//! language is a process-wide setting (`set`), read on every frame, so a change
//! in Configurações shows at once. Numbers and dates follow it too (`num`,
//! `date`).

use std::sync::atomic::{AtomicBool, Ordering};

/// Language of the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    /// Português (Brasil).
    #[default]
    Pt,
    English,
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Changes the language of everything shown from now on.
pub fn set(lang: Lang) {
    ENGLISH.store(lang == Lang::English, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if en() { Lang::English } else { Lang::Pt }
}

/// The interface is in English.
#[inline]
pub fn en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// `tr!("português", "English")`: the text in the current language (`&'static str`).
macro_rules! tr {
    ($pt:literal, $en:literal $(,)?) => {
        if $crate::i18n::en() { $en } else { $pt }
    };
}

/// `trf!("{n} séries", "{n} series", n = 3)`: `format!` in the current language.
/// Both texts must use every argument.
macro_rules! trf {
    ($pt:literal, $en:literal $($args:tt)*) => {
        if $crate::i18n::en() { format!($en $($args)*) } else { format!($pt $($args)*) }
    };
}

/// A text in both languages, for consts, statics and tables (`Tr("Osso", "Bone").get()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tr(pub &'static str, pub &'static str);

impl Tr {
    pub fn get(self) -> &'static str {
        if en() { self.1 } else { self.0 }
    }

    pub fn pt(self) -> &'static str {
        self.0
    }

    /// The text in a given language (for functions that take the language, e.g. in tests).
    pub fn in_lang(self, lang: Lang) -> &'static str {
        match lang {
            Lang::Pt => self.0,
            Lang::English => self.1,
        }
    }
}

impl std::fmt::Display for Tr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.get())
    }
}

/// Decimal separator of the current language.
pub fn decimal_separator() -> char {
    if en() { '.' } else { ',' }
}

/// A number with `decimals` places and the language's separator: "23,4" / "23.4".
pub fn num(value: f64, decimals: usize) -> String {
    dec(format!("{value:.decimals$}"))
}

/// Like `num`, without trailing zeros: 2.50 → "2,5" / "2.5", 3.00 → "3".
pub fn num_trim(value: f64, max_decimals: usize) -> String {
    let s = format!("{value:.max_decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    dec(s)
}

/// Puts the language's decimal separator in a number that `format!` wrote with
/// a point. Only for texts made of numbers and units ("12.5 mm"), never sentences.
pub fn dec(text: impl Into<String>) -> String {
    let text = text.into();
    if en() { text } else { text.replace('.', ",") }
}

/// Reads a decimal typed by the user, with a comma or a point.
pub fn parse_num(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
}

const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A calendar date: "12/03/2024" in Portuguese, "12 Mar 2024" in English
/// (day before the month in words, so it reads the same in the US and the UK).
pub fn date(year: u32, month: u32, day: u32) -> String {
    date_in(current(), year, month, day)
}

pub fn date_in(lang: Lang, year: u32, month: u32, day: u32) -> String {
    match lang {
        Lang::Pt => format!("{day:02}/{month:02}/{year:04}"),
        Lang::English => match MONTHS_EN.get(month.wrapping_sub(1) as usize) {
            Some(m) => format!("{day} {m} {year:04}"),
            None => format!("{year:04}-{month:02}-{day:02}"),
        },
    }
}

/// Code saved in `config.json` and used by the web page.
pub fn code(lang: Lang) -> &'static str {
    match lang {
        Lang::Pt => "pt-BR",
        Lang::English => "en",
    }
}

/// A language tag ("pt-BR", "pt_PT", "en-US", "es"…): Portuguese only for `pt*`.
pub fn from_tag(tag: &str) -> Lang {
    if tag.trim().to_ascii_lowercase().starts_with("pt") { Lang::Pt } else { Lang::English }
}

/// Language of the system: the Windows display language, the browser's
/// language in the web version, `LANG` elsewhere. Portuguese when unknown.
pub fn system() -> Lang {
    static SYSTEM: std::sync::OnceLock<Lang> = std::sync::OnceLock::new();
    *SYSTEM.get_or_init(detect)
}

#[cfg(windows)]
fn detect() -> Lang {
    // LANGID: the low 10 bits are the primary language; 0x16 is Portuguese.
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if id == 0 {
        return Lang::Pt;
    }
    if id & 0x3ff == 0x16 { Lang::Pt } else { Lang::English }
}

#[cfg(target_arch = "wasm32")]
fn detect() -> Lang {
    let tag = crate::web::browser_language();
    if tag.is_empty() { Lang::Pt } else { from_tag(&tag) }
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
fn detect() -> Lang {
    match std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")) {
        Ok(tag) if !tag.is_empty() && tag != "C" && tag != "POSIX" => from_tag(&tag),
        _ => Lang::Pt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(from_tag("pt-BR"), Lang::Pt);
        assert_eq!(from_tag("pt_PT.UTF-8"), Lang::Pt);
        assert_eq!(from_tag("PT"), Lang::Pt);
        assert_eq!(from_tag("en-US"), Lang::English);
        assert_eq!(from_tag("es-AR"), Lang::English);
    }

    #[test]
    fn dates() {
        assert_eq!(date_in(Lang::Pt, 2024, 3, 12), "12/03/2024");
        assert_eq!(date_in(Lang::English, 2024, 3, 12), "12 Mar 2024");
        assert_eq!(date_in(Lang::English, 2024, 13, 1), "2024-13-01");
    }

    #[test]
    fn macros_capture_and_check_arguments() {
        // Portuguese is the default and no test changes it (tests run in parallel).
        let n = 3;
        assert_eq!(trf!("{n} séries", "{n} series"), "3 séries");
        assert_eq!(trf!("{} de {total}", "{} of {total}", 2, total = 5), "2 de 5");
        assert_eq!(tr!("Osso", "Bone"), "Osso");
        assert_eq!(Tr("Pulmão", "Lung").get(), "Pulmão");
        assert_eq!(num(23.44, 1), "23,4");
        assert_eq!(num_trim(2.5, 2), "2,5");
        assert_eq!(num_trim(3.0, 2), "3");
        assert_eq!(parse_num(" 1,5 "), Some(1.5));
    }
}
