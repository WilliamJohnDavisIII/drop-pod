// SPDX-License-Identifier: GPL-3.0-or-later AND MIT
// The setup wizard's state: it checks the Mac, lets the player choose games, where the game files are or go and where
// they come from (Steam, or an MCC folder they already have), then runs install.sh from inside the app in its GUI
// mode and keeps its steps, the Steam QR code and the download's progress. Ported from Setup.swift, adapted from the
// Project Reclaimer Mac port (portions MIT, see LICENSE.reclaimer-mac-port).
use crate::core::*;
use crate::paths;
use crate::proc::{self, Msg, Proc, Waker};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Check { pub id: String, pub title: String, pub ok: Option<bool>, pub detail: String } // ok None: for information

#[derive(Debug, Clone, PartialEq)]
pub enum Page { Checks, Games, Files, Installing, Failed(String), Done }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source { Steam, Existing }

pub struct Setup {
    pub page: Page,
    // 1. the Mac
    pub checks: Vec<Check>,
    pub checks_passed: bool,
    pub needs_dev_tools: bool,
    pub waiting_for_dev_tools: Option<f64>, // since when
    // 2. games
    pub chosen: HashSet<String>,
    // 3. game files
    pub source: Source,
    pub download_folder: PathBuf,
    pub existing_folder: Option<PathBuf>,
    pub existing_found: HashSet<i64>,
    pub folder_problem: Option<String>,
    pub files_free: f64, // bytes free where the game files go
    pub home_free: f64,  // bytes free on this Mac's disk (Wine, prefixes)
    // 4. installing
    pub steps: Vec<String>,
    pub progress: Option<f64>,
    pub remaining: Option<String>,
    pub qr: Option<QrGrid>,
    pub status: Option<String>, // e.g. "Downloading Halo 2 (1 of 3)…"
    pub logging: bool,
    /// Setup finished: Some(native CE next) once the player goes on.
    pub finished: Option<bool>,
    waker: Waker,
    awake: Option<std::process::Child>,
    estimator: RateEstimator,
    collector: QrCollector,
    running: Option<Proc>,
    last_error: Option<String>,
    started: bool,
    next_dev_check: f64,
}

pub const RUNTIME_BYTES: f64 = 3e9; // Wine, the prefixes, the mods

impl Setup {
    pub fn new(waker: Waker) -> Self {
        Setup {
            page: Page::Checks, checks: vec![], checks_passed: false, needs_dev_tools: false, waiting_for_dev_tools: None,
            chosen: games().iter().filter(|g| g.available()).map(|g| g.id.clone()).collect(), source: Source::Steam,
            download_folder: paths::home().join("Game Files"), existing_folder: None, existing_found: HashSet::new(),
            folder_problem: None, files_free: 0.0, home_free: 0.0, steps: vec![], progress: None, remaining: None,
            qr: None, status: None, logging: true, finished: None, waker, awake: None,
            estimator: RateEstimator::default(), collector: QrCollector::default(), running: None, last_error: None,
            started: false, next_dev_check: 0.0,
        }
    }

    pub fn game_folder(&self) -> Option<&Path> {
        match self.source { Source::Steam => Some(&self.download_folder), Source::Existing => self.existing_folder.as_deref() }
    }
    pub fn present(&self) -> HashSet<i64> { if self.source == Source::Existing { self.existing_found.clone() } else { HashSet::new() } }
    pub fn plan(&self) -> DownloadPlan { DownloadPlan::new(&self.chosen, &self.present()) }
    pub fn chosen_games(&self) -> Vec<&'static Game> { games().iter().filter(|g| g.available() && self.chosen.contains(&g.id)).collect() }
    /// Do the chosen games play from MCC's files? (Halo CE alone doesn't: no game files page, no Steam.)
    pub fn needs_files(&self) -> bool { !depots_for(&self.chosen).is_empty() }
    pub fn space_ok(&self) -> bool { self.files_free + 0.5e9 >= self.plan().bytes() }
    pub fn can_install(&self) -> bool {
        !self.chosen_games().is_empty()
            && (!self.needs_files() || (self.space_ok() && self.game_folder().is_some() && self.folder_problem.is_none()))
            && (self.home_free == 0.0 || self.home_free >= RUNTIME_BYTES)
    }
    pub fn installing(&self) -> bool { self.page == Page::Installing }

