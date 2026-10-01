//! Parsing `cargo --message-format=json` output into player-friendly diagnostics.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

/// One compiler message, mapped back onto the player's code.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub level: Level,
    /// e.g. `E0382`
    pub code: Option<String>,
    pub message: String,
    /// 1-based line in the player's code (None if the problem is outside it).
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub end_line: Option<u32>,
    /// Label attached to the primary span, e.g. "value moved here".
    pub label: Option<String>,
    /// `note:` / `help:` children (compiler suggestions).
    pub notes: Vec<String>,
    /// Exactly what rustc would print in a terminal.
    pub rendered: String,
    /// True when the error points into the hidden tests (usually a signature mismatch).
    pub in_hidden_tests: bool,
}

#[derive(Deserialize, Default)]
struct CargoLine {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: Option<RustcMessage>,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    profile: Option<Profile>,
    #[serde(default)]
    success: Option<bool>,
}

#[derive(Deserialize, Default)]
struct Profile {
    #[serde(default)]
    test: bool,
}

#[derive(Deserialize, Default, Clone)]
struct RustcMessage {
    #[serde(default)]
    message: String,
    #[serde(default)]
    code: Option<RustcCode>,
    #[serde(default)]
    level: String,
    #[serde(default)]
    spans: Vec<Span>,
    #[serde(default)]
    children: Vec<RustcMessage>,
    #[serde(default)]
    rendered: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
struct RustcCode {
    #[serde(default)]
    code: String,
}

#[derive(Deserialize, Default, Clone)]
struct Span {
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    line_start: u32,
    #[serde(default)]
    line_end: u32,
    #[serde(default)]
    column_start: u32,
    #[serde(default)]
    is_primary: bool,
    #[serde(default)]
    label: Option<String>,
}

/// Everything extracted from cargo's JSON stream.
#[derive(Debug, Clone, Default)]
pub struct CompileParse {
    pub diagnostics: Vec<Diagnostic>,
    /// Path of the compiled test executable, when the build produced one.
    pub test_executable: Option<String>,
    pub build_success: Option<bool>,
    /// Lines that were not JSON (cargo's own errors, e.g. a broken manifest).
    pub other_output: Vec<String>,
}

fn is_summary_noise(msg: &RustcMessage) -> bool {
    let m = msg.message.as_str();
    msg.spans.is_empty()
        && msg.code.is_none()
        && (m.starts_with("aborting due to")
            || m.starts_with("could not compile")
            || (m.contains("warning") && m.contains("emitted")))
}

/// Parse the JSON stream. `player_lines` is the length of the player's code in `lib.rs`.
pub fn parse_cargo_json(stdout: &str, player_lines: u32) -> CompileParse {
    let mut parse = CompileParse::default();
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(entry) = serde_json::from_str::<CargoLine>(line) else {
            parse.other_output.push(line.to_string());
            continue;
        };
        match entry.reason.as_str() {
            "compiler-message" => {
                if let Some(msg) = entry.message {
                    if let Some(d) = convert(&msg, player_lines) {
                        // The lib is built in more than one profile; avoid duplicate entries.
                        if !parse
                            .diagnostics
                            .iter()
                            .any(|x| x.rendered == d.rendered && x.message == d.message)
                        {
                            parse.diagnostics.push(d);
                        }
                    }
                }
            }
            "compiler-artifact" => {
                if entry.profile.is_some_and(|p| p.test) {
                    if let Some(exe) = entry.executable {
                        parse.test_executable = Some(exe);
                    }
                }
            }
            "build-finished" => parse.build_success = entry.success,
            _ => {}
        }
    }
    parse
}

