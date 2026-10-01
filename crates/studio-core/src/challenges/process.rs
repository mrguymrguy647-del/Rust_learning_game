//! Running child processes with a hard timeout, an output cap and (on Unix) resource limits.
//!
//! This is the safety net around player code: a runaway loop is killed, a program that prints
//! gigabytes cannot exhaust memory, and the whole process *group* is killed so no orphans stay
//! behind.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Limits {
    pub timeout: Duration,
    /// Max bytes kept per stream (the rest is drained and discarded).
    pub output_limit: usize,
    /// Unix only: address-space limit for the child.
    pub memory_bytes: Option<u64>,
    /// Unix only: CPU-seconds limit for the child.
    pub cpu_secs: Option<u64>,
    /// Unix only: largest file the child may create.
    pub file_bytes: Option<u64>,
}

impl Limits {
    pub fn new(timeout: Duration, output_limit: usize) -> Limits {
        Limits { timeout, output_limit, memory_bytes: None, cpu_secs: None, file_bytes: None }
    }

    /// Limits for running untrusted test binaries.
    pub fn for_player_code(timeout: Duration, output_limit: usize) -> Limits {
        Limits {
            timeout,
            output_limit,
            memory_bytes: Some(3 * 1024 * 1024 * 1024),
            cpu_secs: Some(timeout.as_secs() + 5),
            file_bytes: Some(16 * 1024 * 1024),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcStatus {
    Exited(i32),
    /// Terminated by a signal (Unix).
    Signaled(i32),
    TimedOut,
    Cancelled,
    SpawnFailed(String),
}

#[derive(Debug, Clone)]
pub struct ProcOutput {
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub status: ProcStatus,
    pub elapsed: Duration,
}

impl ProcOutput {
    fn failed(msg: String) -> ProcOutput {
        ProcOutput {
            stdout: String::new(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            status: ProcStatus::SpawnFailed(msg),
            elapsed: Duration::ZERO,
        }
    }

    pub fn success(&self) -> bool {
        self.status == ProcStatus::Exited(0)
    }
}

struct Captured {
    bytes: Vec<u8>,
    truncated: bool,
}

fn spawn_reader<R: Read + Send + 'static>(mut source: R, limit: usize) -> mpsc::Receiver<Captured> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut truncated = false;
        let mut chunk = [0u8; 8192];
        loop {
            match source.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    // Always keep reading so the child never blocks on a full pipe.
                    let room = limit.saturating_sub(bytes.len());
                    if room >= n {
                        bytes.extend_from_slice(&chunk[..n]);
                    } else {
                        bytes.extend_from_slice(&chunk[..room]);
                        truncated = true;
                    }
                }
            }
        }
        let _ = tx.send(Captured { bytes, truncated });
    });
    rx
}

#[cfg(unix)]
fn kill_tree(child: &mut Child) {
    // The child leads its own process group (see `configure`), so `-pid` reaches every descendant.
    // SAFETY: plain syscall with integer arguments.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
}

