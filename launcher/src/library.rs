// SPDX-License-Identifier: GPL-3.0-or-later AND MIT
// The library's state: updates, the games (Play, or Get to download one later), what each plays with (its mods in the
// catalogue), their guided setups, Settings, and the game that's running. The work is done by the scripts install.sh
// put in $DROPPOD_HOME/scripts; this only runs them and keeps what they say. Ported from Library.swift, Guided.swift
// and Settings.swift, adapted from the Project Reclaimer Mac port (portions MIT, see LICENSE.reclaimer-mac-port).
use crate::core::*;
use crate::paths;
use crate::proc::{self, Msg, Proc, Waker};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Step { Waiting, Working(String), Done(String), Warning(String) }

impl Step {
    pub fn finished(&self) -> bool { !matches!(self, Step::Waiting | Step::Working(_)) }
    pub fn text(&self) -> &str {
        match self { Step::Waiting => "Waiting…", Step::Working(s) | Step::Done(s) | Step::Warning(s) => s }
    }
}

/// What a game's tile shows. NeedsSetup: what it plays with isn't set up (yet, or any more); NotHere: its mod
/// doesn't run on this computer (Reach under Wine), so it's greyed out with why.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileState { Ready, NotDownloaded, Downloading, Playing, Unavailable, NeedsSetup, NotHere }

/// The sheet over the library. Mod: a mod set up with its game; Disc: one set up from the player's Xbox disc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sheet { Settings, Mod, Disc, SignIn }

/// A question over the window and what its buttons do.
#[derive(Debug, Clone, PartialEq)]
pub enum Act { Get(String), PlayWith(String, String), RestoreGraphics, SwitchSteam, Uninstall(bool), Quit, Close }

#[derive(Debug, Clone, PartialEq)]
pub struct Dialog { pub title: String, pub message: String, pub buttons: Vec<(String, Act)> }

// MARK: one install.sh run from the library

#[derive(Debug, Clone, PartialEq)]
pub enum Phase { Running, Failed(String), Done }

/// One run of install.sh from the library (a tile's Get, or setting up a mod): the steps so far, the current one's
/// status and progress, and the Steam QR code when a sign-in is needed.
pub struct InstallRun {
    pub purpose: String, // the game a Get downloads, or the mod being set up
    pub phase: Phase,
    pub steps: Vec<String>,
    pub status: Option<String>,
    pub progress: Option<f64>,
    pub remaining: Option<String>,
    pub qr: Option<QrGrid>,
    process: Option<Proc>,
    estimator: RateEstimator,
    collector: QrCollector,
    first_error: Option<String>,
}

impl InstallRun {
    pub fn new(purpose: &str) -> Self {
        InstallRun { purpose: purpose.into(), phase: Phase::Running, steps: vec![], status: None, progress: None,
                     remaining: None, qr: None, process: None, estimator: RateEstimator::default(),
                     collector: QrCollector::default(), first_error: None }
    }

    pub fn current(&self) -> Option<&String> { self.steps.last() }

    /// Starts install.sh; false (and failed) when it couldn't.
    fn start(&mut self, waker: &Waker, choices: &[(&str, String)]) -> bool {
        self.process = proc::installer(waker, choices);
        if self.process.is_none() {
            self.phase = Phase::Failed(format!("The installer couldn't start. Use Settings > Repair {}.", app_name()));
        }
        self.process.is_some()
    }

    pub fn stop(&self) { if let Some(p) = &self.process { p.stop(); } }

    /// What it said; Some(ok) once it has ended.
    fn update(&mut self) -> Option<bool> {
        let msgs = self.process.as_mut()?.poll();
        let mut ended = None;
        for m in msgs {
            match m {
                Msg::Line(l) => self.handle(&l),
                Msg::Exit(code) => {
                    self.process = None;
                    self.qr = None;
                    self.phase = if code == 0 { Phase::Done } else {
                        Phase::Failed(self.first_error.clone()
                            .unwrap_or_else(|| "It stopped before it finished. Press Try Again to pick up where it stopped.".into()))
                    };
                    ended = Some(code == 0);
                }
            }
        }
        ended
    }

    pub fn handle(&mut self, line: &str) {
        match SetupEvent::parse(line) {
            SetupEvent::Step(heading) => if let Some(t) = friendly_step(&heading) {
                self.steps.push(t);
                self.status = None; self.progress = None; self.remaining = None; self.qr = None;
                self.estimator = RateEstimator::default();
            },
            SetupEvent::Status(s) => { self.qr = None; self.status = Some(s); }
            SetupEvent::Progress(v) => {
                self.qr = None;
                self.progress = Some(v);
                self.remaining = self.estimator.remaining(v, now());
            }
            SetupEvent::Steam(e) => {
                self.collector.handle(&e);
                if e == SteamEvent::QrEnd { self.qr = self.collector.current.clone(); }
            }
            SetupEvent::Error(e) => if self.first_error.is_none() { self.first_error = Some(e) },
            SetupEvent::Other => {}
        }
    }
}

/// What a run is for, and so what happens when it ends.
#[derive(Debug, Clone, PartialEq)]
enum RunFor { Get(String), AddOn(String), SetUp(String) }

// MARK: Settings

