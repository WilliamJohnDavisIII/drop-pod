// SPDX-License-Identifier: GPL-3.0-or-later
// Drop Pod's window. First opened (from the disk image or Applications) it is the setup wizard: it checks the Mac,
// lets the player choose games, where the game files are or go, and whether they come from Steam or an MCC folder
// they already have, then runs install.sh and shows its steps. Afterwards it is the main menu and the library: the
// games with Play or Get, Settings, and updates. The work itself is done by install.sh and the scripts it installs
// into $DROPPOD_HOME/scripts (play.sh, steam.py, updater.py, tools.py, mods.py); the window only runs them and shows
// what they say. Built by scripts/make_app.sh.
//   DropPod --render-states <dir> [qr-sample.txt]   a PNG of each screen state
//   DropPod --render-guide <dir>                    the disk image's "Open Anyway" picture (guide.png)
mod art;
mod core;
mod headless;
mod input;
mod library;
mod list;
mod paths;
mod proc;
mod ring;
mod screens;
mod setup;
mod sound;
mod states;
mod theme;

use eframe::egui;
use std::path::Path;
use std::sync::Arc;

struct Window {
    menu_done: bool,
    qr_up: bool, // a Steam QR code is on screen (setup, a Get, Repair): it was announced
    app: screens::App,
    input: input::Input,
    timing: Timing,
}

/// For measuring and testing the window: DROPPOD_TIMING=1 says when its first frame is on screen; DROPPOD_BENCH=1
/// moves through the menus for 8 s and says how long each frame took; DROPPOD_TEST_STEPS plays a script of moves,
/// separated by ";": up, down, left, right, accept, back, "wait <seconds>", "shot <name>" (a PNG of the window into
/// DROPPOD_TEST_SHOTS), quit.
#[derive(Default)]
struct Timing { on: bool, bench: bool, frames: u32, start: f64, last: f64, gaps: Vec<f64>,
                steps: std::collections::VecDeque<String>, next_step: f64 }

impl eframe::App for Window {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !std::mem::replace(&mut self.menu_done, true) { let _ = CTX.set(ctx.clone()); quit_closes_the_window(); }
        INSTALLING.store(self.app.installing() && !self.app.allow_close, std::sync::atomic::Ordering::Relaxed);
        let a = &mut self.app;
        a.setup.update();
        a.lib.update();
        if let Some(native) = a.setup.finished.take() {
            // on to the library, here or (from the disk image) in the copy now in Applications
            if paths::is_installed_app() || paths::var("DROPPOD_STAY").is_some() {
                a.mode = screens::Mode::Library;
                a.screen = screens::Screen::Main;
                if native { a.lib.open_setup(&core::disc_mod().id.clone()); }
                a.lib.start();
            } else {
                open_installed_app(if native { &["--setup-native-ce"] } else { &[] });
                a.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // a Steam QR code waits for the player's phone: say so out loud and bounce the Dock icon (until the window
        // is in front), so nobody misses it behind other windows
        let qr = a.setup.qr.is_some() || a.lib.run.as_ref().is_some_and(|r| r.qr.is_some()) || a.lib.settings.repair_qr.is_some();
        if qr && !self.qr_up {
            sound::play(sound::Sound::Attention);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Critical));
        }
        self.qr_up = qr;
        if std::mem::take(&mut a.lib.bring_to_front) { ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        if a.lib.quit { a.allow_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        if let Some(t) = [a.lib.wake_in(), a.setup.wake_in()].into_iter().flatten().reduce(f64::min) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(t));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // the 1280 × 720 design, scaled to the window
        let native = ctx.input(|i| i.content_rect().size()) * ctx.zoom_factor();
        let zoom = (native.x / ring::CANVAS.x).min(native.y / ring::CANVAS.y).max(0.3);
        if (zoom - ctx.zoom_factor()).abs() > 0.001 { ctx.set_zoom_factor(zoom); }
        if ctx.input(|i| i.viewport().close_requested()) && !self.app.allow_close && self.app.installing() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.app.ask_stop_installing();
        }
        let mut nav = self.input.frame(&ctx);
        self.timing.tick(&ctx, &mut nav);
        self.app.frame(ui, nav);
    }

    fn on_exit(&mut self) {
        self.app.lib.shutdown();
        self.app.setup.shutdown();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] { [0.01, 0.035, 0.08, 1.0] }
}

