// SPDX-License-Identifier: GPL-3.0-or-later AND MIT
// Where things are: the app bundle and its Info.plist, $DROPPOD_HOME and what's in it. DROPPOD_* variables change
// where things go (tests, development); see install.sh. Ported from LauncherApp.swift (portions MIT, see
// LICENSE.reclaimer-mac-port).
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn var(name: &str) -> Option<String> { std::env::var(name).ok().filter(|v| !v.is_empty()) }

pub fn user_home() -> PathBuf { PathBuf::from(var("HOME").unwrap_or_else(|| "/".into())) }

/// The .app this runs from (Contents/MacOS/<exe> three levels up), if it runs from one.
pub fn bundle() -> Option<&'static Path> {
    static B: OnceLock<Option<PathBuf>> = OnceLock::new();
    B.get_or_init(|| {
        let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
        let app = exe.parent()?.parent()?.parent()?;
        (app.extension().is_some_and(|e| e == "app")).then(|| app.to_path_buf())
    }).as_deref()
}

/// Contents/Resources of the bundle (or, run from a build folder, the repository, where scripts/ lives).
pub fn resources() -> PathBuf {
    bundle().map(|b| b.join("Contents/Resources")).unwrap_or_else(|| repo().to_path_buf())
}

/// The repository this was built from (for running outside a bundle while developing).
pub fn repo() -> &'static Path { Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")) }

/// A string from the bundle's Info.plist (make_app.sh writes it).
pub fn info_plist_string(key: &str) -> Option<String> {
    static PLIST: OnceLock<Option<plist::Dictionary>> = OnceLock::new();
    let dict = PLIST.get_or_init(|| {
        let v = plist::Value::from_file(bundle()?.join("Contents/Info.plist")).ok()?;
        v.into_dictionary()
    });
    dict.as_ref()?.get(key)?.as_string().map(str::to_owned)
}

pub fn home() -> &'static Path {
    static H: OnceLock<PathBuf> = OnceLock::new();
    H.get_or_init(|| var("DROPPOD_HOME").map(PathBuf::from)
        .unwrap_or_else(|| user_home().join("Library/Application Support").join(crate::core::app_name())))
}
pub fn scripts_dir() -> PathBuf { home().join("scripts") }
pub fn config_path() -> PathBuf { home().join("config.json") }
pub fn apps_dir() -> PathBuf { var("DROPPOD_APPS_DIR").map(PathBuf::from).unwrap_or_else(|| user_home().join("Applications")) }

/// The copy in an Applications folder is the one that updates itself; the disk image's copy installs, then hands off.
pub fn is_installed_app() -> bool {
    let Some(b) = bundle() else { return false };
    let path = b.to_string_lossy();
    path.starts_with("/Applications/") || path.starts_with(&format!("{}/", apps_dir().to_string_lossy()))
}

pub fn installed_app() -> PathBuf {
    if is_installed_app() { return bundle().unwrap().to_path_buf(); }
    let system = PathBuf::from(format!("/Applications/{}.app", crate::core::app_name()));
    if system.exists() { system } else { apps_dir().join(format!("{}.app", crate::core::app_name())) }
}

/// install.sh and its scripts travel inside the app (Contents/Resources/install); DROPPOD_INSTALLER overrides it.
pub fn installer() -> Option<PathBuf> {
    var("DROPPOD_INSTALLER").map(PathBuf::from).or_else(|| bundle().map(|b| b.join("Contents/Resources/install/install.sh")))
}

pub fn app_version() -> String { info_plist_string("CFBundleShortVersionString").unwrap_or_else(|| "dev".into()) }

/// The GitHub repo the app updates from ("owner/name"); empty until the project is published.
pub fn app_repo() -> String {
    std::env::var("DROPPOD_REPO").ok().or_else(|| info_plist_string("DropPodRepo")).unwrap_or_default()
}

/// A Wine mod's game app, which install.sh makes (game-launch.sh.in): opened to start the mod, so the Mac shows the
/// running game as the mod (its name in the menu bar, Dock and Game Mode) instead of "wine".
pub fn game_app(mod_name: &str) -> PathBuf { home().join(format!("game.noindex/{mod_name}.app")) }

/// ChupathingyCE's own data folder (its maps, settings and saves), where its first start puts Halo's maps.
pub fn native_data_dir() -> PathBuf {
    var("DROPPOD_NATIVE_CE_DATA").map(PathBuf::from)
        .unwrap_or_else(|| user_home().join("Library/Application Support/ChupathingyCE"))
}

/// Free space where a folder is or will be (its nearest existing parent), as `df` (and install.sh) sees it.
#[cfg(windows)]
pub fn free_bytes(_: &Path) -> f64 { 1e15 } // not measured on Windows yet
#[cfg(not(windows))]
pub fn free_bytes(path: &Path) -> f64 {
    let mut p = path.to_path_buf();
    while !p.exists() && p.parent().is_some() { p.pop(); }
    let Ok(c) = std::ffi::CString::new(p.to_string_lossy().as_bytes()) else { return 0.0 };
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 { return 0.0; }
    s.f_bavail as f64 * s.f_bsize as f64
}

/// A path with the home folder as ~.
pub fn tilde(path: &Path) -> String {
    let p = path.to_string_lossy();
    let h = user_home().to_string_lossy().into_owned();
    if h.len() > 1 && p.starts_with(&h) { format!("~{}", &p[h.len()..]) } else { p.into_owned() }
}