/// Settings: the game files location, repair, logs and uninstall, plus the fixes that would otherwise need Terminal.
/// Each runs one tools.py command (or install.sh) and says what happened in plain words.
#[derive(Default)]
pub struct Settings {
    pub busy: Option<String>, // what's running now
    pub result: Option<(String, bool)>,
    pub free_bytes: f64,
    pub repair_progress: Option<f64>,
    pub repair_remaining: Option<String>,
    pub repair_qr: Option<QrGrid>,
    pub crash_bytes: f64, // the games' crash dumps in the game files folder
    running: Option<(Tool, Proc)>,
    last: String,
    collector: QrCollector,
    estimator: RateEstimator,
    repair_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tool { CleanCrash, Repair, RepairSetup, GameFiles(String), ForceQuit, ResetGraphics, ForgetSteam, Report, Uninstall }

impl Settings {
    pub fn say(&mut self, text: impl Into<String>, ok: bool) { self.result = Some((text.into(), ok)); }
}

/// Only an exact "ok" (or "ok <detail>") is success: a tool that crashed or said nothing didn't work.
fn succeeded(line: &str) -> bool { line == "ok" || line.starts_with("ok ") }
fn error_in(line: &str) -> Option<String> { line.strip_prefix("error: ").map(str::to_owned) }

// MARK: native Halo CE

#[derive(Debug, Clone, PartialEq)]
pub enum NativePage { Explain, Working, Failed(String), Done }

/// "Set up native Halo CE": the player's own Xbox disc image, checked; ChupathingyCE's latest release; Halo's maps
/// copied out of the image into the port's own data folder (what its first start would do); then checked again.
pub struct Native {
    pub page: NativePage,
    pub image: Option<PathBuf>,
    pub image_summary: Option<String>, // what the check found in the image
    pub image_problem: Option<String>,
    pub checking: bool,
    pub step: usize, // index into steps() while working
    pub progress: Option<f64>,
    pub status: Option<String>,
    running: Option<(NativeTool, Proc)>,
    last: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeTool { Check, Get, Import, Status }

pub fn native_steps() -> Vec<String> {
    let m = disc_mod();
    vec!["Check your disc image".into(), format!("Download the native port, {} ({} MB)", m.name, m.mb as i64),
         "Copy Halo's maps from your disc (about 2 GB)".into(), "Make sure it's ready".into()]
}

impl Default for Native {
    fn default() -> Self {
        Native { page: NativePage::Explain, image: None, image_summary: None, image_problem: None, checking: false,
                 step: 0, progress: None, status: None, running: None, last: String::new() }
    }
}

/// What `xiso.py check` said ("ok <maps folder> <files> <bytes>", or "error: <why>"), in words.
pub fn disc_check(line: &str) -> Result<String, String> {
    let parts: Vec<&str> = line.split(' ').filter(|p| !p.is_empty()).collect();
    let found = (parts.len() == 4 && parts[0] == "ok").then(|| Some((parts[2].parse::<i64>().ok()?, parts[3].parse::<f64>().ok()?))).flatten();
    match found {
        Some((files, bytes)) => Ok(format!("An Xbox disc image of Halo: Combat Evolved: its maps folder has {files} files ({}).", gigabytes(bytes))),
        None => Err(error_in(line).unwrap_or_else(|| "That file couldn't be read.".into())),
    }
}

// MARK: the library

pub struct Library {
    pub update: Step,
    pub update_progress: Option<f64>,
    pub network: Option<Step>,
    pub controllers: Option<Step>,
    pub config: Option<AppConfig>,
    pub present: HashSet<i64>,
    pub files_found: bool, // false: the game files folder is gone (e.g. its drive isn't connected)
    pub mods_there: HashSet<String>, // mods whose program is in mods/<id>/
    pub native_maps_there: bool,     // Halo's Xbox maps, imported for the disc mod (ChupathingyCE)
    pub setup_mod: Option<String>,   // the mod the Mod sheet sets up
    pub playing: Option<String>,
    pub game_error: Option<String>,
    pub notice: Option<String>,
    pub sheet: Option<Sheet>,
    // an install.sh run: a tile's Get (purpose: the game) or setting up a mod (purpose: the mod)
    pub run: Option<InstallRun>,
    pub get_error: Option<String>,
    pub dialog: Option<Dialog>,
    pub settings: Settings,
    pub native: Native,
    pub logging: bool,
    /// Asks the window to come to the front (a game ended) or to quit (uninstalled, or restarting to update).
    pub bring_to_front: bool,
    pub quit: bool,
    waker: Waker,
    run_for: Option<RunFor>,
    tasks: Vec<(Task, Proc)>,
    updater: Option<Proc>,
    game_process: Option<Proc>, // play.sh
    game_app: Option<Proc>,     // `open -W` of the game app: running while the game app is
    playing_engine: String,
    played_at: f64,
    game_gone_since: Option<f64>,
    relaunch_requested: bool,
    checking_crash: bool,
    crash: Option<String>,
    started: f64,
    next_tick: f64,
    watch: Option<std::sync::mpsc::Receiver<bool>>,
    did_start: bool,
}

/// A helper the library started, and what its lines mean.
#[derive(Debug, Clone, PartialEq)]
enum Task { PlayWith, NetCheck, NetFix, ControllerCheck, CrashCheck { early: bool }, CrashReports }

impl Library {
    pub fn new(waker: Waker) -> Self {
        Library {
            update: Step::Waiting, update_progress: None, network: None, controllers: None, config: None,
            present: HashSet::new(), files_found: true, mods_there: HashSet::new(), native_maps_there: false,
            setup_mod: None, playing: None, game_error: None, notice: None, sheet: None, run: None, get_error: None,
            dialog: None, settings: Settings::default(), native: Native::default(), logging: true,
            bring_to_front: false, quit: false, waker, run_for: None, tasks: vec![], updater: None,
            game_process: None, game_app: None, playing_engine: String::new(), played_at: 0.0, game_gone_since: None,
            relaunch_requested: false, checking_crash: false, crash: None, started: now(), next_tick: 0.0, watch: None,
            did_start: false,
        }
    }

    fn log(&self, text: &str) { if self.logging { proc::log("library", text); } }

    pub fn game_files(&self) -> Option<PathBuf> { self.config.as_ref().map(|c| PathBuf::from(&c.game_files)) }
    pub fn installed(&self) -> Vec<String> { installed_games(&self.present) }
    pub fn installing(&self) -> bool { self.run.as_ref().is_some_and(|r| r.phase == Phase::Running) }
    /// The game a running Get downloads.
    pub fn getting(&self) -> Option<&str> {
        self.run.as_ref().filter(|r| r.phase == Phase::Running && game(&r.purpose).is_some()).map(|r| r.purpose.as_str())
    }

    pub fn state(&self, g: &Game) -> TileState {
        if !g.available() { return TileState::NotHere; }
        if self.playing.as_deref() == Some(&g.id) { return TileState::Playing; }
        let Some(m) = mod_(&self.engine(&g.id)) else { return TileState::Unavailable };
        if m.depots.is_empty() { return if self.ready(&m.id) { TileState::Ready } else { TileState::NeedsSetup }; }
        if self.getting() == Some(&g.id) { return TileState::Downloading; }
        if !self.files_found { return TileState::Unavailable; }
        if !self.installed().contains(&g.id) { return TileState::NotDownloaded; }
        if self.ready(&m.id) { TileState::Ready } else { TileState::NeedsSetup }
    }

    /// Is what a game plays with set up: the mod's program, the MCC files it needs, and the disc's maps for a disc mod?
    pub fn ready(&self, option: &str) -> bool {
        let Some(m) = mod_(option) else { return false };
        if !self.mods_there.contains(&m.id) { return false; }
        if m.setup == "xbox-disc" && !self.native_maps_there { return false; }
        m.depots.is_empty() || (self.present.contains(&base_depot().id) && m.depots.iter().all(|d| self.present.contains(d)))
    }

