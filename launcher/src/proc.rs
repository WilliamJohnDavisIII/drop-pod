// SPDX-License-Identifier: GPL-3.0-or-later AND MIT
// Running the scripts: a command whose output lines come back one by one, then its exit status, without blocking the
// window (a thread reads them and wakes the window). Their error output goes to logs/launcher.log. Ported from
// LauncherApp.swift's LineProcess (portions MIT, see LICENSE.reclaimer-mac-port).
use crate::paths;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// What a running command said.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg { Line(String), Exit(i32) }

/// Wakes the window when a command has something to say (egui's repaint request; nothing in snapshots).
pub type Waker = std::sync::Arc<dyn Fn() + Send + Sync>;

pub struct Proc {
    pid: u32,
    rx: Receiver<Msg>,
    done: bool,
}

impl Proc {
    pub fn spawn(waker: &Waker, exe: &str, args: &[String], extra: &[(&str, String)]) -> Option<Proc> {
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .env("DROPPOD_HOME", paths::home())
            .env("DROPPOD_APP_VERSION", paths::app_version())
            .env("DROPPOD_REPO", paths::app_repo())
            // (install.sh copies it into Applications if it isn't there)
            .env("DROPPOD_APP_SOURCE", paths::bundle().map(|b| b.to_path_buf()).unwrap_or_default());
        // updates replace the app the player opened, wherever it is; never the read-only disk image's copy
        if paths::is_installed_app() { cmd.env("DROPPOD_APP_PATH", paths::bundle().unwrap()); }
        for (k, v) in extra { cmd.env(k, v); }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped())
            .stderr(log_file().map(Stdio::from).unwrap_or_else(Stdio::null));
        let mut child = cmd.spawn().ok()?;
        let pid = child.id();
        let out = child.stdout.take()?;
        let (tx, rx) = channel();
        let waker = waker.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(out);
            let mut buf = Vec::new();
            while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                if buf.last() == Some(&b'\n') { buf.pop(); }
                let _ = tx.send(Msg::Line(String::from_utf8_lossy(&buf).into_owned()));
                buf.clear();
                waker();
            }
            // (after every line, so the exit always comes last)
            let code = child.wait().map(|s| {
                #[cfg(unix)]
                { use std::os::unix::process::ExitStatusExt; s.code().or(s.signal()).unwrap_or(-1) }
                #[cfg(not(unix))]
                { s.code().unwrap_or(-1) }
            }).unwrap_or(-1);
            let _ = tx.send(Msg::Exit(code));
            waker();
        });
        Some(Proc { pid, rx, done: false })
    }

    /// What it said since the last call.
    pub fn poll(&mut self) -> Vec<Msg> {
        let msgs: Vec<Msg> = self.rx.try_iter().collect();
        if msgs.iter().any(|m| matches!(m, Msg::Exit(_))) { self.done = true; }
        msgs
    }

    pub fn is_running(&self) -> bool { !self.done }

    /// Asks it to stop (SIGTERM, as Foundation's Process.terminate does).
    pub fn stop(&self) {
        #[cfg(unix)]
        if !self.done { unsafe { libc::kill(self.pid as i32, libc::SIGTERM); } }
        #[cfg(windows)]
        if !self.done { let _ = Command::new("taskkill").args(["/PID", &self.pid.to_string()]).status(); }
    }

    pub fn pid(&self) -> u32 { self.pid }
}

/// The Python 3 the scripts run with (Windows: the py launcher; Linux: the one on PATH).
pub const PYTHON: &str = if cfg!(target_os = "macos") { "/usr/bin/python3" } else if cfg!(windows) { "py" } else { "python3" };

/// `python3 -I scripts/<script> <args>`: a helper script install.sh (setup.py) put in $DROPPOD_HOME/scripts.
pub fn python(waker: &Waker, script: &str, args: &[&str]) -> Option<Proc> {
    let mut a = vec!["-I".to_string(), paths::scripts_dir().join(script).to_string_lossy().into_owned()];
    a.extend(args.iter().map(|s| s.to_string()));
    Proc::spawn(waker, PYTHON, &a, &[])
}

/// install.sh (setup.py off the Mac) from inside the app, in GUI mode, with the given choices.
pub fn installer(waker: &Waker, choices: &[(&str, String)]) -> Option<Proc> {
    let url = paths::installer().filter(|p| p.exists())?;
    let mut env: Vec<(&str, String)> = choices.to_vec();
    env.push(("DROPPOD_GUI", "1".into()));
    if cfg!(target_os = "macos") {
        Proc::spawn(waker, "/bin/bash", &[url.to_string_lossy().into_owned()], &env)
    } else {
        Proc::spawn(waker, PYTHON, &["-I".into(), url.to_string_lossy().into_owned()], &env)
    }
}

/// Does the command succeed? (Waits for it: only for quick checks.)
pub fn succeeds(exe: &str, args: &[&str]) -> bool {
    Command::new(exe).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

/// Is any process matching this pattern running? On a thread of its own; the answer comes back through `done`.
pub fn process_running(pattern: String, done: impl FnOnce(bool) + Send + 'static) {
    std::thread::spawn(move || {
        // -a: pgrep otherwise skips the caller's ancestors
        done(succeeds("/usr/bin/pgrep", &["-a", "-f", &pattern]));
    });
}

/// WebView2's background updater can keep Wine alive after Reach exits. Ask for the mod's actual processes,
/// checked against this install's working folders, so another Reach install does not keep its tile playing.
pub fn mod_running(engine: String, done: impl FnOnce(bool) + Send + 'static) {
    std::thread::spawn(move || {
        let out = Command::new(PYTHON).arg("-I").arg(paths::scripts_dir().join("tools.py"))
            .args(["game-running", &engine]).env("DROPPOD_HOME", paths::home())
            .stdin(Stdio::null()).stderr(Stdio::null()).output();
        done(out.is_ok_and(|o| o.status.success() && o.stdout.trim_ascii() == b"running"));
    });
}

/// Scripts' error output goes to logs/launcher.log for troubleshooting (appended: the window and the scripts write to
/// it at the same time).
pub fn log_file() -> Option<std::fs::File> {
    let path = paths::home().join("logs/launcher.log");
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

pub fn log(who: &str, text: &str) {
    if let Some(mut f) = log_file() { let _ = writeln!(f, "{} {who} {text}", iso8601_now()); }
}

/// The time now in UTC, as ISO 8601 (2026-10-17T22:31:05Z).
pub fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, s) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

/// Opens a file or folder the way Finder would (`open`); `reveal`: selects it in a Finder window instead.
pub fn open_in_finder(path: &Path, reveal: bool) {
    #[cfg(target_os = "macos")]
    let mut c = Command::new("/usr/bin/open");
    #[cfg(target_os = "macos")]
    if reveal { c.arg("-R"); }
    #[cfg(target_os = "windows")]
    let mut c = Command::new("explorer");
    #[cfg(target_os = "windows")]
    if reveal { c.arg("/select,"); }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = { let _ = reveal; Command::new("xdg-open") };
    let _ = c.arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
