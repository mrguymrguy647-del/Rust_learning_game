//! Parsing the output of a libtest binary (the "pretty" format).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    /// Name without the `hidden_tests::` prefix.
    pub name: String,
    pub status: TestStatus,
    /// Panic message (assertion text with left/right values) for failed tests.
    pub message: Option<String>,
    /// Line in the player's code or tests where the panic happened, if reported.
    pub location: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestParse {
    pub cases: Vec<TestCase>,
    /// True when the `test result:` summary line was seen (the binary ran to the end).
    pub finished: bool,
    /// A test whose result line never appeared (it was running when the process died).
    pub unfinished: Option<String>,
}

impl TestParse {
    pub fn passed(&self) -> usize {
        self.count(TestStatus::Passed)
    }
    pub fn failed(&self) -> usize {
        self.count(TestStatus::Failed)
    }
    pub fn ignored(&self) -> usize {
        self.count(TestStatus::Ignored)
    }
    fn count(&self, s: TestStatus) -> usize {
        self.cases.iter().filter(|c| c.status == s).count()
    }
}

fn short_name(full: &str) -> String {
    full.strip_prefix("hidden_tests::").unwrap_or(full).to_string()
}

pub fn parse_libtest(stdout: &str, stderr: &str) -> TestParse {
    let mut parse = TestParse::default();
    let lines: Vec<&str> = stdout.lines().collect();

    for line in &lines {
        if let Some(rest) = line.strip_prefix("test ") {
            if rest.starts_with("result:") {
                parse.finished = true;
                continue;
            }
            if let Some((name, outcome)) = rest.split_once(" ... ") {
                let status = match outcome.trim() {
                    "ok" => TestStatus::Passed,
                    "FAILED" => TestStatus::Failed,
                    s if s.starts_with("ignored") => TestStatus::Ignored,
                    _ => continue,
                };
                parse.cases.push(TestCase {
                    name: short_name(name.trim()),
                    status,
                    message: None,
                    location: None,
                });
            } else if let Some(name) = rest.strip_suffix(" ...") {
                parse.unfinished = Some(short_name(name.trim()));
            }
        }
    }

    // Failure details: `---- name stdout ----` followed by the panic output.
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(name) = line.strip_prefix("---- ").and_then(|l| l.strip_suffix(" stdout ----")) {
            let name = short_name(name);
            let mut j = i + 1;
            let mut message = Vec::new();
            let mut location = None;
            let mut in_panic = false;
            while j < lines.len() && !lines[j].starts_with("---- ") && lines[j] != "failures:" {
                let l = lines[j];
                if let Some(pos) = l.find(" panicked at ") {
                    in_panic = true;
                    let loc = l[pos + " panicked at ".len()..].trim_end_matches(':').trim();
                    location = Some(loc.to_string());
                } else if in_panic {
                    if l.starts_with("note: run with `RUST_BACKTRACE") {
                        break;
                    }
                    message.push(l);
                }
                j += 1;
            }
            let text = message.join("\n").trim().to_string();
            if let Some(case) = parse.cases.iter_mut().find(|c| c.name == name) {
                case.message = if text.is_empty() { None } else { Some(text) };
                case.location = location;
            }
            i = j;
        } else {
            i += 1;
        }
    }

    // Panics can also land on stderr when output capture is off (e.g. in spawned threads).
    if parse.failed() > 0 && parse.cases.iter().all(|c| c.message.is_none()) && !stderr.is_empty() {
        if let Some(case) = parse.cases.iter_mut().find(|c| c.status == TestStatus::Failed) {
            case.message = Some(stderr.lines().take(8).collect::<Vec<_>>().join("\n"));
        }
    }
    parse
}

/// Plain-English hint for the most common runtime panics (beginners rarely recognise them).
pub fn explain_panic(message: &str) -> Option<&'static str> {
    let m = message;
    if m.contains("attempt to subtract with overflow")
        || m.contains("attempt to add with overflow")
        || m.contains("attempt to multiply with overflow")
    {
        Some("Integer overflow: the number did not fit in its type. Use a larger type, or checked_add / saturating_sub / wrapping_* when overflow is expected.")
    } else if m.contains("index out of bounds") {
        Some("You indexed past the end of a Vec/slice/array. Check the length first, or use .get(i) which returns an Option.")
    } else if m.contains("called `Option::unwrap()` on a `None` value") {
        Some("`unwrap()` on a None. Handle the None case with match / if let / unwrap_or instead.")
    } else if m.contains("called `Result::unwrap()` on an `Err` value") {
        Some("`unwrap()` on an Err. Propagate it with `?` or handle it with match.")
    } else if m.contains("already borrowed") || m.contains("already mutably borrowed") {
        Some("A RefCell was borrowed twice at runtime (the borrow rules are enforced at runtime for RefCell). Shorten the borrow's lifetime.")
    } else if m.contains("divide by zero") {
        Some("Division by zero. Check the divisor first.")
    } else if m.contains("overflowed its stack") {
        Some("Stack overflow: probably infinite recursion. Make sure every recursive call moves toward a base case.")
    } else if m.contains("PoisonError") || m.contains("poisoned") {
        Some("A thread panicked while holding the Mutex, which poisons it.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
running 3 tests
test hidden_tests::adds ... ok
test hidden_tests::subtracts ... FAILED
test hidden_tests::slow ... ignored

failures:

---- hidden_tests::subtracts stdout ----

thread 'hidden_tests::subtracts' (123) panicked at src/lib.rs:12:9:
assertion `left == right` failed: 5 - 3
  left: 1
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    hidden_tests::subtracts

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
";

    #[test]
    fn parses_statuses_messages_and_summary() {
        let p = parse_libtest(SAMPLE, "");
        assert_eq!((p.passed(), p.failed(), p.ignored()), (1, 1, 1));
        assert!(p.finished);
        let failed = p.cases.iter().find(|c| c.status == TestStatus::Failed).unwrap();
        assert_eq!(failed.name, "subtracts");
        assert_eq!(failed.location.as_deref(), Some("src/lib.rs:12:9"));
        let msg = failed.message.as_deref().unwrap();
        assert!(msg.contains("left: 1") && msg.contains("right: 2"), "{msg}");
        assert!(!msg.contains("RUST_BACKTRACE"));
    }

    #[test]
    fn detects_the_test_that_was_running_when_killed() {
        let out = "running 2 tests\ntest hidden_tests::fast ... ok\ntest hidden_tests::forever ...";
        let p = parse_libtest(out, "");
        assert_eq!(p.passed(), 1);
        assert_eq!(p.unfinished.as_deref(), Some("forever"));
        assert!(!p.finished);
    }

    #[test]
    fn panic_explanations() {
        assert!(explain_panic("attempt to subtract with overflow").unwrap().contains("overflow"));
        assert!(explain_panic("index out of bounds: the len is 3 but the index is 7").is_some());
        assert!(explain_panic("something custom").is_none());
    }
}