    /// The game's "Play with" choice (a Mod.id): its first option unless the player picked another.
    pub fn engine(&self, id: &str) -> String {
        match &self.config {
            Some(c) => c.engine(id),
            None => game(id).and_then(|g| g.options.first()).map(|m| m.id.clone()).unwrap_or_default(),
        }
    }

    fn keep(&mut self, task: Task, p: Option<Proc>) { if let Some(p) = p { self.tasks.push((task, p)); } }

    /// The player picked what a game plays with: remembered in config.json (tools.py play-with). An extra that isn't
    /// set up yet opens its guided setup instead; its last step offers the switch.
    pub fn set_play_with(&mut self, id: &str, option: &str) {
        if !self.ready(option) { return self.open_setup(option); }
        if self.engine(id) == option { return; }
        if let Some(c) = self.config.as_mut() {
            c.play_with.get_or_insert_default().insert(id.into(), option.into());
        }
        self.game_error = None;
        self.log(&format!("play with: {id} = {option}"));
        let p = proc::python(&self.waker, "tools.py", &["play-with", id, option]);
        self.keep(Task::PlayWith, p);
        if mod_(option).is_some_and(|m| m.has("loopback")) { self.check_network(); }
    }

    /// The guided setup of what a game plays with (a Mod.id): from the player's disc, or with its game.
    pub fn open_setup(&mut self, option: &str) {
        let Some(m) = mod_(option) else { return };
        if m.setup == "xbox-disc" {
            self.native_reset();
            self.set_sheet(Some(Sheet::Disc));
        } else {
            if self.run.as_ref().is_some_and(|r| r.purpose == m.id && r.phase != Phase::Running) { self.run = None; } // (a finished one: start afresh)
            self.setup_mod = Some(m.id.clone());
            self.set_sheet(Some(Sheet::Mod));
        }
    }

    pub fn set_sheet(&mut self, s: Option<Sheet>) {
        if s != self.sheet { self.log(&format!("sheet: {}", s.map_or("none".into(), |s| format!("{s:?}").to_lowercase()))); }
        if s == Some(Sheet::Settings) { self.measure(); }
        self.sheet = s;
    }

    pub fn close_sheet(&mut self) {
        if self.run.as_ref().is_some_and(|r| mod_(&r.purpose).is_some() && r.phase != Phase::Running) { self.run = None; }
        if self.sheet == Some(Sheet::Disc) { self.native_stop(); }
        self.set_sheet(None);
    }

    /// What Get downloads for a game: its depots (and those of the add-ons the player chose) that aren't there yet.
    pub fn get_plan(&self, g: &Game) -> DownloadPlan {
        let chose: HashSet<String> = self.config.as_ref().and_then(|c| c.wanted.clone()).unwrap_or_default().into_iter().collect();
        let mut chosen: HashSet<String> = g.addons.iter().map(|a| a.id.clone()).filter(|a| chose.contains(a)).collect();
        chosen.insert(g.id.clone());
        DownloadPlan::new(&chosen, &self.present)
    }

    /// An add-on of an installed game that isn't downloaded (Settings offers it).
    pub fn missing(&self, a: &AddOn) -> bool {
        self.config.as_ref().is_some_and(|c| c.games.contains(&a.game)) && a.depots.iter().any(|d| !self.present.contains(&d.id))
    }

    fn wanted(&self) -> HashSet<String> {
        let c = self.config.as_ref();
        c.and_then(|c| c.wanted.clone()).or_else(|| c.map(|c| c.games.clone())).unwrap_or_default().into_iter().collect()
    }

    /// Settings' Get for an add-on: chosen from now on, downloaded like a tile's Get (its game's tile shows the progress).
    pub fn get_add_on(&mut self, a: &AddOn) {
        if self.config.is_none() { return; }
        self.get_error = None;
        let mut w = self.wanted();
        w.insert(a.game.clone());
        w.insert(a.id.clone());
        self.start_run(&a.game, w, RunFor::AddOn(a.id.clone()));
    }

    pub fn start(&mut self) {
        if self.did_start { return; }
        self.did_start = true;
        self.log(&format!("started (home {}, app {})", paths::home().display(), paths::app_version()));
        self.reload();
        self.run_update();
    }

    /// config.json and which depots the game files folder has (after Get, Settings > Game files, Repair).
    pub fn reload(&mut self) {
        self.config = AppConfig::load(&paths::config_path());
        if let Some(folder) = self.game_files() {
            self.files_found = folder.exists();
            if self.files_found { self.present = present_depots(&folder); }
        }
        let dir = paths::home().join("mods");
        self.mods_there = mods().iter().filter(|m| m.program_there(&dir)).map(|m| m.id.clone()).collect();
        self.native_maps_there = paths::native_data_dir().join("maps/ui.map").exists();
    }

    // 1. updates (the app itself: the mods update themselves when they start), then the checks
    fn run_update(&mut self) {
        if std::env::args().any(|a| a == "--relaunched") { // (just updated: never restart twice in a row)
            self.set_update(Step::Done("Updated to the latest version".into()));
            return self.after_update();
        }
        self.set_update(Step::Working("Checking for updates…".into()));
        self.updater = proc::python(&self.waker, "updater.py", &["run"]);
        if self.updater.is_none() {
            self.set_update(Step::Warning("Couldn't check for updates. You can still play.".into()));
            self.after_update();
        }
    }

    fn set_update(&mut self, s: Step) {
        if s != self.update { self.log(&format!("update: {s:?}")); }
        self.update = s;
    }

    fn updater_line(&mut self, line: &str) {
        match UpdaterEvent::parse(line) {
            UpdaterEvent::Status(s) => { self.set_update(Step::Working(s)); self.update_progress = None; }
            UpdaterEvent::Progress(v) => self.update_progress = Some(v),
            UpdaterEvent::Relaunch => self.relaunch_requested = true,
            UpdaterEvent::Done(s) => self.set_update(Step::Done(match s.as_str() {
                "updated" => "Updated to the latest version".into(),
                "up to date" => "Up to date".into(),
                _ => { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() }
            })),
            UpdaterEvent::Offline(_) => self.set_update(Step::Warning("Couldn't check for updates right now. You can still play.".into())),
            UpdaterEvent::Error(e) => self.set_update(Step::Warning(format!("An update didn't finish: {e}. You can still play."))),
            UpdaterEvent::Other => {}
        }
    }

