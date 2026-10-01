//! Static checks on the player's source ("must not use `.clone()`", "no `for` loops", …).
//!
//! Comments and string/char literals are blanked out first, so a forbidden word inside a comment
//! or a string does not count against the player.

use crate::data::Check;

/// Replace comments and the *contents* of string/char literals with spaces (newlines are kept,
/// so positions are preserved).
pub fn sanitize(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // line comment
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // block comment (nestable)
        if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            continue;
        }
        // raw string r"..." / r#"..."#
        if c == 'r' && matches!(next, Some('"') | Some('#')) && !prev_is_ident(&chars, i) {
            let mut j = i + 1;
            let mut hashes = 0;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                out.push(' ');
                for _ in 0..hashes {
                    out.push(' ');
                }
                out.push('"');
                j += 1;
                loop {
                    match chars.get(j) {
                        None => break,
                        Some('"') if (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) => {
                            out.push('"');
                            for _ in 0..hashes {
                                out.push(' ');
                            }
                            j += 1 + hashes;
                            break;
                        }
                        Some(&ch) => {
                            out.push(blank(ch));
                            j += 1;
                        }
                    }
                }
                i = j;
                continue;
            }
        }
        // normal string
        if c == '"' {
            out.push('"');
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' => {
                        out.push(' ');
                        if let Some(&e) = chars.get(i + 1) {
                            out.push(blank(e));
                        }
                        i += 2;
                    }
                    '"' => {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    ch => {
                        out.push(blank(ch));
                        i += 1;
                    }
                }
            }
            continue;
        }
        // char literal vs lifetime
        if c == '\'' {
            let is_char =
                matches!((next, chars.get(i + 2).copied()), (Some('\\'), _) | (Some(_), Some('\'')));
            if is_char {
                out.push('\'');
                i += 1;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => {
                            out.push_str("  ");
                            i += 2;
                        }
                        '\'' => {
                            out.push('\'');
                            i += 1;
                            break;
                        }
                        ch => {
                            out.push(blank(ch));
                            i += 1;
                        }
                    }
                }
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn prev_is_ident(chars: &[char], i: usize) -> bool {
    i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whole-word match on identifiers/keywords.
pub fn contains_word(sanitized: &str, word: &str) -> bool {
    let bytes: Vec<char> = sanitized.chars().collect();
    let w: Vec<char> = word.chars().collect();
    if w.is_empty() || bytes.len() < w.len() {
        return false;
    }
    (0..=bytes.len() - w.len()).any(|start| {
        bytes[start..start + w.len()] == w[..]
            && (start == 0 || !is_ident(bytes[start - 1]))
            && bytes.get(start + w.len()).is_none_or(|&c| !is_ident(c))
    })
}

/// Whitespace-insensitive substring match.
pub fn contains_text(sanitized: &str, text: &str) -> bool {
    let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let needle = strip(text);
    !needle.is_empty() && strip(sanitized).contains(&needle)
}

/// Run all checks; returns the messages of those that FAILED.
pub fn run_checks(code: &str, checks: &[Check]) -> Vec<String> {
    let clean = sanitize(code);
    checks
        .iter()
        .filter(|check| {
            let ok = match check {
                Check::ForbidWord { word, .. } => !contains_word(&clean, word),
                Check::RequireWord { word, .. } => contains_word(&clean, word),
                Check::ForbidText { text, .. } => !contains_text(&clean, text),
                Check::RequireText { text, .. } => contains_text(&clean, text),
            };
            !ok
        })
        .map(|c| c.message().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forbid_word(w: &str) -> Check {
        Check::ForbidWord { word: w.into(), message: format!("no {w}") }
    }

    #[test]
    fn comments_and_strings_do_not_count() {
        let code = "// unwrap here\nfn f() { let s = \"unwrap\"; /* unwrap */ let c = 'u'; }";
        assert!(run_checks(code, &[forbid_word("unwrap")]).is_empty());
    }

    #[test]
    fn real_uses_are_found_and_words_are_whole() {
        assert_eq!(
            run_checks("fn f(x: Option<i32>) -> i32 { x.unwrap() }", &[forbid_word("unwrap")]).len(),
            1
        );
        assert!(run_checks("fn f() { let unwrap_or = 1; }", &[forbid_word("unwrap")]).is_empty());
        assert!(run_checks("fn f() { let s = format!(\"x\"); }", &[forbid_word("for")]).is_empty());
        assert_eq!(run_checks("fn f() { for i in 0..3 {} }", &[forbid_word("for")]).len(), 1);
    }

    #[test]
    fn text_checks_ignore_whitespace() {
        let forbid = Check::ForbidText { text: ".clone()".into(), message: "no clone".into() };
        assert_eq!(run_checks("let b = a . clone ( );", std::slice::from_ref(&forbid)).len(), 1);
        assert!(run_checks("let b = a.to_owned();", &[forbid]).is_empty());
        let require = Check::RequireText { text: ".iter()".into(), message: "use iter".into() };
        assert!(run_checks("v.iter().sum::<i32>()", std::slice::from_ref(&require)).is_empty());
        assert_eq!(run_checks("let mut t = 0; t += 1;", &[require]).len(), 1);
    }

    #[test]
    fn raw_strings_lifetimes_and_escapes_are_handled() {
        let code = "fn f<'a>(x: &'a str) -> &'a str { let r = r#\"unwrap \" still string\"#; let q = \"a\\\"unwrap\"; x }";
        assert!(run_checks(code, &[forbid_word("unwrap")]).is_empty());
        let s = sanitize("let c = '\\''; let d = 'x'; unwrap");
        assert!(s.contains("unwrap"));
    }

    #[test]
    fn sanitize_preserves_length_in_lines() {
        let src = "a // c\n/* x\ny */ b\n\"s\ntr\" c";
        assert_eq!(sanitize(src).lines().count(), src.lines().count());
    }
}
