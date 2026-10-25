// SPDX-License-Identifier: GPL-3.0-or-later
// Pictures of every screen, without clicking through them: `DropPod --render-states <dir> [qr-sample.txt]` (what
// make_app.sh states runs), and `DropPod --render-guide <dir>` for the disk image's picture of the "Open Anyway"
// step. Tile pictures come from an MCC folder when one is around (DROPPOD_SNAPSHOT_MCC, else the usual places).
use crate::art::Art;
use crate::core::*;
use crate::headless::Headless;
use crate::library::*;
use crate::proc::Waker;
use crate::ring::*;
use crate::screens::{App, Mode, Screen};
use crate::setup::{Check, Page, Setup, Source};
use crate::theme::*;
use eframe::egui::{self, Align2, Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PPP: f32 = 1.5;

fn noop() -> Waker { Arc::new(|| {}) }

fn save(h: &mut Headless, ctx: &egui::Context, app: &mut App, dir: &Path, name: &str) {
    let img = h.render(ctx, CANVAS, PPP, 45, |ui| app.frame(ui, vec![]));
    let path = dir.join(format!("{name}.png"));
    img.save(&path).expect("PNG written");
    println!("{}", path.display());
}

fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

pub fn render_states(dir: &Path, qr_sample: Option<&Path>) {
    std::fs::create_dir_all(dir).expect("folder for the pictures");
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut h = Headless::new();
    let qr_rows: Vec<String> = qr_sample.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
        .lines().filter_map(|l| l.strip_prefix("qr: ").map(str::to_owned)).collect();
    let qr = QrGrid::new(&qr_rows);
    let mcc: Option<PathBuf> = crate::paths::var("DROPPOD_SNAPSHOT_MCC").map(PathBuf::from).or_else(|| existing_mcc().into_iter().next());
    let files = mcc.as_ref().map_or("/Volumes/Games/Halo Game Files".to_string(), |p| p.to_string_lossy().into_owned());
    let all_steps = s(&["Mac settings (asks for your Mac password once)", "Getting the Windows compatibility layer",
                        "Getting the graphics translation layer", "Getting the Steam downloader", "Setting up the helper scripts",
                        "Setting up Project Reclaimer's own space", "Getting Project Reclaimer's menu graphics library",
                        "Downloading Project Reclaimer (plays Halo 3)", "Setting up the Play buttons", "Getting your game files"]);
    let new_app = |mode: Mode| {
        let mut setup = Setup::new(noop());
        setup.logging = false;
        let mut lib = Library::new(noop());
        lib.logging = false;
        App::new(mode, setup, lib, Art::new(None))
    };

    // the setup wizard
    let checks_ok = vec![
        Check { id: "mac".into(), title: "Apple Silicon Mac, macOS 27".into(), ok: Some(true), detail: String::new() },
        Check { id: "space".into(), title: "Free space on this Mac: 40 GB".into(), ok: Some(true), detail: String::new() },
        Check { id: "tools".into(), title: "Apple's developer tools".into(), ok: Some(true), detail: String::new() },
        Check { id: "rosetta".into(), title: "Rosetta (Apple's Intel translator)".into(), ok: None, detail: "Installed during setup; macOS asks for your password.".into() },
    ];
    let setup_states: Vec<(&str, Box<dyn Fn(&mut Setup)>)> = vec![
        ("20-setup-checks", Box::new(|st| { st.checks = checks_ok.clone(); st.page = Page::Checks; st.checks_passed = true; })),
        ("21-setup-blocked", Box::new(|st| {
            st.checks = vec![checks_ok[0].clone(), Check { id: "tools".into(), title: "Apple's developer tools".into(), ok: Some(false),
                detail: "A small free Apple component the helper scripts use. Press the button below: macOS asks to install it (click Install, then Agree). It takes about 5 minutes.".into() }];
            st.needs_dev_tools = true; st.page = Page::Checks; st.checks_passed = false;
        })),
        ("22-setup-games", Box::new(|st| st.page = Page::Games)),
        ("22b-setup-games-halo-ce-only", Box::new(|st| { st.page = Page::Games; st.chosen = ["halo1".to_string()].into(); })),
        ("23-setup-files-steam", Box::new(|st| {
            st.page = Page::Files; st.source = Source::Steam; st.download_folder = "/Volumes/Games/Halo Game Files".into(); st.files_free = 753e9;
        })),
        ("24-setup-files-existing", Box::new(|st| {
            st.page = Page::Files;
            match &mcc { Some(m) => st.use_existing(m), None => { st.existing_folder = Some(files.clone().into()); st.existing_found = [976731, 976738, 976739].into(); } }
            st.source = Source::Existing; st.files_free = 753e9;
        })),
        ("25-setup-qr", Box::new(|st| { st.steps = all_steps.clone(); st.qr = qr.clone(); st.page = Page::Installing; })),
        ("26-setup-download", Box::new(|st| {
            st.steps = all_steps.clone(); st.progress = Some(0.37); st.remaining = Some("About 2 hours left".into());
            st.status = Some("Downloading Halo 3 campaign (2 of 3)…".into()); st.page = Page::Installing;
        })),
        ("27-setup-failed", Box::new(|st| {
            st.steps = all_steps[..4].iter().cloned().chain(["Getting your game files".to_string()]).collect();
            st.page = Page::Failed("This Steam account doesn't own Halo: The Master Chief Collection.".into());
        })),
        ("28-setup-done", Box::new(|st| { st.steps = all_steps.iter().cloned().chain(["Saving your choices".to_string()]).collect(); st.page = Page::Done; })),
    ];
    for (name, configure) in &setup_states {
        let mut app = new_app(Mode::Setup);
        if let Some(m) = &mcc { app.setup.use_existing(m); app.setup.source = Source::Steam; }
        configure(&mut app.setup);
        save(&mut h, &ctx, &mut app, dir, name);
    }
    let mut app = new_app(Mode::Setup);
    app.setup.steps = all_steps.clone();
    app.setup.page = Page::Installing;
    app.ask_stop_installing();
    save(&mut h, &ctx, &mut app, dir, "29-setup-stop-installing");

    // the library
    let library = |installed: &[&str], reclaimer: bool, native: bool| {
        let mut app = new_app(Mode::Library);
        let l = &mut app.lib;
        let mut c = AppConfig::new(&files);
        c.source = "existing".into();
        c.games = s(installed);
        c.wanted = Some(s(&["halo3"]));
        l.config = Some(c);
        let chosen: HashSet<String> = s(installed).into_iter().collect();
        l.present = depots_for(&chosen).iter().map(|d| d.id).chain([base_depot().id]).collect();
        l.files_found = true;
        l.mods_there = s(&[if reclaimer { "reclaimer" } else { "" }, if native { &disc_mod().id } else { "" }]).into_iter().filter(|x| !x.is_empty()).collect();
        l.native_maps_there = native;
        l.update = Step::Done("Up to date".into());
        app.screen = Screen::Games;
        app.game_sel = games().iter().position(|g| g.id == "halo3").unwrap_or(0);
        app
    };
    let run = |purpose: &str, steps: &[String]| { let mut r = InstallRun::new(purpose); r.steps = steps.to_vec(); r };
    let three = &["halo3"]; // (Halo CE's native port needs no MCC files)
    let get = s(&["Getting your game files"]);

    let mut a = library(three, true, false); a.screen = Screen::Main;
    save(&mut h, &ctx, &mut a, dir, "00-main-menu");
    save(&mut h, &ctx, &mut library(three, true, false), dir, "01-library");
    let mut a = library(three, true, true); a.game_sel = 0;
    save(&mut h, &ctx, &mut a, dir, "02-library-all");
    let mut a = library(three, true, false); a.screen = Screen::Main; a.lib.update = Step::Working("Checking for updates…".into()); a.lib.update_progress = Some(0.4);
    save(&mut h, &ctx, &mut a, dir, "03-main-checking");
    let mut a = library(&[], false, false);
    let mut r = run("halo3", &get); r.qr = qr.clone(); a.lib.run = Some(r);
    save(&mut h, &ctx, &mut a, dir, "04-library-get-qr");
    a.lib.sheet = Some(Sheet::SignIn);
    save(&mut h, &ctx, &mut a, dir, "04b-library-get-signin-sheet");
    let mut a = library(&[], false, true);
    let mut r = run("halo3", &get); r.status = Some("Downloading Halo 3 campaign…".into()); r.progress = Some(0.37); r.remaining = Some("About 25 minutes left".into());
    a.lib.run = Some(r);
    save(&mut h, &ctx, &mut a, dir, "05-library-get-progress");
    let mut a = library(three, true, false); a.lib.playing = Some("halo3".into());
    save(&mut h, &ctx, &mut a, dir, "06-library-playing");
    let mut a = library(three, false, false);
    a.lib.game_error = Some("Halo 3 closed right after starting. Press Play to try again. If it keeps happening, use Settings > Make a report.".into());
    save(&mut h, &ctx, &mut a, dir, "07-library-game-closed");
    let mut a = library(three, true, false); a.lib.files_found = false;
    save(&mut h, &ctx, &mut a, dir, "08-library-files-missing");
    let mut a = library(three, true, false); a.lib.confirm_get(game("halo2").unwrap()); a.game_sel = 1;
    save(&mut h, &ctx, &mut a, dir, "09-library-confirm-get");
    let mut a = library(three, true, false); a.screen = Screen::Main;
    a.lib.controllers = Some(Step::Warning("Your Mac's game controller service is stuck, so games won't see a controller. Restart your Mac to fix it (Apple menu > Restart).".into()));
    save(&mut h, &ctx, &mut a, dir, "10-main-controller-stuck");
    let mut a = library(three, true, false); a.screen = Screen::Main;
    a.lib.network = Some(Step::Working("Fixing the network setup Halo 3 needs. macOS will ask for your Mac password.".into()));
    save(&mut h, &ctx, &mut a, dir, "11-main-network");
    let mut a = library(three, true, false); a.lib.notice = Some("Halo 3 is ready. Press Play.".into());
    save(&mut h, &ctx, &mut a, dir, "12-library-notice");
    save(&mut h, &ctx, &mut library(three, false, false), dir, "13-library-not-set-up");
    let mut a = library(three, true, false); a.game_sel = games().iter().position(|g| g.id == "haloreach").unwrap_or(0);
    save(&mut h, &ctx, &mut a, dir, "14-library-reach-not-installed");

    // the guided setups
    let reclaimer = mod_("reclaimer").unwrap();
    let mut a = library(three, true, false);
    a.lib.setup_mod = Some("renaissance".into()); a.lib.sheet = Some(Sheet::Mod);
    save(&mut h, &ctx, &mut a, dir, "15b-reach-explain");
    let mut a = library(three, false, false);
    a.lib.setup_mod = Some(reclaimer.id.clone()); a.lib.sheet = Some(Sheet::Mod);
    save(&mut h, &ctx, &mut a, dir, "15-reclaimer-explain");
    let rec_steps = s(&["Mac settings (asks for your Mac password once)", "Getting Project Reclaimer's menu graphics library",
                        "Downloading Project Reclaimer (plays Halo 3)", "Getting your game files"]);
    let mut r = run(&reclaimer.id, &rec_steps); r.status = Some("Downloading Halo 3 multiplayer…".into()); r.progress = Some(0.58); r.remaining = Some("About 6 minutes left".into());
    a.lib.run = Some(r);
    save(&mut h, &ctx, &mut a, dir, "16-reclaimer-working");
    let mut r = run(&reclaimer.id, &rec_steps.iter().cloned().chain(["Saving your choices".to_string()]).collect::<Vec<_>>()); r.phase = Phase::Done;
    a.lib.run = Some(r);
    save(&mut h, &ctx, &mut a, dir, "17-reclaimer-done");
    let mut r = run(&reclaimer.id, &rec_steps[..1]);
    r.phase = Phase::Failed("The Mac settings weren't changed (the password box was cancelled or refused). Press Try Again.".into());
    a.lib.run = Some(r);
    save(&mut h, &ctx, &mut a, dir, "18-reclaimer-failed");

    let mut a = library(three, false, false); a.game_sel = 0; a.lib.sheet = Some(Sheet::Disc);
    save(&mut h, &ctx, &mut a, dir, "19-native-explain");
    a.lib.native.image = Some("/Users/player/Downloads/Halo - Combat Evolved (USA).iso".into());
    a.lib.native.image_summary = Some("An Xbox disc image of Halo: Combat Evolved: its maps folder has 25 files (1.8 GB).".into());
    save(&mut h, &ctx, &mut a, dir, "19b-native-image-chosen");
    a.lib.native.page = NativePage::Working; a.lib.native.step = 2; a.lib.native.progress = Some(0.41);
    a.lib.native.status = Some("Copying Halo's maps from your disc image…".into());
    save(&mut h, &ctx, &mut a, dir, "19c-native-working");
    a.lib.native.page = NativePage::Done;
    save(&mut h, &ctx, &mut a, dir, "19d-native-done");
    a.lib.mods_there = [disc_mod().id.clone()].into();
    a.lib.native.page = NativePage::Failed("This isn't an Xbox disc image. Choose the .iso (or .xiso) made from your Halo disc.".into());
    save(&mut h, &ctx, &mut a, dir, "19e-native-failed");

    // Settings
    let mut a = library(three, true, false); a.screen = Screen::Main; a.lib.sheet = Some(Sheet::Settings);
    a.lib.settings.free_bytes = 753e9; a.lib.settings.crash_bytes = 3_260_701_923.0;
    a.lib.settings.say(format!("The report is on your Desktop: {} report 2026-10-17 22.31.zip", app_name()), true);
    save(&mut h, &ctx, &mut a, dir, "30-settings");
    let mut a = library(three, true, false); a.screen = Screen::Main; a.lib.sheet = Some(Sheet::Settings);
    a.lib.settings.busy = Some("Checking your game files against Steam…".into());
    a.lib.settings.repair_progress = Some(0.62); a.lib.settings.repair_remaining = Some("About 9 minutes left".into());
    save(&mut h, &ctx, &mut a, dir, "31-settings-repair");
    let mut a = library(&["halo2", "halo3"], true, false); a.screen = Screen::Main; a.lib.sheet = Some(Sheet::Settings); // (Halo 2 without its multiplayer maps)
    a.lib.settings.free_bytes = 753e9;
    save(&mut h, &ctx, &mut a, dir, "32-settings-add-on");
    let mut a = library(three, true, false); a.screen = Screen::Main; a.lib.sheet = Some(Sheet::Settings);
    a.lib.config.as_mut().unwrap().owns_game_files = Some(true);
    a.lib.ask_uninstall();
    save(&mut h, &ctx, &mut a, dir, "33-settings-uninstall");

    render_guide(dir, "40-open-anyway-guide");
}

/// The picture in the disk image: how to get past macOS's one-time block on an app not signed by Apple.
pub fn render_guide(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).expect("folder for the picture");
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut h = Headless::new();
    let size = Vec2::new(640.0, 560.0);
    let img = h.render(&ctx, size, 2.0, 2, |ui| guide(ui));
    let path = dir.join(format!("{name}.png"));
    img.save(&path).expect("PNG written");
    println!("{}", path.display());
}