    fn log(&self, text: &str) { if self.logging { proc::log("setup", text); } }

    pub fn start(&mut self) {
        if self.started { return; }
        self.started = true;
        self.log(&format!("setup started (home {})", paths::home().display()));
        // an MCC folder already on this Mac (e.g. the Project Reclaimer Mac port's, on an external drive) is offered
        // first: nothing needs downloading twice
        if let Some(found) = paths::var("DROPPOD_MCC_FROM").map(PathBuf::from).or_else(|| existing_mcc().into_iter().next()) {
            self.use_existing(&found);
            self.source = Source::Existing;
        }
        self.run_checks();
    }

    // 1. requirements
    pub fn run_checks(&mut self) {
        let mut list = vec![];
        let version = os_major_version();
        let apple_silicon = sysctl_int("hw.optional.arm64") == Some(1) || std::env::consts::ARCH == "aarch64";
        let mac_ok = apple_silicon && version >= 14;
        let n = app_name();
        if cfg!(target_os = "macos") { list.push(Check {
            id: "mac".into(),
            title: if apple_silicon { format!("Apple Silicon Mac, macOS {version}") } else { "This Mac has an Intel processor".into() },
            ok: Some(mac_ok),
            detail: if mac_ok { String::new() } else if apple_silicon {
                format!("{n} needs macOS 14 Sonoma or newer. Update macOS in System Settings > General > Software Update.")
            } else { format!("{n} needs an Apple Silicon Mac (M1 or newer).") },
        }); }
        self.home_free = paths::free_bytes(paths::home());
        let space_ok = self.home_free >= RUNTIME_BYTES;
        list.push(Check { id: "space".into(), title: format!("Free space on this {COMPUTER}: {} GB", (self.home_free / 1_073_741_824.0) as i64),
            ok: Some(space_ok), detail: if space_ok { String::new() } else {
                "The Windows layer needs about 3 GB. Free up some space (empty the Trash, delete big downloads), then press Check Again.".into() } });
        let tools = proc::succeeds("/usr/bin/xcode-select", &["-p"]);
        self.needs_dev_tools = !tools;
        list.push(Check { id: "tools".into(), title: "Apple's developer tools".into(), ok: Some(tools), detail: if tools { String::new() } else {
            "A small free Apple component the helper scripts use. Press the button below: macOS asks to install it (click Install, then Agree). It takes about 5 minutes.".into() } });
        if !proc::succeeds("/usr/bin/arch", &["-x86_64", "/usr/bin/true"]) {
            list.push(Check { id: "rosetta".into(), title: "Rosetta (Apple's Intel translator)".into(), ok: None,
                              detail: "Installed during setup; macOS asks for your password.".into() });
        }
        self.checks_passed = list.iter().all(|c| c.ok != Some(false));
        self.log(&format!("checks: {:?}", list.iter().map(|c| format!("{}={}", c.id, c.ok.map_or("info".into(), |o| o.to_string()))).collect::<Vec<_>>()));
        self.checks = list;
        self.page = Page::Checks;
        self.measure_space();
        // for automated end-to-end tests only: press Install as soon as the checks pass
        if self.checks_passed && paths::var("DROPPOD_TEST_AUTOINSTALL").as_deref() == Some("1") { self.install(); }
    }

    pub fn measure_space(&mut self) {
        if let Some(f) = self.game_folder() { self.files_free = paths::free_bytes(f); }
    }

    /// macOS's own "install developer tools" dialog; this window carries on by itself once they're there. If the
    /// dialog was cancelled or the install failed, the button comes back.
    pub fn install_dev_tools(&mut self) {
        let _ = std::process::Command::new("/usr/bin/xcode-select").arg("--install").status();
        self.waiting_for_dev_tools = Some(now());
        self.next_dev_check = now() + 5.0;
    }

