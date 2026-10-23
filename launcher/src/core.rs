// SPDX-License-Identifier: GPL-3.0-or-later AND MIT
// Everything the window needs that isn't drawing: the mod catalogue (games, mods, Steam depots), which depots a game
// files folder already has, config.json, the helper scripts' output lines, and the QR code DepotDownloader draws.
// Ported from LauncherCore.swift, which was forked from the Project Reclaimer Mac port (portions MIT, see
// LICENSE.reclaimer-mac-port). Tested below (cargo test).
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The app's name, as make_app.sh puts it in Info.plist from the APP_NAME file (the one place to change it).
pub fn app_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        crate::paths::info_plist_string("CFBundleName").filter(|s| !s.is_empty()).unwrap_or_else(|| "Drop Pod".into())
    })
}

// MARK: the mod catalogue: games, mods and Steam depots

/// One Steam depot of Halo: The Master Chief Collection (app 976730), English only. Sizes are as Steam shows them
/// (GiB). A depot counts as present in a folder when all its marker files are there.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Depot {
    pub id: i64,
    pub name: String,
    pub gib: f64,
    pub markers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Source {
    pub github: String,
    pub asset: String,
    pub sums: Option<String>, // (no sums: GitHub's own SHA-256)
}

/// A community mod a game's Play button can run ("Play with"). Downloaded from its own GitHub releases (mods.py get),
/// then it updates itself; install.sh adds what it needs; play.sh starts it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    pub id: String,     // used in config.json (playWith), mods/<id>/ and prefixes/<id>/
    pub name: String,
    pub licence: String,
    pub about: String,   // what it is, for its guided setup
    pub caption: String, // on its game's tile, after its name
    pub source: Source,
    pub mb: f64,          // its download, about
    pub depots: Vec<i64>, // the MCC depots it needs (none: it doesn't play from MCC's files)
    pub mcc_build: Option<String>, // the MCC build it needs ("current": whatever Steam has; it follows MCC's updates)
    pub runs: String,    // "wine": under Wine, in its own prefix; "mac": a Mac app of its own
    pub program: String, // its program's file name in mods/<id>/ (a regular expression)
    pub process: String, // the running game as ps shows it (a regular expression)
    pub setup: String,   // "with-game": set up with its game (install.sh); "xbox-disc": from the player's disc
    pub extras: Option<Vec<String>>, // what install.sh adds for it: mesa, spinfix, loopback, presets
    pub not_under_wine: Option<String>, // it stops itself under Wine (Reach Renaissance): why, for the player
}

impl Mod {
    pub fn on_wine(&self) -> bool { self.runs == "wine" }
    /// Every Windows mod runs under Wine on the Mac, so one that stops itself there doesn't run here.
    // (Windows runs it natively, so a mod that stops itself under Wine is fine there)
    pub fn runs_here(&self) -> bool { cfg!(target_os = "windows") || !(self.on_wine() && self.not_under_wine.is_some()) }
    pub fn has(&self, extra: &str) -> bool { self.extras.as_ref().is_some_and(|e| e.iter().any(|x| x == extra)) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    pub id: String, // used in config.json and by install.sh
    pub title: String,
    pub short: String,
    pub art: String,         // MCC's own tile picture in the base depot: Data/UI/BackgroundVideos/Images/<art>
    pub options: Vec<Mod>,   // what it can play with, the default first
    pub depots: Vec<Depot>,  // what its mods need, in the catalogue's order (none: the native port reads the disc)
    pub addons: Vec<AddOn>,  // optional extra game files, ticked with it (Halo 2's multiplayer maps)
    /// None of its mods runs here (Reach: Reach Renaissance stops itself under Wine): why, for the player. It's
    /// still listed, greyed out, and never set up, downloaded or played.
    pub unavailable: Option<String>,
}

impl Game {
    pub fn available(&self) -> bool { self.unavailable.is_none() }
    pub fn gib(&self) -> f64 { self.depots.iter().map(|d| d.gib).sum() }
}

/// Optional game files that come with a game only when the player ticks them (in setup, or later in Settings).
#[derive(Debug, Clone, PartialEq)]
pub struct AddOn {
    pub id: String, // in config.json's wanted list and DROPPOD_GAMES, after its game's
    pub game: String,
    pub name: String,
    pub about: String,
    pub depots: Vec<Depot>,
}

impl AddOn {
    pub fn gib(&self) -> f64 { self.depots.iter().map(|d| d.gib).sum() }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    title: String,
    short: String,
    art: String,
    mods: Vec<String>,
    addons: Option<Vec<AddOnEntry>>,
}

#[derive(Deserialize)]
struct AddOnEntry {
    id: String,
    name: String,
    about: String,
    depots: Vec<i64>,
}

/// scripts/mods.json, the one list of games and mods (the scripts read the same file): inside the app at
/// Contents/Resources/install/scripts/mods.json; DROPPOD_CATALOGUE overrides it (tests).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogueFile {
    steam_app: i64,
    base_depot: i64,
    depots: Vec<Depot>,
    games: Vec<Entry>,
    mods: Vec<Mod>,
}