    fn updater_exit(&mut self) {
        self.updater = None;
        self.update_progress = None;
        if self.relaunch_requested { return self.relaunch(); }
        if !self.update.finished() { self.set_update(Step::Warning("Couldn't check for updates. You can still play.".into())); }
        self.after_update();
    }

    fn after_update(&mut self) {
        self.reload();
        self.check_controllers();
        let installed = self.installed();
        if games().iter().any(|g| installed.contains(&g.id) && mod_(&self.engine(&g.id)).is_some_and(|m| m.has("loopback"))) {
            self.check_network();
        }
        // the disk image's setup offered the disc mod (native Halo CE) and handed over to this copy
        if std::env::args().any(|a| a == "--setup-native-ce") && self.sheet.is_none() {
            self.open_setup(&disc_mod().id.clone());
        }
    }

    fn relaunch(&mut self) {
        self.set_update(Step::Working(format!("Restarting {}…", app_name())));
        // wait until this instance has really quit, or `open` would just bring the quitting one forward
        if let Some(b) = paths::bundle() {
            let _ = std::process::Command::new("/bin/sh")
                .args(["-c", "while kill -0 $1 2>/dev/null; do sleep 0.2; done; /usr/bin/open -n \"$0\" --args --relaunched"])
                .arg(b).arg(std::process::id().to_string()).spawn();
        }
        self.quit = true;
    }

    /// The local network addresses Project Reclaimer needs (added by a startup task) can go missing, e.g. right after
    /// a restart or a macOS update; without them Halo 3 can't start a game. Fixed with one password box.
    pub fn check_network(&mut self) {
        let p = proc::python(&self.waker, "tools.py", &["net-check"]);
        self.keep(Task::NetCheck, p);
    }

    fn set_network(&mut self, s: Step) {
        if Some(&s) != self.network.as_ref() { self.log(&format!("network: {s:?}")); }
        self.network = Some(s);
    }

    /// macOS's game controller service sometimes gets stuck for good: Bluetooth shows the controller connected, but
    /// no app (the games included) sees it until the Mac restarts. Say so before Play instead of leaving people guessing.
    pub fn check_controllers(&mut self) {
        let p = proc::python(&self.waker, "tools.py", &["controller-check"]);
        self.keep(Task::ControllerCheck, p);
    }

    fn task_line(&mut self, task: &Task, line: &str) {
        match task {
            Task::PlayWith => if let Some(e) = line.strip_prefix("error: ") {
                self.game_error = Some(format!("That choice couldn't be saved: {e}"));
            },
            Task::NetCheck => if line == "missing" {
                self.set_network(Step::Working("Fixing the network setup Halo 3 needs. macOS will ask for your Mac password.".into()));
                let p = proc::python(&self.waker, "tools.py", &["net-fix"]);
                self.keep(Task::NetFix, p);
            },
            Task::NetFix => {
                if line == "ok" { self.set_network(Step::Done("Fixed".into())); }
                else if let Some(e) = line.strip_prefix("error: ") { self.set_network(Step::Warning(e.into())); }
            }
            Task::ControllerCheck => if line == "stuck" {
                let s = Step::Warning("Your Mac's game controller service is stuck, so games won't see a controller. \
                                       Restart your Mac to fix it (Apple menu > Restart). Keyboard and mouse still work.".into());
                self.log(&format!("controllers: {s:?}"));
                self.controllers = Some(s);
            },
            Task::CrashCheck { .. } => if let Some(c) = line.strip_prefix("crash: ") { self.crash = Some(c.into()); },
            Task::CrashReports => self.settings.crash_bytes = line.parse().unwrap_or(0.0),
        }
    }

    fn task_exit(&mut self, task: &Task) {
        match task {
            Task::NetFix => if self.network.as_ref().is_some_and(|n| !n.finished()) {
                self.set_network(Step::Warning("The network setup couldn't be fixed. Halo 3 may not start.".into()));
            },
            Task::CrashCheck { early } => {
                self.checking_crash = false;
                if let Some(c) = self.crash.take() { self.set_playing(None); self.game_gone_since = None; self.set_game_error(Some(c)); }
                else { self.game_stopped(*early); }
                self.bring_to_front = true;
            }
            _ => {}
        }
    }

    fn set_playing(&mut self, p: Option<String>) {
        if p != self.playing { self.log(&format!("playing: {}", p.as_deref().unwrap_or("nothing"))); }
        self.playing = p;
    }

    fn set_game_error(&mut self, e: Option<String>) {
        if let Some(e) = &e { self.log(&format!("game error: {e}")); }
        self.game_error = e;
    }

    // 2. play
    pub fn play(&mut self, id: &str, chosen: Option<&str>) {
        if self.playing.is_some() { return; }
        self.notice = None;
        self.game_error = None;
        self.reload();
        let engine = chosen.map(str::to_owned).unwrap_or_else(|| self.engine(id));
        let m = mod_(&engine);
        if !self.files_found && m.is_some_and(|m| !m.depots.is_empty()) {
            return self.set_game_error(Some("The game files aren't there. Connect the drive they're on, or choose where they are now in Settings.".into()));
        }
        self.set_playing(Some(id.into()));
        self.playing_engine = engine.clone();
        self.played_at = now();
        self.game_gone_since = None;
        self.game_app = None;
        self.game_process = None;
        // Wine mods start through their game app, so the Mac shows them as the mod (menu bar, Dock, Cmd-Tab, Game Mode)
        // rather than "wine". A Mac mod is an app of its own; an install from before the game apps, play.sh
        let app = paths::game_app(m.map_or("", |m| m.name.as_str()));
        if m.is_some_and(|m| m.on_wine()) && app.join("Contents/MacOS/launch").exists() {
            let args: Vec<String> = ["-n", "-W", "--env"].iter().map(|s| s.to_string())
                .chain([format!("DROPPOD_HOME={}", paths::home().display()), app.to_string_lossy().into_owned(),
                        "--args".into(), id.into(), engine])
                .collect();
            self.game_app = Proc::spawn(&self.waker, "/usr/bin/open", &args, &[]);
            if self.game_app.is_none() { self.game_stopped(true); }
            return;
        }
        self.game_process = if cfg!(target_os = "macos") {
            Proc::spawn(&self.waker, "/bin/zsh", &[paths::scripts_dir().join("play.sh").to_string_lossy().into_owned(), id.into(), engine], &[])
        } else {
            proc::python(&self.waker, "play.py", &[id, &engine]) // (Windows, Linux: setup.py put it there)
        };
        if self.game_process.is_none() { self.game_stopped(true); }
    }