    // 2-3. choices
    pub fn toggle(&mut self, id: &str) {
        if game(id).is_some_and(|g| !g.available()) { return; } // (Reach under Wine: greyed out)
        if self.chosen.remove(id) {
            for a in game(id).map(|g| g.addons.as_slice()).unwrap_or(&[]) { self.chosen.remove(&a.id); } // (an add-on only comes with its game)
        } else {
            self.chosen.insert(id.into());
        }
    }

    pub fn set_source(&mut self, s: Source) { self.source = s; self.measure_space(); }

    /// An MCC folder the player has: used where it is; only what it lacks is downloaded into it.
    pub fn use_existing(&mut self, url: &Path) {
        let folder = std::fs::canonicalize(url).unwrap_or_else(|_| url.to_path_buf());
        self.existing_found = present_depots(&folder);
        self.folder_problem = if looks_like_mcc(&folder) && self.existing_found.contains(&base_depot().id) { None } else {
            Some("That folder doesn't look like the Halo game files. Choose the folder with Data and the game folders (halo2, halo3) in it.".into())
        };
        // add-ons the folder already has come along (nothing to download): ODST in a full MCC folder
        let there: Vec<String> = games().iter().filter(|g| self.chosen.contains(&g.id)).flat_map(|g| &g.addons)
            .filter(|a| !a.depots.is_empty() && a.depots.iter().all(|d| self.existing_found.contains(&d.id)))
            .map(|a| a.id.clone()).collect();
        self.chosen.extend(there);
        let mut found: Vec<i64> = self.existing_found.iter().copied().collect();
        found.sort();
        self.log(&format!("existing folder {}: depots {found:?}", folder.display()));
        self.existing_folder = Some(folder);
        self.measure_space();
    }

    pub fn choose_folder(&mut self, existing: bool) {
        let dialog = rfd::FileDialog::new().set_can_create_directories(!existing).set_title(if existing {
            "Choose the folder your Halo game files are in (the one with Data and the game folders in it)."
        } else { "Choose where the game files go. An external drive works too." });
        let Some(url) = dialog.pick_folder() else { return };
        if existing { return self.use_existing(&url); }
        // a folder of its own inside the chosen one, unless the player picked an empty one or one named for it
        let empty = std::fs::read_dir(&url).map(|mut d| d.next().is_none()).unwrap_or(true);
        let named = url.file_name().is_some_and(|n| { let n = n.to_string_lossy(); n.contains("MCC") || n.contains("Halo") });
        self.download_folder = if empty || named { url } else { url.join("Halo Game Files") };
        self.folder_problem = None;
        self.measure_space();
    }

    // 4. install
    pub fn install(&mut self) {
        // (no game files needed: a good MCC folder the player has is still recorded, for a later Get)
        let existing = self.source == Source::Existing && (self.needs_files() || self.folder_problem.is_none());
        let folder = if existing { self.existing_folder.clone() } else { Some(self.download_folder.clone()) };
        let Some(folder) = folder else { return };
        self.page = Page::Installing;
        self.steps.clear();
        self.progress = None; self.remaining = None; self.qr = None; self.status = None; self.last_error = None;
        let wanted: Vec<String> = choice_order().into_iter().filter(|g| self.chosen.contains(g)).collect();
        self.log(&format!("installing {wanted:?} into {} ({})", folder.display(), if existing { "existing" } else { "steam" }));
        self.running = proc::installer(&self.waker, &[("DROPPOD_GAMES", wanted.join(" ")), ("DROPPOD_GAME_FILES", folder.to_string_lossy().into_owned()),
                                                      ("DROPPOD_SOURCE", if existing { "existing" } else { "steam" }.into())]);
        match &self.running {
            // a big download: the Mac must not fall asleep on it (the screen may)
            Some(p) => self.awake = std::process::Command::new("/usr/bin/caffeinate").args(["-i", "-w", &p.pid().to_string()])
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok(),
            None => self.page = Page::Failed("The installer couldn't start.".into()),
        }
    }