pub struct Catalogue {
    #[allow(dead_code)] // (the tests check it)
    pub steam_app: i64,
    pub base_depot: Depot,
    pub depots: Vec<Depot>,
    pub mods: Vec<Mod>,
    /// Every game in the catalogue; one whose mods don't run here (Reach: its mod stops itself under Wine) is
    /// listed as unavailable.
    pub games: Vec<Game>,
}

impl Catalogue {
    pub fn path() -> PathBuf {
        if let Some(p) = crate::paths::var("DROPPOD_CATALOGUE") { return p.into(); }
        let inside = crate::paths::resources().join("install/scripts/mods.json");
        // (run from a build folder: the repository's)
        if inside.exists() { inside } else { crate::paths::repo().join("scripts/mods.json") }
    }

    pub fn load(path: &Path) -> Catalogue {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("The mod catalogue {} couldn't be read: {e}", path.display()));
        let f: CatalogueFile = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("The mod catalogue {} couldn't be read: {e}", path.display()));
        let base = f.depots.iter().find(|d| d.id == f.base_depot).expect("the base depot is listed").clone();
        let games = f.games.iter().map(|g| {
            let all: Vec<Mod> = g.mods.iter().filter_map(|id| f.mods.iter().find(|m| &m.id == id).cloned()).collect();
            let here: Vec<Mod> = all.iter().filter(|m| m.runs_here()).cloned().collect();
            // (with none that runs here, its mods are still named, for the greyed-out tile)
            let unavailable = here.is_empty().then(|| all.iter().find_map(|m| m.not_under_wine.clone())
                .unwrap_or_else(|| "Its mod doesn't run on this computer yet.".into()));
            let options = if here.is_empty() { all } else { here };
            Game {
                id: g.id.clone(), title: g.title.clone(), short: g.short.clone(), art: g.art.clone(),
                depots: f.depots.iter().filter(|d| options.iter().any(|m| m.depots.contains(&d.id))).cloned().collect(),
                addons: g.addons.iter().flatten().map(|a| AddOn {
                    id: a.id.clone(), game: g.id.clone(), name: a.name.clone(), about: a.about.clone(),
                    depots: f.depots.iter().filter(|d| a.depots.contains(&d.id)).cloned().collect(),
                }).collect(),
                options,
                unavailable,
            }
        }).collect();
        Catalogue { steam_app: f.steam_app, base_depot: base, depots: f.depots, mods: f.mods, games }
    }
}

pub fn catalogue() -> &'static Catalogue {
    static CAT: OnceLock<Catalogue> = OnceLock::new();
    CAT.get_or_init(|| Catalogue::load(&Catalogue::path()))
}
pub fn games() -> &'static [Game] { &catalogue().games }
pub fn mods() -> &'static [Mod] { &catalogue().mods }
pub fn base_depot() -> &'static Depot { &catalogue().base_depot }
pub fn game(id: &str) -> Option<&'static Game> { games().iter().find(|g| g.id == id) }
pub fn add_on(id: &str) -> Option<&'static AddOn> { games().iter().flat_map(|g| &g.addons).find(|a| a.id == id) }
pub fn mod_(id: &str) -> Option<&'static Mod> { mods().iter().find(|m| m.id == id) }

/// What a player can choose, in the order install.sh gets it: each game that's available here, then its add-ons.
pub fn choice_order() -> Vec<String> {
    games().iter().filter(|g| g.available()).flat_map(|g| std::iter::once(g.id.clone()).chain(g.addons.iter().map(|a| a.id.clone()))).collect()
}

/// The catalogue's mod that plays from the player's own Xbox disc (ChupathingyCE).
pub fn disc_mod() -> &'static Mod { mods().iter().find(|m| m.setup == "xbox-disc").expect("a disc mod in the catalogue") }

/// Every depot the chosen games (and their chosen add-ons) need, the shared one first (when any is needed), in a
/// fixed order. An add-on counts only with its game; a game that isn't available here never counts.
pub fn depots_for(chosen: &HashSet<String>) -> Vec<Depot> {
    let list: Vec<Depot> = games().iter().filter(|g| g.available() && chosen.contains(&g.id))
        .flat_map(|g| g.depots.iter().chain(g.addons.iter().filter(|a| chosen.contains(&a.id)).flat_map(|a| &a.depots)))
        .cloned().collect();
    if list.is_empty() { list } else { std::iter::once(base_depot().clone()).chain(list).collect() }
}

/// Depots whose marker files are all in `folder` (an MCC folder: the one with MCC/, Data/, halo1/ … in it).
pub fn present_depots(folder: &Path) -> HashSet<i64> {
    std::iter::once(base_depot())
        .chain(games().iter().flat_map(|g| g.depots.iter().chain(g.addons.iter().flat_map(|a| &a.depots))))
        .filter(|d| d.markers.iter().all(|m| folder.join(m).exists()))
        .map(|d| d.id).collect()
}