    /// The running game, as it shows up (its launcher may exit while it keeps running): most Wine mods' whole Wine session
    /// (this install's wineserver, which outlives the game when it restarts itself to update or for some settings), a
    /// Mac mod's process as the catalogue gives it. Reach uses proc::mod_running instead to exclude EdgeUpdate.
    fn game_pattern(engine: &str) -> String {
        let Some(m) = mod_(engine) else { return "^$".into() };
        if m.on_wine() { return extended_regex_escaped(&format!("{}/wine/", paths::home().display())) + ".*/wineserver"; }
        m.process.clone()
    }

    pub fn title_of(id: &str) -> String { game(id).map_or(id.to_string(), |g| g.title.clone()) }

    /// The game ended: if its log shows a crash, say what happened; within seconds of Play, say it closed right away.
    /// Either way the library comes back to the front.
    fn game_ended(&mut self, early: bool) {
        if self.checking_crash { return; }
        let Some(id) = self.playing.clone() else { return };
        self.checking_crash = true;
        self.crash = None;
        match proc::python(&self.waker, "tools.py", &["crash-check", &id]) {
            Some(p) => self.tasks.push((Task::CrashCheck { early }, p)),
            None => { // couldn't even run the check: never stay stuck on "Playing"
                self.checking_crash = false;
                self.game_stopped(early);
            }
        }
    }

    fn game_stopped(&mut self, early: bool) {
        let id = self.playing.clone();
        self.set_playing(None);
        self.game_gone_since = None;
        if let (true, Some(id)) = (early, id) {
            self.set_game_error(Some(format!("{} closed right after starting. Press Play to try again. If it keeps happening, \
                                              use Settings > Make a report.", Self::title_of(&id))));
        }
    }

    /// Called by Settings > Force quit: the library stays, ready to Play again.
    fn game_force_quit(&mut self) {
        self.set_playing(None);
        self.game_gone_since = None;
        self.notice = Some("The game was stopped. Press Play to start it again.".into());
    }

    // 3. install.sh runs: a tile's Get, and setting up a mod with its game (finished steps are skipped, so both only
    // add what's missing)
    pub fn get(&mut self, id: &str) {
        if self.config.is_none() { return; }
        let mut w = self.wanted();
        w.insert(id.into());
        self.get_error = None;
        self.start_run(id, w, RunFor::Get(id.into()));
    }

    /// A tile's Get asks first: how much, from where, into where.
    pub fn confirm_get(&mut self, g: &Game) {
        let plan = self.get_plan(g);
        let into = self.config.as_ref().map_or("your game files folder".to_string(), |c| c.game_files.clone());
        self.dialog = Some(Dialog {
            title: format!("Download {}?", g.title),
            message: format!("{} from your Steam account, into {into}. You may need to scan a QR code with the Steam app on your phone.",
                             steam_size(plan.gib())),
            buttons: vec![("Download".into(), Act::Get(g.id.clone())), ("Cancel".into(), Act::Close)],
        });
    }

    pub fn set_up(&mut self, m: &Mod) {
        if self.config.is_none() { return; }
        self.setup_mod = Some(m.id.clone());
        self.set_sheet(Some(Sheet::Mod));
        let mut w = self.wanted();
        for g in games().iter().filter(|g| g.options.contains(m)) { w.insert(g.id.clone()); }
        self.start_run(&m.id, w, RunFor::SetUp(m.id.clone()));
    }

    fn start_run(&mut self, purpose: &str, wanted: HashSet<String>, run_for: RunFor) {
        if self.installing() { return; }
        let Some(c) = self.config.clone() else { return };
        let mut r = InstallRun::new(purpose);
        let list: Vec<String> = choice_order().into_iter().filter(|g| wanted.contains(g)).collect();
        self.log(&format!("installing: {purpose}"));
        let ok = r.start(&self.waker, &[("DROPPOD_GAMES", list.join(" ")), ("DROPPOD_GAME_FILES", c.game_files.clone()),
                                       ("DROPPOD_SOURCE", c.source.clone())]);
        self.run = Some(r);
        self.run_for = Some(run_for);
        if !ok { self.run_ended(false); }
    }

    fn run_ended(&mut self, ok: bool) {
        self.reload();
        if self.sheet == Some(Sheet::SignIn) { self.set_sheet(None); }
        let failed = match self.run.as_ref().map(|r| &r.phase) { Some(Phase::Failed(e)) => Some(e.clone()), _ => None };
        match self.run_for.take() {
            Some(RunFor::Get(id)) => {
                if ok { self.notice = Some(format!("{} is ready. Press Play.", Self::title_of(&id))); } else if failed.is_some() { self.get_error = failed; }
                self.run = None;
            }
            Some(RunFor::AddOn(a)) => {
                if ok {
                    if let Some(a) = add_on(&a) {
                        self.notice = Some(format!("{}'s {} are ready.", game(&a.game).map_or("", |g| g.short.as_str()), a.name.to_lowercase()));
                    }
                } else if failed.is_some() { self.get_error = failed; }
                self.run = None;
            }
            Some(RunFor::SetUp(_)) | None => {} // (the sheet shows how it went; closing it clears the run)
        }
    }

    pub fn stop_run(&self) { if let Some(r) = &self.run { r.stop(); } }

    /// Everything the helpers said since the last frame, and the once-a-second checks while a game runs or the
    /// updater does. Runs even while the window is hidden.
    pub fn update(&mut self) {
        // helpers
        let mut tasks = std::mem::take(&mut self.tasks);
        for (task, p) in tasks.iter_mut() {
            for m in p.poll() {
                match m { Msg::Line(l) => self.task_line(task, &l), Msg::Exit(_) => self.task_exit(task) }
            }
        }
        tasks.retain(|(_, p)| p.is_running());
        tasks.append(&mut self.tasks);
        self.tasks = tasks;
        // the updater
        if let Some(mut u) = self.updater.take() {
            let mut exited = false;
            for m in u.poll() { match m { Msg::Line(l) => self.updater_line(&l), Msg::Exit(_) => exited = true } }
            if exited { self.updater_exit(); } else { self.updater = Some(u); }
        }
        // the install run; a Get's Steam QR code comes up in a sheet of its own
        if let Some(ended) = self.run.as_mut().and_then(|r| r.update()) { self.run_ended(ended); }
        if let Some(qr) = self.run.as_ref().filter(|r| game(&r.purpose).is_some()).map(|r| r.qr.is_some()) {
            if qr && self.sheet.is_none() { self.set_sheet(Some(Sheet::SignIn)); }
            if !qr && self.sheet == Some(Sheet::SignIn) { self.set_sheet(None); }
        }
        // the game
        for p in [&mut self.game_process].into_iter().flatten() { p.poll(); }
        if let Some(p) = self.game_app.as_mut() {
            for m in p.poll() {
                if let Msg::Exit(code) = m {
                    if code != 0 && self.playing.is_some() && now() - self.played_at < 20.0 {
                        proc::log("library", &format!("game app didn't open (open said {code})"));
                        self.game_stopped(true);
                    }
                }
            }
        }
        self.settings_update();
        self.native_update();
        if let Some(rx) = &self.watch {
            if let Ok(up) = rx.try_recv() { self.watch = None; self.game_watch(up); }
        }
        // once a second while something is being waited on
        if self.playing.is_some() || self.updater.is_some() {
            let t = now();
            if t >= self.next_tick { self.next_tick = t + 1.0; self.tick(); }
        }
    }

