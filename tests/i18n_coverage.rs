//! Every Portuguese text the user sees must have its English twin: it goes
//! through `tr!`, `trf!` or `Tr(…)` (src/i18n.rs), where the compiler checks
//! that both languages are there. This test reads the source and fails on
//! string literals with Portuguese accents found anywhere else, so a new
//! untranslated label is caught before a release. Log messages, tests, egui ids
//! and the few accented data strings listed in `DATA` are left alone.

use std::path::{Path, PathBuf};

/// Calls whose string arguments are translated.
const TRANSLATED: &[&str] = &["tr!(", "trf!(", "Tr("];

/// Calls whose string arguments are not shown in the interface.
const NOT_SHOWN: &[&str] = &[
    "log::", "info!(", "warn!(", "error!(", "debug!(", "trace!(", "assert!(", "assert_eq!(", "assert_ne!(", "debug_assert!(",
    "panic!(", ".expect(", "unreachable!(", "Id::unique(", "id_salt(", "#[",
];

/// Accented literals that are not interface text: language names written in
/// their own language, and a query name that only goes to the log. Keyword
/// lists matched against DICOM descriptions (`&["tórax", "pré", …]`) are
/// recognized by their shape: slices made only of string literals.
const DATA: &[&str] = &["Português", "Português (Brasil)", "Idioma · Language", "Language · Idioma", "de séries"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// String literals as (start byte, end byte, text), skipping comments and char literals.
fn literals(src: &str) -> Vec<(usize, usize, &str)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if src[i..].starts_with("//") {
            i = src[i..].find('\n').map_or(b.len(), |n| i + n);
        } else if src[i..].starts_with("/*") {
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth > 0 {
                if src[i..].starts_with("/*") {
                    depth += 1;
                    i += 2;
                } else if src[i..].starts_with("*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if b[i] == b'r' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) && matches!(b.get(i + 1), Some(b'"' | b'#')) {
            let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
            if b.get(i + 1 + hashes) != Some(&b'"') {
                i += 1;
                continue;
            }
            let start = i + 2 + hashes;
            let close = format!("\"{}", "#".repeat(hashes));
            let end = src[start..].find(&close).map_or(b.len(), |n| start + n);
            out.push((i, end + close.len(), &src[start..end]));
            i = end + close.len();
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            out.push((i, j + 1, &src[i + 1..j.min(b.len())]));
            i = j + 1;
        } else if b[i] == b'\'' {
            // Char literal ('a', '\n', '\u{2212}', 'ã'); a lifetime has no closing quote.
            let rest = &src[i + 1..];
            let len = if rest.starts_with("\\u{") {
                rest.find('}').map(|n| n + 1)
            } else if rest.starts_with('\\') {
                Some(2)
            } else {
                rest.chars().next().map(char::len_utf8)
            };
            match len {
                Some(n) if rest[n..].starts_with('\'') => i += n + 2,
                _ => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Byte spans from each call's opening parenthesis (or bracket) to its closing one.
fn spans(src: &str, lits: &[(usize, usize, &str)], names: &[&str]) -> Vec<(usize, usize)> {
    let in_literal = |pos: usize| lits.iter().any(|&(s, e, _)| s <= pos && pos < e);
    let b = src.as_bytes();
    let mut out = Vec::new();
    for name in names {
        for (at, _) in src.match_indices(name) {
            if in_literal(at) {
                continue;
            }
            let mut k = at + name.len() - 1;
            let mut depth = 0i32;
            while k < b.len() {
                if !in_literal(k) {
                    match b[k] {
                        b'(' | b'[' | b'{' => depth += 1,
                        b')' | b']' | b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                k += 1;
            }
            out.push((at, k));
        }
    }
    out
}

/// `&["…", "…"]` slices made only of string literals: keywords matched against data.
fn keyword_lists(src: &str, lits: &[(usize, usize, &str)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (at, _) in src.match_indices("&[\"") {
        let mut k = at + 2;
        let mut only_literals = true;
        while k < src.len() {
            if let Some(&(_, end, _)) = lits.iter().find(|&&(s, _, _)| s == k) {
                k = end;
                continue;
            }
            match src.as_bytes()[k] {
                b']' => break,
                c if c == b',' || c.is_ascii_whitespace() => {}
                _ => {
                    only_literals = false;
                    break;
                }
            }
            k += 1;
        }
        if only_literals {
            out.push((at, k));
        }
    }
    out
}

/// `#[cfg(test)] mod … { … }` blocks.
fn test_modules(src: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (at, _) in src.match_indices("#[cfg(test)]") {
        let Some(open) = src[at..].find('{').map(|n| at + n) else { continue };
        if !src[at..open].contains("mod ") {
            continue;
        }
        let mut depth = 0;
        for (k, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        out.push((at, open + k));
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn portuguese(text: &str) -> bool {
    text.chars().any(|c| "áàâãéêíóôõúçÁÀÂÃÉÊÍÓÔÕÚÇ".contains(c))
}

#[test]
fn portuguese_texts_have_an_english_twin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    assert!(files.len() > 40, "source files not found in {}", root.display());
    let mut missing = Vec::new();
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let lits = literals(&src);
        let translated = spans(&src, &lits, TRANSLATED);
        let mut skipped = spans(&src, &lits, NOT_SHOWN);
        skipped.extend(test_modules(&src));
        skipped.extend(keyword_lists(&src, &lits));
        let inside = |pos: usize, list: &[(usize, usize)]| list.iter().any(|&(a, b)| a <= pos && pos <= b);
        for &(start, _, text) in &lits {
            if portuguese(text) && !DATA.contains(&text) && !inside(start, &translated) && !inside(start, &skipped) {
                let line = src[..start].matches('\n').count() + 1;
                let name = file.strip_prefix(&root).unwrap_or(&file).display().to_string();
                missing.push(format!("src/{name}:{line}: \"{text}\""));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "{} Portuguese text(s) without English (wrap them in tr!/trf!/Tr, see src/i18n.rs):\n{}",
        missing.len(),
        missing.join("\n")
    );
}