/// Games that play from MCC's files and whose depots are all in the folder.
pub fn installed_games(present: &HashSet<i64>) -> Vec<String> {
    if !present.contains(&base_depot().id) { return vec![]; }
    games().iter().filter(|g| g.available() && !g.depots.is_empty() && g.depots.iter().all(|d| present.contains(&d.id)))
        .map(|g| g.id.clone()).collect()
}

/// What setup (or a tile's Get) will download: the chosen games' depots that aren't in the folder yet.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadPlan {
    pub needed: Vec<Depot>,
}

impl DownloadPlan {
    pub fn new(chosen: &HashSet<String>, present: &HashSet<i64>) -> Self {
        DownloadPlan { needed: depots_for(chosen).into_iter().filter(|d| !present.contains(&d.id)).collect() }
    }
    pub fn gib(&self) -> f64 { self.needed.iter().map(|d| d.gib).sum() }
    pub fn bytes(&self) -> f64 { self.gib() * 1_073_741_824.0 }
    pub fn is_empty(&self) -> bool { self.needed.is_empty() }
}

/// Does this look like an MCC folder? (A copy from Steam on a PC, or one this app downloaded.)
pub fn looks_like_mcc(folder: &Path) -> bool {
    ["MCC", "mcclauncher.exe", "Data/UI"].iter().any(|p| folder.join(p).exists())
}

/// Places an MCC folder is often found on a Mac: the Project Reclaimer Mac port's copy (often a link to an external
/// drive), a Games folder, and Steam libraries on external drives.
pub fn existing_mcc_candidates(home: &Path, volumes: &Path) -> Vec<PathBuf> {
    let mcc = "Halo The Master Chief Collection";
    let mut list = vec![
        home.join(format!("Games/ProjectReclaimer/prefix-wine/drive_c/Program Files (x86)/Steam/steamapps/common/{mcc}")),
        home.join("Games/Halo MCC"),
        home.join(format!("Games/{mcc}")),
    ];
    let mut vols: Vec<PathBuf> = std::fs::read_dir(volumes).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    vols.sort();
    for v in vols {
        list.push(v.join("Games/Halo MCC"));
        list.push(v.join(format!("Games/{mcc}")));
        list.push(v.join(format!("SteamLibrary/steamapps/common/{mcc}")));
    }
    let mut seen = HashSet::new();
    list.into_iter().filter_map(|p| std::fs::canonicalize(&p).ok())
        .filter(|p| looks_like_mcc(p) && seen.insert(p.clone())).collect()
}

/// The usual places for this user.
pub fn existing_mcc() -> Vec<PathBuf> {
    existing_mcc_candidates(&crate::paths::user_home(), Path::new("/Volumes"))
}

/// Text as a pattern for pgrep's extended regular expressions that matches just that text (a folder's path, say).
pub fn extended_regex_escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\^$.|?*+()[]{}".contains(c) { out.push('\\'); }
        out.push(c);
    }
    out
}

/// A Mac path as Wine sees it (its Z: drive is /).
#[allow(dead_code)] // (the scripts write it; the tests check the form)
pub fn windows_path(path: &str) -> String { format!("Z:{}", path.replace('/', "\\")) }

/// Bytes the way Steam (and the game sizes here, and `df -g`) count them: GiB, called GB.
pub fn gigabytes(bytes: f64) -> String {
    if bytes >= 1_073_741_824.0 { format!("{:.1} GB", bytes / 1_073_741_824.0) } else { format!("{:.0} MB", bytes / 1_048_576.0) }
}

/// Sizes the way Steam shows them (and the way the plan lists them): GiB, called GB.
pub fn steam_size(gib: f64) -> String { format!("{gib:.1} GB") }

// MARK: config.json

/// $DROPPOD_HOME/config.json, written by install.sh (and tools.py when the game files move).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "one")]
    pub version: i64,
    pub game_files: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_files_windows: Option<String>,
    #[serde(default = "steam")]
    pub source: String, // "steam" (downloaded here) or "existing" (a folder the player had)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owns_game_files: Option<bool>, // true: this app created the folder, so uninstall may offer to delete it
    #[serde(default)]
    pub games: Vec<String>, // installed: every depot there
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wanted: Option<Vec<String>>, // what the player chose
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_with: Option<BTreeMap<String, String>>, // each game's "Play with" choice (a Mod.id); missing: the default
}

fn one() -> i64 { 1 }
fn steam() -> String { "steam".into() }

impl AppConfig {
    pub fn new(game_files: &str) -> Self {
        AppConfig { version: 1, game_files: game_files.into(), game_files_windows: None, source: "steam".into(),
                    owns_game_files: None, games: vec![], wanted: None, play_with: None }
    }

    /// The choice for a game, or its default (the first) when none or an unknown one is saved.
    pub fn engine(&self, id: &str) -> String {
        let options = game(id).map(|g| g.options.as_slice()).unwrap_or(&[]);
        let saved = self.play_with.as_ref().and_then(|p| p.get(id));
        options.iter().find(|m| Some(&m.id) == saved).or(options.first()).map(|m| m.id.clone()).unwrap_or_default()
    }