    /// When `update` should run again even if nothing happens (None: only when something does).
    pub fn wake_in(&self) -> Option<f64> {
        (self.playing.is_some() || self.updater.is_some()).then(|| (self.next_tick - now()).max(0.05))
    }

    fn tick(&mut self) {
        // a stuck update (it has its own network timeouts, so this is a last resort) never keeps Play off for good
        if let Some(u) = &self.updater {
            if now() - self.started > 600.0 {
                u.stop();
                self.updater = None;
                self.set_update(Step::Warning("The update took too long and was stopped. You can still play.".into()));
            }
        }
        if self.playing.is_none() || self.checking_crash || self.watch.is_some() { return; }
        let (tx, rx) = std::sync::mpsc::channel();
        let w = self.waker.clone();
        let done = move |up| { let _ = tx.send(up); w(); };
        if cfg!(target_os = "macos") && mod_(&self.playing_engine).is_some_and(|m| m.has("chromehoist")) {
            proc::mod_running(self.playing_engine.clone(), done);
        } else {
            proc::process_running(Self::game_pattern(&self.playing_engine), done);
        }
        self.watch = Some(rx);
    }

    fn game_watch(&mut self, game_up: bool) {
        if self.playing.is_none() || self.checking_crash { return; }
        if self.game_process.as_ref().is_some_and(|p| p.is_running()) || self.game_app.as_ref().is_some_and(|p| p.is_running()) || game_up {
            self.game_gone_since = None;
            return;
        }
        if now() - self.played_at < 20.0 { return self.game_ended(true); }
        let since = *self.game_gone_since.get_or_insert(now());
        // a moment's grace: a Mac mod may restart itself
        if now() - since > 3.0 { self.game_ended(false); }
    }

    pub fn shutdown(&mut self) {
        self.log("quitting");
        self.stop_run();
        self.native_stop();
    }

    // MARK: questions

    pub fn choose(&mut self, act: Act) {
        self.dialog = None;
        match act {
            Act::Get(id) => self.get(&id),
            Act::PlayWith(id, option) => self.set_play_with(&id, &option),
            Act::RestoreGraphics => self.tool("Restoring graphics settings…", &["reset-graphics"], Tool::ResetGraphics),
            Act::SwitchSteam => self.tool("Forgetting the Steam account…", &["forget-steam"], Tool::ForgetSteam),
            Act::Uninstall(delete) => {
                let mut args = vec!["uninstall"];
                if delete { args.push("delete-game-files"); }
                self.tool("Uninstalling…", &args, Tool::Uninstall);
            }
            Act::Quit => self.quit = true,
            Act::Close => {}
        }
    }

    /// "Play with" for a game with more than one choice: what each is, which one is chosen.
    pub fn ask_play_with(&mut self, g: &Game) {
        let current = self.engine(&g.id);
        let mut buttons: Vec<(String, Act)> = g.options.iter().map(|o| {
            let label = if o.id == current { format!("{} (chosen)", o.name) } else if self.ready(&o.id) { o.name.clone() } else { format!("{} (set up…)", o.name) };
            (label, Act::PlayWith(g.id.clone(), o.id.clone()))
        }).collect();
        buttons.push(("Cancel".into(), Act::Close));
        self.dialog = Some(Dialog { title: format!("Play {} with", g.short),
                                    message: "What Play starts. Your choice is remembered for this game.".into(), buttons });
    }

    pub fn ask_restore_graphics(&mut self) {
        self.dialog = Some(Dialog { title: "Restore recommended graphics?".into(),
            message: "Replaces Halo 3's graphics settings and fills in missing Reach settings (60 fps, V-Sync). Other settings stay.".into(),
            buttons: vec![("Restore".into(), Act::RestoreGraphics), ("Cancel".into(), Act::Close)] });
    }

    pub fn ask_switch_steam(&mut self) {
        self.dialog = Some(Dialog { title: "Use a different Steam account?".into(),
            message: "Your game files stay. The next download asks you to scan a QR code.".into(),
            buttons: vec![("Continue".into(), Act::SwitchSteam), ("Cancel".into(), Act::Close)] });
    }

    pub fn ask_uninstall(&mut self) {
        let n = app_name();
        let owns = self.config.as_ref().and_then(|c| c.owns_game_files) == Some(true);
        let mut buttons = vec![("Uninstall, Keep Game Files".into(), Act::Uninstall(false))];
        if owns { buttons.push(("Uninstall and Delete Game Files".into(), Act::Uninstall(true))); }
        buttons.push(("Cancel".into(), Act::Close));
        let closes = if cfg!(target_os = "macos") { " Any running game closes, and macOS asks for your password to remove the network setting." } else { " Any running game closes." };
        self.dialog = Some(Dialog { title: format!("Uninstall {n}?"), buttons, message: if owns {
            format!("The game files {n} downloaded can stay (to set it up again later without downloading) or go.{closes}")
        } else {
            format!("Your game files folder stays as it is.{closes}")
        } });
    }

    // MARK: Settings' tools

    pub fn measure(&mut self) {
        let Some(folder) = self.game_files() else { return };
        self.settings.free_bytes = if folder.exists() { paths::free_bytes(&folder) } else { 0.0 };
        if !self.logging { return; } // (not in snapshots)
        let p = proc::python(&self.waker, "tools.py", &["crash-reports"]);
        self.keep(Task::CrashReports, p);
    }