fn convert(msg: &RustcMessage, player_lines: u32) -> Option<Diagnostic> {
    let level = match msg.level.as_str() {
        "error" | "error: internal compiler error" => Level::Error,
        "warning" => Level::Warning,
        _ => return None,
    };
    if is_summary_noise(msg) {
        return None;
    }
    let lib_spans: Vec<&Span> = msg.spans.iter().filter(|s| s.file_name.ends_with("lib.rs")).collect();
    let primary = lib_spans.iter().find(|s| s.is_primary).or_else(|| lib_spans.first()).copied();
    let (line, column, end_line, label, in_hidden_tests) = match primary {
        Some(s) if s.line_start >= 1 && s.line_start <= player_lines => (
            Some(s.line_start),
            Some(s.column_start),
            Some(s.line_end.min(player_lines).max(s.line_start)),
            s.label.clone().filter(|l| !l.is_empty()),
            false,
        ),
        Some(_) => (None, None, None, None, true),
        None => (None, None, None, None, false),
    };
    let notes = msg
        .children
        .iter()
        .filter(|c| matches!(c.level.as_str(), "note" | "help"))
        .map(|c| format!("{}: {}", c.level, c.message))
        .collect();
    Some(Diagnostic {
        level,
        code: msg.code.as_ref().map(|c| c.code.clone()).filter(|c| !c.is_empty()),
        message: msg.message.clone(),
        line,
        column,
        end_line,
        label,
        notes,
        rendered: msg.rendered.clone().unwrap_or_else(|| msg.message.clone()),
        in_hidden_tests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg_line(level: &str, code: Option<&str>, message: &str, line: u32, primary: bool) -> String {
        let code =
            code.map(|c| format!(r#"{{"code":"{c}","explanation":null}}"#)).unwrap_or_else(|| "null".into());
        format!(
            r#"{{"reason":"compiler-message","package_id":"x","message":{{"message":"{message}","code":{code},"level":"{level}","spans":[{{"file_name":"src/lib.rs","line_start":{line},"line_end":{line},"column_start":5,"column_end":9,"is_primary":{primary},"label":"value moved here"}}],"children":[{{"message":"consider cloning","code":null,"level":"help","spans":[],"children":[],"rendered":null}}],"rendered":"error rendering"}}}}"#
        )
    }

    #[test]
    fn extracts_code_line_label_and_notes() {
        let json = [
            msg_line("error", Some("E0382"), "borrow of moved value: `s`", 3, true),
            r#"{"reason":"build-finished","success":false}"#.to_string(),
        ]
        .join("\n");
        let p = parse_cargo_json(&json, 10);
        assert_eq!(p.build_success, Some(false));
        let d = &p.diagnostics[0];
        assert_eq!(d.level, Level::Error);
        assert_eq!(d.code.as_deref(), Some("E0382"));
        assert_eq!((d.line, d.column), (Some(3), Some(5)));
        assert_eq!(d.label.as_deref(), Some("value moved here"));
        assert_eq!(d.notes, vec!["help: consider cloning".to_string()]);
        assert!(!d.in_hidden_tests);
    }

    #[test]
    fn errors_after_the_player_code_are_attributed_to_hidden_tests() {
        let json = msg_line("error", Some("E0425"), "cannot find function `add`", 25, true);
        let p = parse_cargo_json(&json, 10);
        assert!(p.diagnostics[0].in_hidden_tests);
        assert_eq!(p.diagnostics[0].line, None);
    }

    #[test]
    fn summary_noise_and_duplicates_are_dropped() {
        let noise = r#"{"reason":"compiler-message","message":{"message":"aborting due to 1 previous error","code":null,"level":"error","spans":[],"children":[],"rendered":"error: aborting"}}"#;
        let real = msg_line("error", Some("E0308"), "mismatched types", 2, true);
        let json = format!("{real}\n{real}\n{noise}");
        let p = parse_cargo_json(&json, 5);
        assert_eq!(p.diagnostics.len(), 1);
    }

    #[test]
    fn finds_the_test_executable_and_keeps_non_json_lines() {
        let json = concat!(
            r#"{"reason":"compiler-artifact","profile":{"test":false},"executable":null}"#,
            "\n",
            r#"{"reason":"compiler-artifact","profile":{"test":true},"executable":"/tmp/t/player_code-abc"}"#,
            "\nerror: failed to parse manifest\n"
        );
        let p = parse_cargo_json(json, 5);
        assert_eq!(p.test_executable.as_deref(), Some("/tmp/t/player_code-abc"));
        assert_eq!(p.other_output, vec!["error: failed to parse manifest".to_string()]);
    }

    #[test]
    fn warnings_are_kept_as_warnings() {
        let json = msg_line("warning", None, "unused variable: `x`", 1, true);
        let p = parse_cargo_json(&json, 5);
        assert_eq!(p.diagnostics[0].level, Level::Warning);
        assert_eq!(p.diagnostics[0].code, None);
    }
}