    pub fn load(path: &Path) -> Option<AppConfig> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }
}

// MARK: helper script output

/// One line from `updater.py run`.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdaterEvent { Status(String), Progress(f64), Relaunch, Done(String), Offline(String), Error(String), Other }

fn rest<'a>(line: &'a str, prefix: &str) -> Option<&'a str> { line.strip_prefix(prefix) }

/// "progress: 37" as 0.37, clamped to 0…1.
fn fraction(s: &str) -> Option<f64> { s.parse::<f64>().ok().map(|v| v.clamp(0.0, 100.0) / 100.0) }

impl UpdaterEvent {
    pub fn parse(line: &str) -> Self {
        if line == "relaunch" { return Self::Relaunch; }
        if let Some(s) = rest(line, "status: ") { return Self::Status(s.into()); }
        if let Some(v) = rest(line, "progress: ").and_then(fraction) { return Self::Progress(v); }
        if let Some(s) = rest(line, "done: ") { return Self::Done(s.into()); }
        if let Some(s) = rest(line, "offline: ") { return Self::Offline(s.into()); }
        if let Some(s) = rest(line, "error: ") { return Self::Error(s.into()); }
        Self::Other
    }
}

/// One line from `steam.py` about signing in.
#[derive(Debug, Clone, PartialEq)]
pub enum SteamEvent { QrBegin, QrRow(String), QrEnd, Ok(String), NeedLogin, Offline, Error(String), Other }

impl SteamEvent {
    pub fn parse(line: &str) -> Self {
        match line {
            "qr-begin" => Self::QrBegin,
            "qr-end" => Self::QrEnd,
            "need-login" => Self::NeedLogin,
            "offline" => Self::Offline,
            _ => {
                if let Some(r) = rest(line, "qr: ") { Self::QrRow(r.into()) }
                else if let Some(a) = rest(line, "ok ") { Self::Ok(a.into()) }
                else if let Some(e) = rest(line, "error: ") { Self::Error(e.into()) }
                else { Self::Other }
            }
        }
    }
}

/// A QR code as DepotDownloader prints it (QRCoder line-by-line ASCII): "██" per dark module, two spaces per light.
#[derive(Debug, Clone, PartialEq)]
pub struct QrGrid {
    pub modules: Vec<Vec<bool>>, // rows of columns, true = dark
}

impl QrGrid {
    pub fn new(rows: &[String]) -> Option<QrGrid> {
        let parsed: Vec<Vec<bool>> = rows.iter().map(|row| {
            let chars: Vec<char> = row.chars().collect();
            chars.chunks_exact(2).map(|p| p[0] == '█').collect()
        }).collect();
        let width = parsed.iter().map(|r| r.len()).max().unwrap_or(0);
        if width == 0 || !parsed.iter().any(|r| r.contains(&true)) { return None; }
        // short rows (trailing spaces trimmed somewhere) are light to the right edge
        Some(QrGrid { modules: parsed.into_iter().map(|mut r| { r.resize(width, false); r }).collect() })
    }
    pub fn size(&self) -> usize { self.modules.len() }
}

/// Collects QR rows between qr-begin and qr-end; Steam refreshes the code every ~30 s, so later codes replace it.
#[derive(Default)]
pub struct QrCollector {
    rows: Vec<String>,
    pub current: Option<QrGrid>,
}

impl QrCollector {
    pub fn handle(&mut self, event: &SteamEvent) {
        match event {
            SteamEvent::QrBegin => self.rows.clear(),
            SteamEvent::QrRow(r) => self.rows.push(r.clone()),
            SteamEvent::QrEnd => if let Some(g) = QrGrid::new(&self.rows) { self.current = Some(g) },
            _ => {}
        }
    }
}

// MARK: setup

/// What the window opens as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaunchMode {
    Setup,   // nothing installed yet, or a first install that stopped partway: the setup wizard
    HandOff, // the disk image's copy opened while an install exists: open the installed app instead
    Library, // the games
}

pub fn launch_mode(home: &Path, is_installed_app: bool, installed_app_exists: bool) -> LaunchMode {
    // (version: install.sh's last step writes it; Wine is there only when a chosen mod runs on it)
    if !home.join("config.json").exists() || !home.join("version").exists() || home.join(".setup-incomplete").exists() {
        return LaunchMode::Setup;
    }
    if !is_installed_app && installed_app_exists { LaunchMode::HandOff } else { LaunchMode::Library }
}

/// One line of install.sh output in GUI mode.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupEvent { Step(String), Progress(f64), Status(String), Error(String), Steam(SteamEvent), Other }

/// The line without terminal colour codes (ESC [ digits/semicolons m).
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1B}' && chars.peek() == Some(&'[') {
            let saved = chars.clone();
            chars.next();
            while chars.peek().is_some_and(|c| c.is_ascii_digit() || *c == ';') { chars.next(); }
            if chars.peek() == Some(&'m') { chars.next(); continue; }
            chars = saved;
        }
        out.push(c);
    }
    out
}