    /// Runs a tools.py command; what happens next is up to its last line.
    fn tool(&mut self, title: &str, args: &[&str], what: Tool) {
        self.settings.busy = Some(title.into());
        self.settings.result = None;
        self.settings.last.clear();
        match proc::python(&self.waker, "tools.py", args) {
            Some(p) => self.settings.running = Some((what, p)),
            None => { self.settings.busy = None; self.settings.say(format!("That couldn't start. Open {} again.", app_name()), false); }
        }
    }

    pub fn settings_cancel(&mut self) {
        if let Some((_, p)) = &self.settings.running { p.stop(); }
        self.settings.repair_qr = None;
        self.settings.repair_progress = None;
        self.settings.say("Stopped.", false);
    }

    /// A crashing game writes a ~3 GB memory dump into the game files folder (as MCC does); nothing needs it later.
    pub fn clean_crash_reports(&mut self) { self.tool("Deleting crash reports…", &["clean-crash-reports"], Tool::CleanCrash); }

    /// Repair game files: check every installed game's files against Steam (QR code if the saved sign-in expired).
    pub fn repair(&mut self) {
        self.settings.collector = QrCollector::default();
        self.settings.estimator = RateEstimator::default();
        self.settings.repair_progress = None;
        self.tool("Checking your game files against Steam…", &["repair"], Tool::Repair);
    }

    /// Repair the app: install.sh again with every step redone (Wine, DXVK, scripts, Play buttons); prefixes, saves
    /// and game files stay.
    pub fn repair_setup(&mut self) {
        self.settings.busy = Some(format!("Repairing {}…", app_name()));
        self.settings.result = None;
        self.settings.repair_error = None;
        match proc::installer(&self.waker, &[("DROPPOD_REPAIR", "1".into())]) {
            Some(p) => self.settings.running = Some((Tool::RepairSetup, p)),
            None => { self.settings.busy = None; self.settings.say("The installer couldn't start.", false); }
        }
    }

    /// The game files moved (another drive, another folder): point the app at them.
    pub fn choose_game_files(&mut self) {
        let Some(url) = rfd::FileDialog::new()
            .set_title("Choose the folder your game files are in now (the one with Data and the game folders in it).")
            .pick_folder() else { return };
        let name = url.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.tool("Looking at that folder…", &["set-game-files", &url.to_string_lossy()], Tool::GameFiles(name));
    }

    pub fn force_quit(&mut self) { self.tool("Stopping the game…", &["force-quit"], Tool::ForceQuit); }

    pub fn make_report(&mut self) { self.tool("Making the report…", &["report"], Tool::Report); }

    fn settings_update(&mut self) {
        let Some((what, mut p)) = self.settings.running.take() else { return };
        let mut exit = None;
        for m in p.poll() {
            match m {
                Msg::Line(line) => {
                    if what == Tool::RepairSetup {
                        match SetupEvent::parse(&line) {
                            SetupEvent::Step(h) => if let Some(t) = friendly_step(&h) { self.settings.busy = Some(t + "…") },
                            SetupEvent::Error(e) => if self.settings.repair_error.is_none() { self.settings.repair_error = Some(e) },
                            _ => {}
                        }
                        continue;
                    }
                    if what == Tool::Repair {
                        let s = &mut self.settings;
                        match SetupEvent::parse(&line) {
                            SetupEvent::Steam(e) => { s.collector.handle(&e); if e == SteamEvent::QrEnd { s.repair_qr = s.collector.current.clone(); } }
                            SetupEvent::Progress(v) => { s.repair_qr = None; s.repair_progress = Some(v); s.repair_remaining = s.estimator.remaining(v, now()); }
                            SetupEvent::Status(st) => { s.repair_qr = None; s.busy = Some(st); }
                            _ => {}
                        }
                    }
                    self.settings.last = line;
                }
                Msg::Exit(code) => exit = Some(code),
            }
        }
        let Some(code) = exit else { self.settings.running = Some((what, p)); return };
        let stopped = self.settings.result.as_ref().is_some_and(|(t, _)| t == "Stopped.");
        self.settings.busy = None;
        let last = std::mem::take(&mut self.settings.last);
        let ok = succeeded(&last);
        let say = |s: &mut Settings, good: String, bad: &str| {
            if ok { s.say(good, true) } else { s.say(error_in(&last).unwrap_or_else(|| bad.into()), false) }
        };
        match what {
            Tool::CleanCrash => {
                let freed: f64 = last.get(3..).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                if ok { self.settings.crash_bytes = 0.0; self.measure(); }
                say(&mut self.settings, format!("Deleted the crash reports: {} freed.", gigabytes(freed)), "The crash reports couldn't be deleted.");
            }
            Tool::Repair => {
                let s = &mut self.settings;
                s.repair_qr = None; s.repair_progress = None; s.repair_remaining = None;
                self.reload();
                self.settings.crash_bytes = 0.0;
                if ok { self.settings.say("Your game files are all there and match Steam.", true); }
                else if !stopped { self.settings.say(error_in(&last).unwrap_or_else(|| "Repair didn't finish. Try it again.".into()), false); }
            }
            Tool::RepairSetup => {
                self.reload();
                if code == 0 { self.settings.say(format!("{} was repaired.", app_name()), true); }
                else { let e = self.settings.repair_error.take().unwrap_or_else(|| "Repair didn't finish. Try it again.".into()); self.settings.say(e, false); }
            }
            Tool::GameFiles(name) => {
                self.reload();
                self.measure();
                say(&mut self.settings, format!("{} now uses the game files in {name}.", app_name()), "That folder couldn't be used.");
            }
            Tool::ForceQuit => {
                if ok { self.game_force_quit(); self.settings.say("The game was stopped.", true); }
                else { self.settings.say(error_in(&last).unwrap_or_else(|| format!("The game couldn't be stopped. Restart your {COMPUTER} to close it.")), false); }
            }
            Tool::ResetGraphics => say(&mut self.settings, "Done. The settings apply the next time you play.".into(), "The graphics settings couldn't be changed."),
            Tool::ForgetSteam => {
                if ok { self.settings.say("Done. The next download shows a QR code for the account you want.", true); }
                else { self.settings.say("The Steam account couldn't be changed.", false); }
            }
            Tool::Report => {
                if ok && last.len() > 3 {
                    let path = PathBuf::from(&last[3..]);
                    proc::open_in_finder(&path, true);
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.settings.say(format!("The report is on your Desktop: {name}"), true);
                } else { self.settings.say(error_in(&last).unwrap_or_else(|| "The report couldn't be made.".into()), false); }
            }
            Tool::Uninstall => {
                if !ok { return self.settings.say(error_in(&last).unwrap_or_else(|| "Uninstalling didn't finish. Try it again.".into()), false); }
                self.dialog = Some(Dialog {
                    title: format!("{} was removed.", app_name()),
                    message: if last == "ok network-kept" {
                        "Its network setting stayed because the password box was cancelled. It's harmless and goes away if you reinstall and uninstall again.".into()
                    } else { "Thanks for playing!".into() },
                    buttons: vec![("OK".into(), Act::Quit)],
                });
            }
        }
    }