impl Timing {
    fn tick(&mut self, ctx: &egui::Context, nav: &mut Vec<input::Nav>) {
        self.script(ctx, nav);
        if !self.on && !self.bench { return; }
        let t = ctx.input(|i| i.time);
        self.frames += 1;
        if self.frames == 1 { self.start = t; ctx.request_repaint(); }
        if self.frames == 2 && self.on {
            let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
            println!("second-frame {wall:.3}");
        }
        if !self.bench { return; }
        // move every 0.3 s for 8 s (down the main menu, into the library, across the games), timing every frame
        let el = t - self.start;
        if el > 1.0 && el < 9.0 {
            let step = ((el - 1.0) / 0.3) as u32;
            let before = (((self.last - self.start) - 1.0).max(0.0) / 0.3) as u32;
            if step != before {
                nav.push(match step { 0..=5 => input::Nav::Down, 6 => input::Nav::Up, 7 => input::Nav::Accept, s if s % 6 < 3 => input::Nav::Left, _ => input::Nav::Right });
            }
            if self.last > 0.0 { self.gaps.push(t - self.last); }
        }
        if el >= 9.0 && !self.gaps.is_empty() {
            let mut g = std::mem::take(&mut self.gaps);
            g.sort_by(f64::total_cmp);
            let mean = g.iter().sum::<f64>() / g.len() as f64;
            println!("bench frames {} mean {:.2} ms ({:.0} fps) median {:.2} ms p95 {:.2} ms worst {:.2} ms",
                     g.len(), mean * 1000.0, 1.0 / mean, g[g.len() / 2] * 1000.0, g[g.len() * 95 / 100] * 1000.0, g[g.len() - 1] * 1000.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.last = t;
        ctx.request_repaint();
    }
}

impl Timing {
    fn script(&mut self, ctx: &egui::Context, nav: &mut Vec<input::Nav>) {
        if self.steps.is_empty() { return; }
        let t = ctx.input(|i| i.time);
        if t < self.next_step { ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.next_step - t)); return; }
        let step = self.steps.pop_front().unwrap();
        self.next_step = t + 0.45;
        let (word, rest) = step.split_once(' ').unwrap_or((step.as_str(), ""));
        use input::Nav::*;
        match word {
            "up" => nav.push(Up), "down" => nav.push(Down), "left" => nav.push(Left), "right" => nav.push(Right),
            "accept" => nav.push(Accept), "back" => nav.push(Back),
            "wait" => self.next_step = t + rest.parse::<f64>().unwrap_or(1.0),
            "shot" => {
                let path = std::path::PathBuf::from(paths::var("DROPPOD_TEST_SHOTS").unwrap_or_else(|| ".".into())).join(format!("{rest}.png"));
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let _ = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, img.as_raw().to_vec()).map(|i| i.save(&path));
                    println!("shot {}", path.display());
                })));
            }
            "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "cmd-q" => menu_quit(),
            other => eprintln!("DROPPOD_TEST_STEPS: what's \"{other}\"?"),
        }
        ctx.request_repaint();
    }
}

/// Is something installing that quitting would stop? (Kept current by the window, read when macOS asks to quit.)
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CTX: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

/// Quitting (Cmd-Q, the Dock's Quit, logging out) while something installs asks first, as the window's close button
/// does: macOS's "should the app quit?" becomes a close request the window answers with its question.
#[cfg(target_os = "macos")]
fn quit_closes_the_window() {
    use objc2::{ffi, runtime::{AnyObject, Imp, Sel}, sel};
    extern "C-unwind" fn should_terminate(_this: &AnyObject, _cmd: Sel, _app: &AnyObject) -> usize {
        if !INSTALLING.load(std::sync::atomic::Ordering::Relaxed) { return 1; } // NSTerminateNow
        if let Some(ctx) = CTX.get() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); ctx.request_repaint(); }
        0 // NSTerminateCancel
    }
    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else { return };
    let Some(delegate) = objc2_app_kit::NSApplication::sharedApplication(mtm).delegate() else { return };
    let object: &AnyObject = delegate.as_ref();
    let class = object.class();
    // (winit's application delegate doesn't answer this itself)
    unsafe {
        let imp: Imp = std::mem::transmute(should_terminate as extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize);
        ffi::class_addMethod(class as *const _ as *mut _, sel!(applicationShouldTerminate:), imp, c"Q@:@".as_ptr());
    }
}