#[cfg(windows)]
fn kill_tree(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
fn kill_tree(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(unix)]
fn configure(cmd: &mut Command, limits: &Limits) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    let (mem, cpu, fsize) = (limits.memory_bytes, limits.cpu_secs, limits.file_bytes);
    // SAFETY: the closure only calls `setrlimit`, which is async-signal-safe, and allocates nothing.
    unsafe {
        cmd.pre_exec(move || {
            // A failure to lower a limit is not fatal: the wall-clock timeout still applies.
            macro_rules! limit {
                ($resource:expr, $value:expr) => {{
                    let lim =
                        libc::rlimit { rlim_cur: $value as libc::rlim_t, rlim_max: $value as libc::rlim_t };
                    libc::setrlimit($resource, &lim);
                }};
            }
            if let Some(m) = mem {
                limit!(libc::RLIMIT_AS, m);
            }
            if let Some(c) = cpu {
                limit!(libc::RLIMIT_CPU, c);
            }
            if let Some(f) = fsize {
                limit!(libc::RLIMIT_FSIZE, f);
            }
            limit!(libc::RLIMIT_CORE, 0u64);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure(_cmd: &mut Command, _limits: &Limits) {}

/// Run `cmd` to completion, or kill it when `limits.timeout` passes or `cancel` is set.
pub fn run_limited(mut cmd: Command, limits: &Limits, cancel: &AtomicBool) -> ProcOutput {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    configure(&mut cmd, limits);

    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ProcOutput::failed(e.to_string()),
    };
    let out_rx = child.stdout.take().map(|s| spawn_reader(s, limits.output_limit));
    let err_rx = child.stderr.take().map(|s| spawn_reader(s, limits.output_limit));

    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break exit_status(st),
            Ok(None) => {}
            Err(e) => break ProcStatus::SpawnFailed(e.to_string()),
        }
        if cancel.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            let _ = child.wait();
            break ProcStatus::Cancelled;
        }
        if started.elapsed() >= limits.timeout {
            kill_tree(&mut child);
            let _ = child.wait();
            break ProcStatus::TimedOut;
        }
        thread::sleep(Duration::from_millis(5));
    };

    // Descendants that escaped the group could keep the pipes open; never wait on them forever.
    let grab = |rx: Option<mpsc::Receiver<Captured>>| match rx {
        Some(rx) => rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(Captured { bytes: Vec::new(), truncated: false }),
        None => Captured { bytes: Vec::new(), truncated: false },
    };
    let out = grab(out_rx);
    let err = grab(err_rx);
    ProcOutput {
        stdout: String::from_utf8_lossy(&out.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&err.bytes).into_owned(),
        stdout_truncated: out.truncated,
        stderr_truncated: err.truncated,
        status,
        elapsed: started.elapsed(),
    }
}

#[cfg(unix)]
fn exit_status(st: std::process::ExitStatus) -> ProcStatus {
    use std::os::unix::process::ExitStatusExt;
    match (st.code(), st.signal()) {
        (Some(c), _) => ProcStatus::Exited(c),
        (None, Some(s)) => ProcStatus::Signaled(s),
        (None, None) => ProcStatus::Exited(-1),
    }
}

#[cfg(not(unix))]
fn exit_status(st: std::process::ExitStatus) -> ProcStatus {
    ProcStatus::Exited(st.code().unwrap_or(-1))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    fn limits(secs: u64, cap: usize) -> Limits {
        Limits::new(Duration::from_secs(secs), cap)
    }

    #[test]
    fn captures_output_and_exit_code() {
        let out =
            run_limited(sh("echo hi; echo oops >&2; exit 3"), &limits(5, 1000), &AtomicBool::new(false));
        assert_eq!(out.stdout.trim(), "hi");
        assert_eq!(out.stderr.trim(), "oops");
        assert_eq!(out.status, ProcStatus::Exited(3));
    }

    #[test]
    fn runaway_process_is_killed_at_the_timeout() {
        let started = Instant::now();
        let l = Limits::new(Duration::from_millis(400), 1000);
        let out = run_limited(sh("while true; do :; done"), &l, &AtomicBool::new(false));
        assert_eq!(out.status, ProcStatus::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn grandchildren_die_with_the_group() {
        // The inner sleep would outlive the shell if only the shell were killed.
        let l = Limits::new(Duration::from_millis(300), 1000);
        let out = run_limited(sh("sleep 30 & sleep 30; echo done"), &l, &AtomicBool::new(false));
        assert_eq!(out.status, ProcStatus::TimedOut);
        assert!(out.elapsed < Duration::from_secs(5), "reader threads must not hang on orphans");
    }

    #[test]
    fn output_is_capped_but_fully_drained() {
        let out = run_limited(sh("yes | head -c 5000000"), &limits(10, 1000), &AtomicBool::new(false));
        assert_eq!(out.stdout.len(), 1000);
        assert!(out.stdout_truncated);
        assert!(out.success());
    }

    #[test]
    fn cancel_flag_stops_the_process() {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            c2.store(true, Ordering::Relaxed);
        });
        let out = run_limited(sh("sleep 30"), &limits(20, 100), &cancel);
        assert_eq!(out.status, ProcStatus::Cancelled);
    }

    #[test]
    fn missing_program_is_reported_not_panicked() {
        let out = run_limited(
            Command::new("definitely-not-a-program-xyz"),
            &limits(2, 100),
            &AtomicBool::new(false),
        );
        assert!(matches!(out.status, ProcStatus::SpawnFailed(_)));
    }
}