    pub fn update(&mut self) {
        if let Some(since) = self.waiting_for_dev_tools {
            let t = now();
            if t >= self.next_dev_check {
                self.next_dev_check = t + 5.0;
                if proc::succeeds("/usr/bin/xcode-select", &["-p"]) { self.waiting_for_dev_tools = None; self.run_checks(); }
                else if t - since > 20.0 && !proc::succeeds("/usr/bin/pgrep", &["-x", "Install Command Line Developer Tools"]) {
                    self.waiting_for_dev_tools = None; // Apple's installer isn't running any more: cancelled or failed
                }
            }
        }
        let Some(p) = self.running.as_mut() else { return };
        for m in p.poll() {
            match m {
                Msg::Line(line) => match SetupEvent::parse(&line) {
                    SetupEvent::Step(h) => if let Some(t) = friendly_step(&h) {
                        self.steps.push(t);
                        self.progress = None; self.remaining = None; self.status = None; self.qr = None;
                        self.estimator = RateEstimator::default();
                    },
                    SetupEvent::Status(s) => { self.qr = None; self.status = Some(s); } // (Steam accepted the scan)
                    SetupEvent::Progress(v) => { // signed in: the download started
                        self.qr = None;
                        self.progress = Some(v);
                        self.remaining = self.estimator.remaining(v, now());
                    }
                    SetupEvent::Steam(e) => { self.collector.handle(&e); if e == SteamEvent::QrEnd { self.qr = self.collector.current.clone(); } }
                    SetupEvent::Error(e) => if self.last_error.is_none() { self.last_error = Some(e) }, // the first error is the specific one
                    SetupEvent::Other => {}
                },
                Msg::Exit(code) => {
                    self.running = None;
                    self.qr = None;
                    if let Some(mut a) = self.awake.take() { let _ = a.kill(); let _ = a.wait(); }
                    self.log(&format!("installer exited {code}{}", self.last_error.as_ref().map_or(String::new(), |e| format!(": {e}"))));
                    self.page = if code == 0 { Page::Done } else {
                        Page::Failed(self.last_error.clone().unwrap_or_else(|| "The install stopped before it finished.".into()))
                    };
                    return;
                }
            }
        }
    }

    /// When `update` should run again even if nothing happens.
    pub fn wake_in(&self) -> Option<f64> { self.waiting_for_dev_tools.map(|_| (self.next_dev_check - now()).max(0.1)) }

    pub fn shutdown(&mut self) {
        if let Some(p) = &self.running { p.stop(); } // (the installer stops too; running it again picks up where it stopped)
        if let Some(mut a) = self.awake.take() { let _ = a.kill(); }
    }

    /// Setup finished: on to the library, here or (from the disk image) in the copy now in Applications, which is the
    /// one to use and the one that updates. `native_ce`: open the native Halo CE setup there first.
    pub fn finish(&mut self, native_ce: bool) { self.finished = Some(native_ce); }
}

/// macOS's major version (27 for 27.0.1).
fn os_major_version() -> i64 {
    sysctl_string("kern.osproductversion").and_then(|v| v.split('.').next()?.parse().ok()).unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
fn sysctl_string(_: &str) -> Option<String> { None }
#[cfg(not(target_os = "macos"))]
fn sysctl_int(_: &str) -> Option<i32> { None }

#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    let r = unsafe { libc::sysctlbyname(c.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) };
    (r == 0).then(|| String::from_utf8_lossy(&buf[..len.saturating_sub(1)]).into_owned())
}

#[cfg(target_os = "macos")]
fn sysctl_int(name: &str) -> Option<i32> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut v: i32 = 0;
    let mut len = std::mem::size_of::<i32>();
    let r = unsafe { libc::sysctlbyname(c.as_ptr(), (&mut v as *mut i32).cast(), &mut len, std::ptr::null_mut(), 0) };
    (r == 0).then_some(v)
}