impl SetupEvent {
    pub fn parse(line: &str) -> Self {
        let p = plain(line);
        if let Some(s) = p.strip_prefix("==> ") { return Self::Step(s.into()); }
        if let Some(v) = p.strip_prefix("progress: ").and_then(fraction) { return Self::Progress(v); }
        if let Some(s) = p.strip_prefix("error: ") { return Self::Error(s.into()); }
        if let Some(s) = p.strip_prefix("status: ") { return Self::Status(s.into()); }
        if p == "qr-begin" || p == "qr-end" || p.starts_with("qr: ") { return Self::Steam(SteamEvent::parse(&p)); }
        Self::Other
    }
}

/// install.sh's step headings (by prefix), said plainly (None: a step not worth showing on its own). In a title, {}
/// is the mod the heading names (the rest of it, up to " ("), and {game} the game that mod plays.
pub fn step_titles() -> &'static [(&'static str, Option<String>)] {
    static TITLES: OnceLock<Vec<(&'static str, Option<String>)>> = OnceLock::new();
    TITLES.get_or_init(|| {
        let n = app_name();
        let t = |s: &str| Some(s.to_string());
        vec![
            ("Starting setup", None),
            ("Mac settings", t("Mac settings (asks for your Mac password once)")),
            ("Wine", t("Getting the Windows compatibility layer")),
            ("Graphics translation", t("Getting the graphics translation layer")),
            ("DepotDownloader", t("Getting the Steam downloader")),
            ("Helper scripts", t("Setting up the helper scripts")),
            ("Windows environment for ", t("Setting up {}'s own space")),
            ("Menu library (Mesa) for ", t("Getting {}'s menu graphics library")),
            ("Mod: ", t("Downloading {} (plays {game})")),
            ("Tuned graphics settings for ", t("Choosing {}'s graphics settings for your Mac")),
            ("Game launchers", t("Setting up the Play buttons")),
            ("Game app", t("Making each mod show as itself in the menu bar and Dock")),
            ("Game files", t("Getting your game files")),
            ("Settings file", t("Saving your choices")),
            ("Adding", Some(format!("Adding {n} to Applications"))),
            ("Done", None),
            ("Updated to", None),
        ]
    })
}

pub fn friendly_step(heading: &str) -> Option<String> {
    let Some((prefix, title)) = step_titles().iter().find(|(p, _)| heading.starts_with(p)) else {
        return Some(heading.into());
    };
    let title = title.as_ref()?;
    let rest = &heading[prefix.len()..];
    let name = rest.find(" (").map_or(rest, |i| &rest[..i]);
    let plays = games().iter().find(|g| g.options.iter().any(|m| m.name == name)).map_or("a game", |g| g.short.as_str());
    Some(title.replace("{game}", plays).replace("{}", name))
}

/// "About 12 minutes left" from how fast a download has been going. Times in seconds.
#[derive(Default)]
pub struct RateEstimator {
    start: Option<(f64, f64)>,
}

impl RateEstimator {
    pub fn remaining(&mut self, fraction: f64, now: f64) -> Option<String> {
        let Some((t0, f0)) = self.start else {
            self.start = Some((now, fraction));
            return None;
        };
        let (done, elapsed) = (fraction - f0, now - t0);
        if done <= 0.002 || elapsed <= 5.0 { return None; }
        let left = (1.0 - fraction) * elapsed / done;
        Some(if left < 90.0 { "About a minute left".into() }
             else if left < 3600.0 { format!("About {} minutes left", (left / 60.0).round() as i64) }
             else {
                 let h = left / 3600.0;
                 if h < 1.5 { "About an hour left".into() } else { format!("About {} hours left", h.round() as i64) }
             })
    }
}

/// Seconds on a clock that only goes forward (for RateEstimator and timeouts).
pub fn now() -> f64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf() }
    fn set(ids: &[&str]) -> HashSet<String> { ids.iter().map(|s| s.to_string()).collect() }
    fn ids(depots: &[Depot]) -> Vec<i64> { depots.iter().map(|d| d.id).collect() }

    #[test]
    fn updater_lines() {
        assert_eq!(UpdaterEvent::parse("status: Checking for updates…"), UpdaterEvent::Status("Checking for updates…".into()));
        assert_eq!(UpdaterEvent::parse("progress: 37"), UpdaterEvent::Progress(0.37));
        assert_eq!(UpdaterEvent::parse("progress: 140"), UpdaterEvent::Progress(1.0), "progress clamps");
        assert_eq!(UpdaterEvent::parse("relaunch"), UpdaterEvent::Relaunch);
        assert_eq!(UpdaterEvent::parse("done: up to date"), UpdaterEvent::Done("up to date".into()));
        assert_eq!(UpdaterEvent::parse("offline: timed out"), UpdaterEvent::Offline("timed out".into()));
        assert_eq!(UpdaterEvent::parse("error: checksum"), UpdaterEvent::Error("checksum".into()));
        assert_eq!(UpdaterEvent::parse("    ok"), UpdaterEvent::Other);
    }

    #[test]
    fn steam_lines() {
        assert_eq!(SteamEvent::parse("ok RealName_1"), SteamEvent::Ok("RealName_1".into()));
        assert_eq!(SteamEvent::parse("need-login"), SteamEvent::NeedLogin);
        assert_eq!(SteamEvent::parse("error: Expired"), SteamEvent::Error("Expired".into()));
        assert_eq!(SteamEvent::parse("qr:   ██"), SteamEvent::QrRow("  ██".into()), "qr row keeps leading spaces");
    }

    #[test]
    fn qr_codes() {
        // real DepotDownloader output (captured 2026-10-10 from DepotDownloader 3.4.0, 37 rows)
        let captured = fs::read_to_string(repo().join("launcher/tests/qr-sample.txt")).unwrap();
        let mut collector = QrCollector::default();
        for line in captured.split('\n') { collector.handle(&SteamEvent::parse(line)); }
        let g = collector.current.expect("a QR code parsed from the sample");
        assert_eq!(g.size(), 37);
        assert!(g.modules.iter().all(|r| r.len() == 37), "square");
        assert!((4..11).all(|x| g.modules[4][x]), "finder pattern top edge");
        assert_eq!(QrGrid::new(&["    ".into(), "    ".into()]), None, "all-light grid is no QR");
        let mut c2 = QrCollector::default();
        for l in ["qr-begin", "qr: ██", "qr-end", "qr-begin", "qr:   ██"] { c2.handle(&SteamEvent::parse(l)); }
        assert_eq!(c2.current.unwrap().modules, vec![vec![true]], "keeps last complete code until the new one ends");
    }

    #[test]
    fn install_lines() {
        assert_eq!(SetupEvent::parse("\u{1B}[1m==> Wine (Gcenx Wine Staging 11.18) with MoltenVK 1.4.2\u{1B}[0m"),
                   SetupEvent::Step("Wine (Gcenx Wine Staging 11.18) with MoltenVK 1.4.2".into()), "step heading");
        assert_eq!(SetupEvent::parse("progress: 63.1"), SetupEvent::Progress(0.631));
        assert_eq!(SetupEvent::parse("status: Downloading Halo 2 (1 of 3)…"), SetupEvent::Status("Downloading Halo 2 (1 of 3)…".into()));
        assert_eq!(SetupEvent::parse("error: The disk is full."), SetupEvent::Error("The disk is full.".into()));
        assert_eq!(SetupEvent::parse("qr-begin"), SetupEvent::Steam(SteamEvent::QrBegin));
        assert_eq!(SetupEvent::parse("    dry-run: wine wineboot"), SetupEvent::Other);
    }

    #[test]
    fn every_install_heading_has_a_plain_title() {
        let script = fs::read_to_string(repo().join("install.sh")).unwrap();
        let headings: Vec<String> = script.lines().filter_map(|l| {
            let i = l.find("bold \"")?;
            let s: String = l[i + 6..].chars().take_while(|c| *c != '"' && *c != '$').collect();
            (!s.is_empty()).then_some(s)
        }).collect();
        assert!(headings.len() >= 15, "found install.sh's headings: {}", headings.len());
        for h in &headings {
            assert!(step_titles().iter().any(|(p, _)| h.starts_with(p)), "install.sh heading “{h}” has no plain title");
        }
        assert_eq!(friendly_step("Starting setup: Drop Pod 0.1.0"), None, "the first heading is hidden");
        assert_eq!(friendly_step("Done (dry run)"), None, "Done is hidden");
        assert_eq!(friendly_step("Mac settings: Rosetta (Apple's Intel translator) (one password prompt)").as_deref(),
                   Some("Mac settings (asks for your Mac password once)"), "admin step");
        assert_eq!(friendly_step("Mod: Project Reclaimer (latest release, checksum-verified)").as_deref(),
                   Some("Downloading Project Reclaimer (plays Halo 3 and ODST)"), "a mod's download names it and its game");
        assert_eq!(friendly_step("Windows environment for Project Reclaimer").as_deref(),
                   Some("Setting up Project Reclaimer's own space"), "a mod's prefix");
    }

    #[test]
    fn the_catalogue() {
        let gids: Vec<&str> = games().iter().map(|g| g.id.as_str()).collect();
        assert_eq!(gids, ["halo1", "halo2", "halo3", "haloreach"], "every game in the catalogue, in order");
        let opts: Vec<Vec<&str>> = games().iter().map(|g| g.options.iter().map(|m| m.id.as_str()).collect()).collect();
        assert_eq!(opts, [vec!["chupathingyce"], vec!["gravemind"], vec!["reclaimer"], vec!["renaissance"]], "each game's mods");
        let reach = game("haloreach").unwrap();
        assert!(!reach.available() && reach.unavailable.as_deref().is_some_and(|r| r.contains("Wine")),
                "Reach is listed but unavailable: Reach Renaissance stops itself under Wine, and the tile says why");
        assert!(games().iter().filter(|g| g.id != "haloreach").all(|g| g.available()), "the others are available");
        assert!(depots_for(&set(&["haloreach"])).is_empty() && installed_games(&[976731, 1064225, 1064226].into()).is_empty(),
                "an unavailable game is never downloaded or counted as installed");
        assert!(mods().iter().all(|m| !gids.contains(&m.id.as_str())), "mod and game IDs never clash");
        assert!(mod_("renaissance").is_some_and(|m| !m.runs_here()), "Reach Renaissance doesn't run under Wine");
        assert!(base_depot().id == 976731 && catalogue().steam_app == 976730, "MCC's shared depot and Steam app");
        for m in mods() {
            assert!(["wine", "mac"].contains(&m.runs.as_str()) && ["with-game", "xbox-disc"].contains(&m.setup.as_str()), "{}: how it runs", m.id);
            assert!(m.source.github.split('/').count() == 2 && regex::Regex::new(&m.source.asset).is_ok()
                    && m.source.sums.as_deref() != Some(""), "{}: its GitHub releases, asset and checksum file", m.id);
            assert!(regex::Regex::new(&m.program).is_ok() && regex::Regex::new(&m.process).is_ok(), "{}: its patterns", m.id);
            assert!(m.depots.iter().all(|d| catalogue().depots.iter().any(|x| x.id == *d)) && !m.depots.contains(&base_depot().id),
                    "{}: depots from the list (the shared one comes with any)", m.id);
            assert!(m.mb > 0.0 && !m.about.is_empty() && !m.caption.is_empty(), "{}: size and words", m.id);
            assert!(m.extras.iter().flatten().all(|e| ["mesa", "spinfix", "loopback", "presets"].contains(&e.as_str())), "{}: extras", m.id);
            assert!(games().iter().any(|g| g.options.contains(m)), "{}: some game plays with it", m.id);
        }
        assert!(mod_("reclaimer").unwrap().on_wine() && mod_("reclaimer").unwrap().has("loopback"));
        assert!(mod_("chupathingyce").unwrap().depots.is_empty() && mod_("chupathingyce").unwrap().setup == "xbox-disc");
        assert_eq!(ids(&depots_for(&set(&["halo3"]))), [976731, 976738, 976739], "Halo 3: shared, campaign, multiplayer");
        assert_eq!(ids(&depots_for(&set(&["halo2"]))), [976731, 976735], "Halo 2 on Project Gravemind: shared, campaign");
        assert_eq!(ids(&depots_for(&set(&["halo2", "halo2-mp"]))), [976731, 976735, 976736], "Halo 2's multiplayer maps, when ticked");
        assert_eq!(ids(&depots_for(&set(&["halo2-mp", "halo3"]))), [976731, 976738, 976739], "an add-on never comes without its game");
        assert_eq!(choice_order(), ["halo1", "halo2", "halo2-mp", "halo3", "halo3-odst"], "add-ons follow their game");
        assert_eq!(ids(&depots_for(&set(&["halo3", "halo3-odst"]))), [976731, 976738, 976739, 1064222], "ODST, when ticked");
        assert_eq!(add_on("halo3-odst").unwrap().game, "halo3");
        assert_eq!(add_on("halo2-mp").unwrap().game, "halo2");
        assert!(mod_("gravemind").unwrap().on_wine() && mod_("gravemind").unwrap().source.sums.is_none(), "Gravemind: GitHub's checksums");
        assert!(depots_for(&set(&["halo1"])).is_empty(), "Halo CE's native port downloads nothing from Steam");
        assert!(depots_for(&set(&[])).is_empty(), "nothing chosen, nothing needed");
        let all: HashSet<String> = games().iter().map(|g| g.id.clone()).collect(); // (Reach among them: never counted)
        assert!((DownloadPlan::new(&all, &HashSet::new()).gib() - (16.0 + 29.1 + 8.4 + 8.7)).abs() < 0.01, "total size");
        assert_eq!(ids(&DownloadPlan::new(&set(&["halo3"]), &[976731, 976738].into()).needed), [976739], "only what's missing");
        assert!(DownloadPlan::new(&set(&["halo3"]), &[976731, 976738, 976739].into()).is_empty(), "nothing missing");
        assert!(!depots_for(&all).iter().any(|d| [1064227, 1064228, 1064229, 1064274, 1064275].contains(&d.id)), "no language depots");
        assert!(steam_size(29.1) == "29.1 GB" && steam_size(16.0) == "16.0 GB", "sizes as Steam shows them");
        assert_eq!(windows_path("/Volumes/Library/Games/Halo MCC"), r"Z:\Volumes\Library\Games\Halo MCC");
        assert_eq!(extended_regex_escaped("/a b/Drop Pod (2)/x.y"), r"/a b/Drop Pod \(2\)/x\.y");
        assert_eq!(disc_mod().id, "chupathingyce");
    }

    /// A scratch folder, removed afterwards.
    struct Tmp(PathBuf);
    impl Drop for Tmp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn folders_config_and_launch_mode() {
        let tmp = Tmp(std::env::temp_dir().join(format!("droppod-tests-{}", std::process::id())));
        let mcc = tmp.0.join("Halo MCC");
        let touch = |rel: &str| {
            let p = mcc.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"").unwrap();
        };
        for m in &base_depot().markers { touch(m); }
        touch("halo3/halo3.dll");
        touch("halo3/maps/010_jungle.map"); // Halo 3 campaign, but not its multiplayer depot
        assert_eq!(present_depots(&mcc), [976731, 976738].into());
        assert!(installed_games(&present_depots(&mcc)).is_empty(), "Halo 3 needs its multiplayer depot too");
        touch("halo3/maps/guardian.map");
        assert_eq!(installed_games(&present_depots(&mcc)), ["halo3"], "Halo CE never counts as downloaded");
        assert!(looks_like_mcc(&mcc) && !looks_like_mcc(&tmp.0));
        assert!(installed_games(&[976733].into()).is_empty(), "nothing without the shared files");

        // the scripts read the same catalogue: games.py's list of the chosen games' mods
        let out = std::process::Command::new("/usr/bin/python3")
            .args(["-I", "-B"]).arg(repo().join("scripts/games.py")).args(["mods", "halo1", "halo3"])
            .output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout),
                   "chupathingyce mac - ChupathingyCE\nreclaimer wine mesa,spinfix,loopback,presets Project Reclaimer\n");

        // launch mode and config.json
        let h = tmp.0.join("home");
        fs::create_dir_all(&h).unwrap();
        assert_eq!(launch_mode(&h, true, true), LaunchMode::Setup, "nothing installed: setup");
        let mut config = AppConfig::new(mcc.to_str().unwrap());
        config.game_files_windows = Some(windows_path(mcc.to_str().unwrap()));
        config.source = "existing".into();
        config.owns_game_files = Some(false);
        config.games = vec!["halo3".into()];
        config.wanted = Some(vec!["halo3".into()]);
        fs::write(h.join("config.json"), serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(launch_mode(&h, true, true), LaunchMode::Setup, "config.json alone: setup");
        fs::write(h.join("version"), "0.1.0\n").unwrap();
        assert_eq!(launch_mode(&h, true, true), LaunchMode::Library, "installed: library");
        assert_eq!(launch_mode(&h, false, true), LaunchMode::HandOff, "disk image copy: hand off");
        assert_eq!(launch_mode(&h, false, false), LaunchMode::Library, "no installed copy: library here");
        fs::write(h.join(".setup-incomplete"), "").unwrap();
        assert_eq!(launch_mode(&h, true, true), LaunchMode::Setup, "stopped partway: setup again");
        assert_eq!(AppConfig::load(&h.join("config.json")).as_ref(), Some(&config), "config round trip");
        // the file games.py writes decodes too (extra keys like wine are fine)
        let written = r#"{"version": 1, "gameFiles": "/x", "gameFilesWindows": "Z:\\x", "source": "steam", "ownsGameFiles": true, "games": [], "wanted": ["halo3"], "wine": "/w"}"#;
        fs::write(h.join("config.json"), written).unwrap();
        assert_eq!(AppConfig::load(&h.join("config.json")).unwrap().wanted, Some(vec!["halo3".into()]));

        // "Play with": each game's first option unless a valid other choice is saved
        let mut pw = config.clone();
        assert!(pw.engine("halo3") == "reclaimer" && pw.engine("halo1") == "chupathingyce", "each game's mod by default");
        pw.play_with = Some([("halo3".into(), "chupathingyce".into()), ("halo1".into(), "reclaimer".into())].into());
        assert!(pw.engine("halo3") == "reclaimer" && pw.engine("halo1") == "chupathingyce", "a choice a game doesn't have falls back");
        assert!(pw.engine("halo2") == "gravemind" && pw.engine("halo4").is_empty(), "no game, no choice");

        // existing MCC folders are found through a link (the Reclaimer Mac port links its prefix to an external drive)
        let fake_home = tmp.0.join("user");
        let link = fake_home.join("Games/ProjectReclaimer/prefix-wine/drive_c/Program Files (x86)/Steam/steamapps/common/Halo The Master Chief Collection");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&mcc, &link).unwrap();
        let found = existing_mcc_candidates(&fake_home, &tmp.0.join("no-volumes"));
        assert_eq!(found, [fs::canonicalize(&mcc).unwrap()], "found via the port's link");
    }

    #[test]
    fn download_time_estimates() {
        let mut est = RateEstimator::default();
        assert_eq!(est.remaining(0.1, 100.0), None, "no estimate from one sample");
        assert_eq!(est.remaining(0.2, 160.0).as_deref(), Some("About 8 minutes left"));
    }
}