    // MARK: native Halo CE

    pub fn native_maps_there(&self) -> bool { self.native_maps_there }
    pub fn native_can_start(&self) -> bool {
        let n = &self.native;
        (n.image.is_some() && n.image_problem.is_none() && !n.checking) || self.native_maps_there
    }

    pub fn native_reset(&mut self) {
        self.native_stop();
        let n = &mut self.native;
        n.page = NativePage::Explain;
        n.step = 0; n.progress = None; n.status = None;
        if n.image.is_none() { n.image_summary = None; n.image_problem = None; }
    }

    /// 1-2. the player's disc image, picked and checked (an Xbox disc image with Halo's maps folder)
    pub fn native_choose_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose the disc image (.iso or .xiso) made from your own Halo: Combat Evolved Xbox disc.")
            .add_filter("Xbox disc image", &["iso", "xiso"]).pick_file() else { return };
        self.native_check(&path);
    }

    pub fn native_check(&mut self, path: &Path) {
        let n = &mut self.native;
        n.image = Some(path.to_path_buf());
        n.image_summary = None;
        n.image_problem = None;
        n.checking = true;
        n.last.clear();
        let args: Vec<String> = ["-I", "-B"].iter().map(|s| s.to_string())
            .chain([paths::scripts_dir().join("xiso.py").to_string_lossy().into_owned(), "check".into(), path.to_string_lossy().into_owned()]).collect();
        n.running = Proc::spawn(&self.waker, proc::PYTHON, &args, &[]).map(|p| (NativeTool::Check, p));
        if n.running.is_none() { n.checking = false; n.image_problem = Some("That file couldn't be read.".into()); }
    }

    /// 3-4. the port, the maps, the check
    pub fn native_start(&mut self) {
        let n = &mut self.native;
        n.page = NativePage::Working;
        n.step = 1; // (the disc image was checked when it was chosen)
        n.progress = None;
        n.status = None;
        self.native_tool("mods.py", &["get", &disc_mod().id], NativeTool::Get);
    }

    fn native_tool(&mut self, script: &str, args: &[&str], what: NativeTool) {
        self.native.last.clear();
        self.native.running = proc::python(&self.waker, script, args).map(|p| (what, p));
        if self.native.running.is_none() { self.native_fail(format!("That couldn't start. Use Settings > Repair {}.", app_name())); }
    }

    fn native_fail(&mut self, message: String) {
        self.native.page = NativePage::Failed(message);
        self.reload();
    }

    fn native_finish(&mut self) {
        self.native.step = 3;
        self.native.progress = None;
        self.native.status = None;
        self.native_tool("tools.py", &["native-ce-status"], NativeTool::Status);
    }

    pub fn native_stop(&mut self) {
        if let Some((_, p)) = self.native.running.take() { p.stop(); }
        self.native.checking = false;
    }

    fn native_update(&mut self) {
        let Some((what, mut p)) = self.native.running.take() else { return };
        let mut exited = false;
        for m in p.poll() {
            match m {
                Msg::Line(line) => {
                    if what == NativeTool::Check { self.native.last = line; continue; }
                    match UpdaterEvent::parse(&line) {
                        UpdaterEvent::Progress(v) => self.native.progress = Some(v),
                        UpdaterEvent::Status(s) => self.native.status = Some(s),
                        _ => self.native.last = line,
                    }
                }
                Msg::Exit(_) => exited = true,
            }
        }
        if !exited { self.native.running = Some((what, p)); return; }
        let last = std::mem::take(&mut self.native.last);
        if what == NativeTool::Check {
            self.native.checking = false;
            match disc_check(&last) {
                Ok(summary) => self.native.image_summary = Some(summary),
                Err(problem) => self.native.image_problem = Some(problem),
            }
            return;
        }
        let expect = if what == NativeTool::Status { Some("ready") } else { None };
        let passed = match expect { Some(e) => last == e, None => succeeded(&last) };
        if passed {
            match what {
                NativeTool::Get => {
                    self.native.step = 2;
                    if self.native_maps_there { return self.native_finish(); }
                    let Some(image) = self.native.image.clone() else { return self.native_fail("Choose your disc image first.".into()) };
                    self.native_tool("tools.py", &["native-ce-import", &image.to_string_lossy()], NativeTool::Import);
                }
                NativeTool::Import => self.native_finish(),
                NativeTool::Status => { self.reload(); self.native.page = NativePage::Done; }
                NativeTool::Check => {}
            }
            return;
        }
        if last == "needs-maps" { return self.native_fail("Halo's maps aren't in place yet. Choose your disc image again.".into()); }
        self.native_fail(error_in(&last).unwrap_or_else(|| "That step didn't finish. Try again.".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_answers() {
        assert_eq!(disc_check("ok maps 25 1932735283"), Ok("An Xbox disc image of Halo: Combat Evolved: its maps folder has 25 files (1.8 GB).".into()));
        assert_eq!(disc_check("error: This isn't an Xbox disc image."), Err("This isn't an Xbox disc image.".into()));
        assert_eq!(disc_check(""), Err("That file couldn't be read.".into()));
        assert!(succeeded("ok") && succeeded("ok 123") && !succeeded("okay") && !succeeded(""));
        assert_eq!(error_in("error: no"), Some("no".into()));
    }

    #[test]
    fn an_install_run_reads_install_sh() {
        let mut r = InstallRun::new("halo3");
        r.handle("\u{1B}[1m==> Starting setup: Drop Pod 0.1.0\u{1B}[0m");
        r.handle("==> Game files (from your Steam account)");
        for l in ["qr-begin", "qr: ██", "qr-end"] { r.handle(l); }
        assert_eq!(r.steps, ["Getting your game files"]);
        assert!(r.qr.is_some(), "the QR code shows");
        r.handle("progress: 37");
        assert!(r.qr.is_none() && r.progress == Some(0.37), "signed in: the code goes, the progress comes");
        r.handle("error: The disk is full.");
        r.handle("error: and something after");
        assert_eq!(r.first_error.as_deref(), Some("The disk is full."), "the first error is the specific one");
    }
}