#[cfg(not(target_os = "macos"))]
fn quit_closes_the_window() {}

/// (tests) choose the menu's Quit, as Cmd-Q does.
#[cfg(target_os = "macos")]
fn menu_quit() {
    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else { return };
    let Some(menu) = objc2_app_kit::NSApplication::sharedApplication(mtm).mainMenu() else { return };
    let Some(sub) = menu.itemAtIndex(0).and_then(|item| item.submenu()) else { return };
    for j in 0..sub.numberOfItems() {
        if sub.itemAtIndex(j).is_some_and(|it| it.keyEquivalent().to_string() == "q") { sub.performActionForItemAtIndex(j); }
    }
}

#[cfg(not(target_os = "macos"))]
fn menu_quit() {}

/// Open the installed app (after installing from the disk image, or when the disk image's copy is opened).
fn open_installed_app(args: &[&str]) {
    let mut c = std::process::Command::new("/usr/bin/open");
    c.arg(paths::installed_app());
    if !args.is_empty() { c.arg("--args").args(args); }
    let _ = c.status();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--render-guide") {
        return states::render_guide(Path::new(&args[i + 1]), "guide"); // (scripts/make_dmg.sh)
    }
    if let Some(i) = args.iter().position(|a| a == "--render-states") {
        return states::render_states(Path::new(&args[i + 1]), args.get(i + 2).map(Path::new));
    }
    let mode = core::launch_mode(paths::home(), paths::is_installed_app(), paths::installed_app().exists());
    if mode == core::LaunchMode::HandOff { return open_installed_app(&[]); }
    let name = core::app_name();
    let mut viewport = egui::ViewportBuilder::default().with_title(name).with_app_id(name)
        .with_inner_size([1280.0, 720.0]).with_min_inner_size([800.0, 450.0])
        // (a scripted test run stays behind whatever the person at the Mac is doing)
        .with_active(paths::var("DROPPOD_TEST_STEPS").is_none());
    if cfg!(target_os = "macos") {
        // the Dock shows the bundle's own icon (AppIcon.icns): eframe would put egui's in its place, unless the icon
        // it's given can't be read (it then leaves the app's icon alone)
        viewport = viewport.with_icon(egui::IconData { rgba: vec![], width: 1, height: 1 });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    if paths::var("DROPPOD_TEST_STEPS").is_some() { list::MOUSE.store(false, std::sync::atomic::Ordering::Relaxed); }
    let timing = Timing { on: paths::var("DROPPOD_TIMING").is_some(), bench: paths::var("DROPPOD_BENCH").is_some(),
                          steps: paths::var("DROPPOD_TEST_STEPS").unwrap_or_default().split(';').map(|s| s.trim().to_string())
                              .filter(|s| !s.is_empty()).collect(), ..Default::default() };
    eframe::run_native(name, options, Box::new(move |cc| {
        theme::install_fonts(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let waker: proc::Waker = Arc::new(move || ctx.request_repaint());
        let mut setup = setup::Setup::new(waker.clone());
        let mut lib = library::Library::new(waker.clone());
        let screen_mode = if mode == core::LaunchMode::Setup { setup.start(); screens::Mode::Setup } else { lib.start(); screens::Mode::Library };
        let app = screens::App::new(screen_mode, setup, lib, art::Art::new(Some(waker.clone())));
        Ok(Box::new(Window { menu_done: false, qr_up: false, app, input: input::Input::new(Some(waker)), timing }))
    })).expect("the window opens");
}