fn guide(ui: &mut egui::Ui) {
    let p = ui.painter().clone();
    let r = ui.max_rect();
    backdrop(&p, r, None, None);
    let n = app_name();
    text(&p, Pos2::new(r.center().x, r.top() + 50.0), Align2::CENTER_CENTER, &format!("OPENING {} THE FIRST TIME", n.to_uppercase()), menu(17.0), INK, 4.0);
    fade_line(&p, Pos2::new(r.center().x, r.top() + 76.0), 420.0, rgba(110, 205, 255, 160));
    let mut y = rich(&p, Pos2::new(r.center().x, r.top() + 92.0), r.width() - 80.0,
                     "macOS blocks apps that aren't from the App Store or a paid Apple developer account, once. This is how to let it open:",
                     16.0, DIM, true).bottom() + 18.0;
    for (i, step) in [format!("Double-click **{n}**. macOS says it can't verify it: click **Done**."),
                      "Open **System Settings**, then **Privacy & Security**. Scroll down to Security.".to_string(),
                      format!("Next to “{n} was blocked”, click **Open Anyway**. Confirm with **Open Anyway** and your Mac password."),
                      format!("That's it. The {n} window opens and sets everything up; you won't see this again.")].iter().enumerate() {
        let tw = r.width() - 40.0 - 120.0;
        let th = rich_height(&p, tw, step, 17.0);
        let card = Rect::from_min_size(Pos2::new(r.left() + 40.0, y), Vec2::new(r.width() - 80.0, th.max(40.0) + 36.0));
        p.rect_filled(card, 4.0, rgba(6, 22, 44, 230));
        p.rect_stroke(card, 4.0, egui::Stroke::new(1.0, rgba(90, 170, 230, 90)), egui::StrokeKind::Inside);
        let c = Pos2::new(card.left() + 36.0, card.center().y);
        p.circle_filled(c, 17.0, rgba(40, 130, 220, 255));
        text(&p, c + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, &(i + 1).to_string(), menu(15.0), INK, 0.0);
        rich(&p, Pos2::new(card.left() + 74.0, card.center().y - th / 2.0), tw, step, 17.0, INK, false);
        y = card.bottom() + 14.0;
    }
}


