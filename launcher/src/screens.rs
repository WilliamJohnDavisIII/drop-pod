// SPDX-License-Identifier: GPL-3.0-or-later
// The window: the setup wizard the first time; afterwards the main menu, the library of games, and the sheets over
// it (Settings, a mod's guided setup, the disc setup, the Steam sign-in), plus the questions over those. Each screen
// is a list (list.rs) under a title, on the art of the game in front.
use crate::art::Art;
use crate::core::*;
use crate::input::Nav;
use crate::library::*;
use crate::list::{list, Row};
use crate::paths;
use crate::ring::*;
use crate::setup::{Page, Setup, Source};
use crate::sound::{self, Sound};
use crate::theme::*;
use std::path::PathBuf;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode { Setup, Library }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen { Main, Games }

/// Everything a list on any screen can choose.
#[derive(Debug, Clone, PartialEq)]
pub enum Id {
    // main menu
    Play, Settings, Quit,
    // library
    PlayGame, Get, SetUp, PlayWith, StopDownload,
    // Settings
    Stop, ShowFiles, AddOn(String), MoveFiles, Repair, CleanCrash, RepairApp, ForceQuit, Graphics, Steam, Logs, Report,
    Uninstall, Done,
    // guided setups
    Cancel, TryAgain, Close, ChooseImage, LetPort,
    // setup
    DevTools, CheckAgain, Continue, Toggle(String), Back, Install, FromSteam, FromFolder, ChangeFolder, ChooseFolder,
    OpenGames, NativeCe,
    // a question's buttons
    Button(usize),
}

pub struct App {
    pub mode: Mode,
    pub screen: Screen,
    pub setup: Setup,
    pub lib: Library,
    pub art: Art,
    pub game_sel: usize,
    /// A question of the window's own (stop installing?), over everything.
    pub dialog: Option<Dialog>,
    pub allow_close: bool,
    backdrop: Option<(String, f64)>,      // the picture behind, and since when
    backdrop_before: Option<String>,
}

const RING_ART: &str = "HaloRingVignette.png";

impl App {
    pub fn new(mode: Mode, setup: Setup, lib: Library, art: Art) -> Self {
        App { mode, screen: Screen::Main, setup, lib, art, game_sel: 0, dialog: None, allow_close: false,
              backdrop: None, backdrop_before: None }
    }

    pub fn installing(&self) -> bool {
        (self.mode == Mode::Setup && self.setup.installing()) || (self.mode == Mode::Library && self.lib.installing())
    }

    /// Closing the window mid-install stops it: say so first (Try Again picks up where it stopped).
    pub fn ask_stop_installing(&mut self) {
        self.dialog = Some(Dialog {
            title: "Stop installing?".into(),
            message: format!("You can carry on later: open {} again and it picks up where it stopped.", app_name()),
            buttons: vec![("Keep Installing".into(), Act::Close), ("Stop".into(), Act::Quit)],
        });
    }

    /// The game files the art (and the music) come from.
    fn game_files(&self) -> Option<PathBuf> {
        match self.mode {
            Mode::Setup => self.setup.existing_folder.clone().filter(|_| self.setup.folder_problem.is_none()),
            Mode::Library => self.lib.game_files().filter(|_| self.lib.files_found),
        }
    }

    /// Whose title music plays, and from where: Halo CE's on the main menu and in setup, the selected tile's in the
    /// library, none while a game runs.
    pub fn music(&self) -> (Option<PathBuf>, Option<&str>) {
        let game = match (self.mode, self.screen) {
            (Mode::Library, _) if self.lib.playing.is_some() => None,
            (Mode::Library, Screen::Games) => games().get(self.game_sel).map(|g| g.id.as_str()),
            _ => Some("halo1"),
        };
        (self.game_files(), game)
    }

    /// One frame: the screen in `full` (the window), laid out on the canvas in its middle.
    pub fn frame(&mut self, ui: &mut egui::Ui, nav: Vec<Nav>) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        let canvas = Rect::from_center_size(full.center(), CANVAS);
        let p = ui.painter().clone();

        // the art behind
        let folder = self.game_files();
        self.art.set_folder(folder.as_deref());
        let behind = match (self.mode, self.screen) {
            (Mode::Library, Screen::Games) => games().get(self.game_sel).map_or(RING_ART.to_string(), |g| g.art.clone()),
            _ => RING_ART.to_string(),
        };
        let now = ctx.input(|i| i.time);
        if self.backdrop.as_ref().map(|(b, _)| b) != Some(&behind) {
            self.backdrop_before = self.backdrop.take().map(|(b, _)| b);
            self.backdrop = Some((behind.clone(), now));
        }
        let since = self.backdrop.as_ref().map_or(0.0, |(_, t)| now - t);
        let fade = (1.0 - since / 0.35).clamp(0.0, 1.0) as f32;
        if fade > 0.0 { ctx.request_repaint(); }
        let art = self.art.get(&ctx, &behind, true);
        let before = self.backdrop_before.clone().filter(|_| fade > 0.0).and_then(|b| self.art.get(&ctx, &b, true));
        backdrop(&p, full, art.as_ref(), before.as_ref().map(|t| (t, fade)));

        // who gets the keys: the question on top, else the sheet, else the screen
        let top_dialog = self.dialog.is_some() || (self.mode == Mode::Library && self.lib.dialog.is_some());
        let sheet = self.mode == Mode::Library && self.lib.sheet.is_some();
        let (screen_nav, sheet_nav, dialog_nav) = if top_dialog { (vec![], vec![], nav) } else if sheet { (vec![], nav, vec![]) } else { (nav, vec![], vec![]) };
        match self.mode {
            Mode::Setup => self.setup_screen(ui, canvas, screen_nav),
            Mode::Library => {
                match self.screen {
                    Screen::Main => self.main_menu(ui, canvas, screen_nav),
                    Screen::Games => self.games(ui, canvas, screen_nav),
                }
                if sheet { veil(&p, full, 1.0); self.sheet(ui, canvas, sheet_nav); }
            }
        }
        if top_dialog {
            veil(&p, full, 0.8);
            let d = self.dialog.clone().or_else(|| self.lib.dialog.clone()).unwrap();
            if let Some(act) = dialog(ui, canvas, &d, dialog_nav) {
                if self.dialog.is_some() {
                    self.dialog = None;
                    if act == Act::Quit { self.allow_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                } else {
                    self.lib.choose(act);
                }
            }
        }
    }

    // MARK: main menu

    fn main_menu(&mut self, ui: &mut egui::Ui, c: Rect, nav: Vec<Nav>) {
        let p = ui.painter().clone();
        let cx = c.center().x;
        glow_text(&p, Pos2::new(cx, c.top() + 128.0), Align2::CENTER_CENTER, &app_name().to_uppercase(), menu(46.0), INK,
                  rgba(70, 190, 255, 28), 14.0);
        fade_line(&p, Pos2::new(cx, c.top() + 172.0), 420.0, rgba(110, 205, 255, 170));
        text(&p, Pos2::new(cx, c.top() + 192.0), Align2::CENTER_CENTER, &self.subtitle().to_uppercase(), bold(13.0), DIM, 3.5);
        let rows = vec![Row::action("Play", true, Id::Play), Row::action("Settings", true, Id::Settings), Row::action("Quit", true, Id::Quit)];
        let area = Rect::from_min_size(Pos2::new(cx - 300.0, c.top() + 300.0), Vec2::new(600.0, 170.0));
        let chosen = list(ui, "main", area, &rows, &nav, None);
        let sel: Option<Id> = crate::list::selected(ui.ctx(), "main");
        let about = match sel { Some(Id::Settings) => "Game files, repair, logs and uninstall".to_string(),
                                Some(Id::Quit) => format!("Close {}", app_name()), _ => "Choose a game and play it".into() };
        text(&p, Pos2::new(cx, area.bottom() + 24.0), Align2::CENTER_CENTER, &about, body(16.0), DIM, 0.5);
        match chosen {
            Some(Id::Play) => self.screen = Screen::Games,
            Some(Id::Settings) => self.lib.set_sheet(Some(Sheet::Settings)),
            Some(Id::Quit) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        self.messages(&p, c);
        self.status_lines(&p, c);
        footer(&p, c, &[("A", "SELECT")]);
    }

    fn subtitle(&self) -> String {
        if let Some(id) = &self.lib.playing { return format!("Playing {}. Quit the game to come back here.", Library::title_of(id)); }
        if self.lib.getting().is_some() { return "Downloading a game. You can play the others meanwhile.".into(); }
        "Community-made Halo, set up from one window".into()
    }

    /// Updates, and the Halo 3 network setup or a stuck controller service when there's something to say.
    fn status_lines(&self, p: &egui::Painter, c: Rect) {
        let mut rows: Vec<(&str, &Step, Option<f64>)> = vec![("Updates", &self.lib.update, self.lib.update_progress)];
        if let Some(n) = &self.lib.network { rows.push(("Halo 3 network setup", n, None)); }
        if let Some(n) = &self.lib.controllers { rows.push(("Controllers", n, None)); }
        let mut bottom = c.bottom() - 26.0;
        let w = 820.0;
        for (name, step, fraction) in rows.iter().rev() {
            let color = match step { Step::Waiting => FAINT, Step::Working(_) => CYAN, Step::Done(_) => OK, Step::Warning(_) => WARN };
            let line = format!("**{}**   {}", name.to_uppercase(), step.text());
            let h = rich_height(p, w, &line, 15.0);
            let top = bottom - h;
            p.circle_filled(Pos2::new(c.left() + 64.0, top + 10.0), 4.0, color);
            let r = rich(p, Pos2::new(c.left() + 76.0, top), w, &line, 15.0, DIM, false);
            if let Some(v) = fraction { progress(p, Rect::from_min_size(Pos2::new(r.right() + 14.0, top + 9.0), Vec2::new(120.0, 4.0)), *v); }
            bottom = top - 6.0;
        }
    }

    /// What went wrong or what's new, over the footer.
    fn messages(&self, p: &egui::Painter, c: Rect) {
        let mut lines: Vec<(String, Color32)> = vec![];
        if self.mode == Mode::Library && !self.lib.files_found {
            lines.push((format!("The game files aren't there ({}). Connect the drive they're on, or choose where they are now in Settings.",
                                self.lib.config.as_ref().map_or("?", |c| c.game_files.as_str())), WARN));
        }
        for e in [&self.lib.get_error, &self.lib.game_error].into_iter().flatten() { lines.push((e.clone(), WARN)); }
        if let Some(n) = &self.lib.notice { lines.push((n.clone(), DIM)); }
        let mut y = c.bottom() - 92.0;
        for (t, color) in lines.iter().rev() {
            let h = rich_height(p, 820.0, t, 15.0);
            y -= h;
            rich(p, Pos2::new(c.center().x, y), 820.0, t, 15.0, *color, true);
            y -= 6.0;
        }
    }

    // MARK: the library

    fn games(&mut self, ui: &mut egui::Ui, c: Rect, nav: Vec<Nav>) {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let cx = c.center().x;
        let all = games();
        if all.is_empty() { return; }
        self.game_sel = self.game_sel.min(all.len() - 1);
        let mut rest = vec![];
        for n in nav {
            match n {
                Nav::Left if self.game_sel > 0 => { self.game_sel -= 1; sound::play(Sound::Move); }
                Nav::Right if self.game_sel + 1 < all.len() => { self.game_sel += 1; sound::play(Sound::Move); }
                Nav::Back => { self.screen = Screen::Main; sound::play(Sound::Back); return; }
                Nav::Left | Nav::Right => sound::play(Sound::Error),
                other => rest.push(other),
            }
        }
        text(&p, Pos2::new(cx, c.top() + 50.0), Align2::CENTER_CENTER, "CHOOSE A GAME", menu(20.0), INK, 8.0);
        fade_line(&p, Pos2::new(cx, c.top() + 76.0), 340.0, rgba(110, 205, 255, 150));
        let sub = if self.lib.playing.is_some() || self.lib.getting().is_some() { self.subtitle() } else { String::new() };
        if !sub.is_empty() { text(&p, Pos2::new(cx, c.top() + 98.0), Align2::CENTER_CENTER, &sub, body(15.0), DIM, 0.3); }
        // a running Get: what, where it's at, how long
        if let (Some(r), Some(g)) = (self.lib.run.as_ref().filter(|r| r.phase == Phase::Running), self.lib.getting().and_then(game)) {
            let what = [r.status.clone().or_else(|| r.current().cloned()).or(Some("Starting…".into())),
                        r.progress.map(|v| format!("{}%", (v * 100.0) as i64)), r.remaining.clone()]
                .into_iter().flatten().collect::<Vec<_>>().join("  ·  ");
            let y = c.top() + 124.0;
            let head = format!("GETTING {}", g.title.to_uppercase());
            let hw = text_width(&p, &head, menu(12.0), 3.0);
            let s = middle_cut(&p, &what, body(15.0), 700.0);
            let total = 40.0 + hw + 16.0 + p.layout_no_wrap(s.clone(), body(15.0), INK).size().x;
            let x0 = cx - total / 2.0;
            dots(&p, &ctx, Pos2::new(x0, y), CYAN);
            text(&p, Pos2::new(x0 + 40.0, y), Align2::LEFT_CENTER, &head, menu(12.0), INK, 3.0);
            text(&p, Pos2::new(x0 + 40.0 + hw + 16.0, y), Align2::LEFT_CENTER, &s, body(15.0), DIM, 0.0);
        }

        // the tiles: MCC's own pictures
        let n = all.len() as f32;
        let gap = 24.0;
        let w = ((c.width() - 160.0 - (n - 1.0) * gap) / n).min(300.0);
        let h = w * 220.0 / 392.0;
        let x0 = cx - (n * w + (n - 1.0) * gap) / 2.0;
        let (pointer, clicked) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_clicked() && crate::list::mouse_on()));
        for (i, g) in all.iter().enumerate() {
            let sel = i == self.game_sel;
            let grow = ctx.animate_value_with_time(egui::Id::unique(("card", i)), if sel { 1.0 } else { 0.0 }, 0.14);
            let center = Pos2::new(x0 + i as f32 * (w + gap) + w / 2.0, c.top() + 160.0 + h / 2.0);
            let card = Rect::from_center_size(center, Vec2::new(w, h) * (1.0 + 0.1 * grow));
            if clicked && pointer.is_some_and(|pt| card.contains(pt)) && !sel { self.game_sel = i; sound::play(Sound::Move); }
            let state = self.lib.state(g);
            if grow > 0.01 {
                p.add(egui::Shadow { offset: [0, 0], blur: 28, spread: 2, color: rgba(60, 180, 255, (90.0 * grow) as u8) }.as_shape(card, 6.0));
            }
            let dim = matches!(state, TileState::NotDownloaded | TileState::Unavailable | TileState::NotHere);
            p.rect_filled(card, 6.0, rgba(14, 30, 52, 255));
            if let Some(t) = self.art.get(&ctx, &g.art, false) {
                let tint = if state == TileState::NotHere { Color32::from_gray(60) } else if dim { Color32::from_gray(105) }
                           else { Color32::from_gray((150.0 + 105.0 * grow) as u8) };
                cover(&p, card, &t, tint);
            }
            vgrad(&p, card, &[Color32::TRANSPARENT, Color32::TRANSPARENT, rgba(0, 0, 0, 190)]);
            text(&p, card.left_bottom() + Vec2::new(12.0, -15.0), Align2::LEFT_CENTER, &g.short.to_uppercase(), menu(13.0), INK, 2.5);
            let stroke = if sel { egui::Stroke::new(2.0, rgba(130, 215, 255, 255)) } else { egui::Stroke::new(1.0, rgba(60, 100, 140, 200)) };
            p.rect_stroke(card, 6.0, stroke, egui::StrokeKind::Outside);
            let tag = match state {
                TileState::NotDownloaded => Some(("NOT DOWNLOADED".to_string(), DIM)),
                TileState::Downloading => Some((self.lib.run.as_ref().and_then(|r| r.progress).map_or("DOWNLOADING".into(), |v| format!("DOWNLOADING {}%", (v * 100.0) as i64)), CYAN)),
                TileState::Playing => Some(("PLAYING".into(), OK)),
                TileState::NeedsSetup => Some(("NOT SET UP".into(), WARN)),
                TileState::Unavailable => Some(("FILES NOT FOUND".into(), WARN)),
                TileState::NotHere => Some((format!("NOT ON THIS {} YET", COMPUTER.to_uppercase()), DIM)),
                TileState::Ready => None,
            };
            if let Some((t, color)) = tag {
                let tw = p.layout_no_wrap(t.clone(), bold(11.0), INK).size().x + t.len() as f32 * 1.2 + 16.0;
                let tag = Rect::from_min_size(card.right_top() + Vec2::new(-tw - 8.0, 8.0), Vec2::new(tw, 20.0));
                p.rect_filled(tag, 3.0, rgba(0, 0, 0, 180));
                text(&p, tag.center(), Align2::CENTER_CENTER, &t, bold(11.0), color, 1.2);
            }
            if state == TileState::Downloading {
                progress(&p, Rect::from_min_size(card.left_bottom() + Vec2::new(0.0, -4.0), Vec2::new(card.width(), 4.0)),
                         self.lib.run.as_ref().and_then(|r| r.progress).unwrap_or(0.0));
            }
        }

        // the chosen game: its name, what it plays with, what you can do
        let g = &all[self.game_sel];
        let state = self.lib.state(g);
        let engine = mod_(&self.lib.engine(&g.id));
        let top = c.top() + 180.0 + h + 50.0;
        text(&p, Pos2::new(cx, top), Align2::CENTER_CENTER, &g.title.to_uppercase(), menu(28.0), INK, 6.0);
        let caption = match state {
            TileState::Ready => engine.map_or(String::new(), |m| format!("{}  ·  {}", m.name, m.caption)),
            TileState::NotDownloaded => format!("Not downloaded  ·  {}", steam_size(self.lib.get_plan(g).gib())),
            TileState::Downloading => self.lib.run.as_ref().and_then(|r| r.progress.map(|v| format!("Downloading  ·  {}%", (v * 100.0) as i64))
                .or_else(|| r.current().cloned())).unwrap_or_else(|| "Getting ready…".into()),
            TileState::Playing => "Quit the game to come back".into(),
            TileState::Unavailable => "Game files not found".into(),
            TileState::NeedsSetup => format!("{}  ·  not set up yet", engine.map_or("Its mod", |m| m.name.as_str())),
            TileState::NotHere => format!("{}  ·  not on this {COMPUTER} yet", engine.map_or("Its mod", |m| m.name.as_str())),
        };
        text(&p, Pos2::new(cx, top + 38.0), Align2::CENTER_CENTER, &caption.to_uppercase(), bold(15.0), CYAN, 3.0);
        let selected = g.options.iter().find(|o| Some(&o.id) == engine.map(|m| &m.id)).or(g.options.first());
        let mut action_top = top + 96.0;
        if let Some(why) = &g.unavailable {
            // why it's greyed out, in its own words
            let r = rich(&p, Pos2::new(cx, top + 60.0), 760.0, why, 15.0, DIM, true);
            action_top = action_top.max(r.bottom() + 20.0);
        } else if let Some(o) = selected {
            let line = if state == TileState::NeedsSetup { format!("Plays with {}", o.name) }
                       else if self.lib.ready(&o.id) { format!("Plays with {}  ·  set up", o.name) } else { format!("Plays with {}", o.name) };
            text(&p, Pos2::new(cx, top + 64.0), Align2::CENTER_CENTER, &line, body(15.0), DIM, 0.3);
        }
        let mut rows: Vec<Row<Id>> = vec![];
        match state {
            TileState::Ready | TileState::Unavailable => rows.push(Row::action("Play", self.lib.playing.is_none() && state != TileState::Unavailable, Id::PlayGame)),
            TileState::Playing => rows.push(Row::action("Playing", false, Id::PlayGame)),
            TileState::NotDownloaded => rows.push(Row::action(format!("Get  ·  {}", steam_size(self.lib.get_plan(g).gib())), !self.lib.installing(), Id::Get)),
            TileState::Downloading => rows.push(Row::action("Stop download", true, Id::StopDownload)),
            TileState::NeedsSetup => rows.push(Row::action(format!("Set up {}", engine.map_or("", |m| m.name.as_str())), true, Id::SetUp)),
            TileState::NotHere => rows.push(Row::action("Not available here", false, Id::PlayGame)),
        }
        if g.options.len() > 1 && g.available() {
            rows.push(Row::action(format!("Play with: {}", selected.map_or("", |o| o.name.as_str())), self.lib.playing.as_deref() != Some(&g.id), Id::PlayWith));
        }
        let area = Rect::from_min_size(Pos2::new(cx - 280.0, action_top), Vec2::new(560.0, 110.0));
        match list(ui, &format!("game-{}", g.id), area, &rows, &rest, None) {
            Some(Id::PlayGame) => { sound::play(Sound::Launch); self.lib.play(&g.id, None); }
            Some(Id::Get) => self.lib.confirm_get(g),
            Some(Id::StopDownload) => self.lib.stop_run(),
            Some(Id::SetUp) => { let e = self.lib.engine(&g.id); self.lib.open_setup(&e); }
            Some(Id::PlayWith) => self.lib.ask_play_with(g),
            _ => {}
        }
        self.messages(&p, c);
        let st = &self.lib.update;
        let color = match st { Step::Done(_) => OK, Step::Warning(_) => WARN, _ => CYAN };
        status_line(&p, Pos2::new(c.left() + 60.0, c.bottom() - 36.0), color, "UPDATES", st.text(), 520.0);
        footer(&p, c, &[("<>", "GAME"), ("A", "SELECT"), ("B", "BACK")]);
    }

    // MARK: sheets over the library

    fn sheet(&mut self, ui: &mut egui::Ui, c: Rect, nav: Vec<Nav>) {
        let p = ui.painter().clone();
        let r = Rect::from_center_size(c.center(), Vec2::new(900.0, 640.0));
        panel(&p, r);
        let inner = Rect::from_min_max(r.min + Vec2::new(50.0, 136.0), r.max - Vec2::new(50.0, 26.0));
        let back = nav.contains(&Nav::Back);
        match self.lib.sheet {
            Some(Sheet::Settings) => {
                title(&p, Rect::from_min_size(r.min - Vec2::new(0.0, 6.0), r.size()), "SETTINGS", "");
                let rows = self.settings_rows(&ui.ctx().clone());
                let chosen = list(ui, "settings", inner, &rows, &nav, None);
                if back { sound::play(Sound::Back); return self.lib.set_sheet(None); }
                self.settings_chosen(chosen);
            }
            Some(Sheet::Mod) => {
                let Some(m) = self.lib.setup_mod.as_deref().and_then(mod_) else { return self.lib.close_sheet() };
                title(&p, Rect::from_min_size(r.min - Vec2::new(0.0, 6.0), r.size()), &format!("SET UP {}", m.name.to_uppercase()),
                      &format!("What plays {} here, with its own menus.", plays(m)));
                let rows = self.mod_rows(&ui.ctx().clone(), m);
                let chosen = list(ui, &format!("mod-{}", m.id), inner, &rows, &nav, None);
                let running = self.lib.installing();
                if back && !running { sound::play(Sound::Back); return self.lib.close_sheet(); }
                match chosen {
                    Some(Id::SetUp) | Some(Id::TryAgain) => self.lib.set_up(m),
                    Some(Id::Cancel) | Some(Id::Close) | Some(Id::Done) => self.lib.close_sheet(),
                    Some(Id::Stop) => self.lib.stop_run(),
                    _ => {}
                }
            }
            Some(Sheet::Disc) => {
                title(&p, Rect::from_min_size(r.min - Vec2::new(0.0, 6.0), r.size()), "SET UP NATIVE HALO CE",
                      "The original Xbox Halo, rebuilt to run natively, with split-screen.");
                let rows = self.disc_rows(&ui.ctx().clone());
                let chosen = list(ui, "disc", inner, &rows, &nav, None);
                if back && self.lib.native.page != NativePage::Working { sound::play(Sound::Back); return self.lib.close_sheet(); }
                match chosen {
                    Some(Id::ChooseImage) => { if matches!(self.lib.native.page, NativePage::Failed(_)) { self.lib.native_reset(); } self.lib.native_choose_image() }
                    Some(Id::SetUp) | Some(Id::TryAgain) => self.lib.native_start(),
                    Some(Id::Cancel) | Some(Id::Close) | Some(Id::Done) => self.lib.close_sheet(),
                    Some(Id::Stop) => { self.lib.native_stop(); self.lib.native_reset(); }
                    Some(Id::LetPort) => {
                        self.lib.close_sheet();
                        if let Some(g) = games().iter().find(|g| g.options.contains(disc_mod())) { self.lib.play(&g.id, Some(&disc_mod().id)); }
                    }
                    _ => {}
                }
            }
            Some(Sheet::SignIn) => {
                let Some(run) = &self.lib.run else { return };
                let name = game(&run.purpose).map_or("the game".to_string(), |g| g.title.clone());
                title(&p, Rect::from_min_size(r.min - Vec2::new(0.0, 6.0), r.size()), "SIGN IN TO STEAM",
                      &format!("To download {name}. It downloads from your own Steam account. You only do this once: Steam remembers this {COMPUTER}."));
                let ctx = ui.ctx().clone();
                let qr = run.qr.clone();
                let rows = vec![Row::Gap(16.0), qr_row(&ctx, qr), Row::Gap(24.0), Row::action("Stop download", true, Id::Stop)];
                if list(ui, "signin", inner.shrink2(Vec2::new(90.0, 0.0)), &rows, &nav, None) == Some(Id::Stop) { self.lib.stop_run(); }
            }
            None => {}
        }
    }

    fn settings_rows(&self, ctx: &egui::Context) -> Vec<Row<Id>> {
        let l = &self.lib;
        let s = &l.settings;
        let n = app_name();
        let free = s.busy.is_none() && !l.installing(); // one at a time, and never mid-download
        let tool = |title: String, button: &str, detail: &str, id: Id| Row::Tool { title, detail: detail.into(), button: button.into(), enabled: free, indent: false, id };
        let mut rows = vec![];
        if let Some(busy) = &s.busy {
            rows.push(Row::Item { icon: Icon::Working, color: CYAN, text: busy.clone(), detail: String::new(), note: String::new() });
            if s.repair_qr.is_some() { rows.push(qr_row(ctx, s.repair_qr.clone())); }
            if let Some(v) = s.repair_progress { rows.push(progress_row(v, s.repair_remaining.clone())); }
            rows.push(Row::action("Stop", true, Id::Stop));
        }
        if let Some((t, ok)) = &s.result {
            rows.push(Row::item(if *ok { Icon::Ok } else { Icon::Warn }, if *ok { OK } else { WARN }, t.clone()));
        }
        rows.push(Row::Header("Game files".into()));
        let source = if l.config.as_ref().is_some_and(|c| c.source == "existing") { "A game files folder you already had, used where it is." } else { "Downloaded from your Steam account." };
        rows.push(Row::Tool {
            title: l.config.as_ref().map_or("Not set up yet".into(), |c| paths::tilde(std::path::Path::new(&c.game_files))),
            detail: if l.files_found { format!("{} free there. {source}", gigabytes(s.free_bytes)) } else { "That folder isn't there right now. Is its drive connected?".into() },
            button: if cfg!(target_os = "macos") { "Show in Finder" } else { "Show Folder" }.into(), enabled: l.files_found, indent: false, id: Id::ShowFiles,
        });
        for a in games().iter().flat_map(|g| &g.addons).filter(|a| l.missing(a)) {
            rows.push(tool(format!("{} {}", game(&a.game).map_or("", |g| g.short.as_str()), a.name.to_lowercase()),
                           &format!("Get ({})", steam_size(a.gib())), &a.about, Id::AddOn(a.id.clone())));
        }
        rows.push(tool("Moved your game files?".into(), "Choose…", &format!("Point {n} at the folder they're in now, for example on another drive. Nothing is copied."), Id::MoveFiles));
        rows.push(Row::Header("Repair".into()));
        rows.push(tool("Repair game files".into(), "Repair", "Deletes crash reports, then checks your games' files against Steam and downloads anything missing or damaged. Quit the game first.", Id::Repair));
        if s.crash_bytes > 0.0 {
            rows.push(tool(format!("Delete crash reports ({})", gigabytes(s.crash_bytes)), "Delete",
                           "When a game crashes it leaves a memory dump in your game files folder. Nothing needs it later.", Id::CleanCrash));
        }
        rows.push(tool(format!("Repair {n}"), "Repair", "Sets up the Windows layer, graphics translation and Play buttons again. Your saves and game files stay.", Id::RepairApp));
        rows.push(tool("Force quit the game".into(), "Force Quit", "For a frozen game window that won't close, even with Force Quit.", Id::ForceQuit));
        let installed = l.installed();
        if games().iter().any(|g| installed.contains(&g.id) && mod_(&l.engine(&g.id)).is_some_and(|m| m.has("presets"))) {
            rows.push(tool("Restore recommended graphics".into(), "Restore", "Restores Halo 3's tuned settings and fills in missing Reach settings (60 fps, V-Sync). Quit the game first.", Id::Graphics));
        }
        rows.push(tool("Use a different Steam account".into(), "Switch…", "Forgets the Steam account used for downloads; the next download shows a new QR code.", Id::Steam));
        rows.push(Row::Header("Logs".into()));
        rows.push(tool("Show the logs".into(), "Show", if cfg!(target_os = "macos") { "Each game's last session and the installer's messages, in a Finder window." } else { "Each game's last session and the installer's messages, in a folder window." }, Id::Logs));
        rows.push(tool("Make a report".into(), "Make Report", "Puts the logs in a zip file on your Desktop to send to whoever set this up for you. It doesn't include your Steam account.", Id::Report));
        rows.push(Row::Header("Remove".into()));
        rows.push(tool(format!("Uninstall {n}"), "Uninstall…", &format!("Removes the app, the Windows layer, the mods, the games' crash reports{} from this {COMPUTER}. Your game files stay unless you choose otherwise. Your Steam account isn't affected.", if cfg!(target_os = "macos") { " and the network setting" } else { "" }), Id::Uninstall));
        rows.push(Row::Gap(10.0));
        rows.push(Row::Text { text: format!("{n} {}", paths::app_version()), color: FAINT, size: 13.0, center: false });
        rows.push(Row::action("Done", true, Id::Done));
        rows
    }

    fn settings_chosen(&mut self, chosen: Option<Id>) {
        let l = &mut self.lib;
        match chosen {
            Some(Id::Stop) => l.settings_cancel(),
            Some(Id::ShowFiles) => if let Some(f) = l.game_files() { crate::proc::open_in_finder(&f, true) },
            Some(Id::AddOn(a)) => if let Some(a) = add_on(&a) { l.set_sheet(None); l.get_add_on(a); },
            Some(Id::MoveFiles) => l.choose_game_files(),
            Some(Id::Repair) => l.repair(),
            Some(Id::CleanCrash) => l.clean_crash_reports(),
            Some(Id::RepairApp) => l.repair_setup(),
            Some(Id::ForceQuit) => l.force_quit(),
            Some(Id::Graphics) => l.ask_restore_graphics(),
            Some(Id::Steam) => l.ask_switch_steam(),
            Some(Id::Logs) => crate::proc::open_in_finder(&paths::home().join("logs"), false),
            Some(Id::Report) => l.make_report(),
            Some(Id::Uninstall) => l.ask_uninstall(),
            Some(Id::Done) => l.set_sheet(None),
            _ => {}
        }
    }

    fn mod_rows(&self, ctx: &egui::Context, m: &Mod) -> Vec<Row<Id>> {
        let l = &self.lib;
        let mut rows = vec![];
        if let Some(run) = l.run.as_ref().filter(|r| r.purpose == m.id) {
            rows.extend(steps_rows(ctx, &run.steps, &run.phase, run.status.clone(), run.progress, run.remaining.clone(), run.qr.clone(), None));
            rows.push(Row::Gap(8.0));
            match &run.phase {
                Phase::Running => {
                    rows.push(Row::note(format!("Keep this open. You can use your {COMPUTER} meanwhile.")));
                    rows.push(Row::action("Stop", true, Id::Stop));
                }
                Phase::Failed(e) => {
                    rows.push(Row::item(Icon::Warn, WARN, e.clone()));
                    rows.push(Row::action("Try again", true, Id::TryAgain));
                    rows.push(Row::action("Close", true, Id::Close));
                    rows.push(Row::note("It picks up where it stopped."));
                }
                Phase::Done => {
                    rows.push(Row::item(Icon::Ok, OK, format!("**{} is ready.**", m.name)));
                    rows.push(Row::text(format!("Press Play on the {} tile.", games().iter().filter(|g| g.options.contains(m)).map(|g| g.short.as_str()).collect::<Vec<_>>().join(" or ")), DIM));
                    rows.push(Row::action("Done", true, Id::Done));
                }
            }
            return rows;
        }
        let its: Vec<&Game> = games().iter().filter(|g| g.options.contains(m)).collect();
        let installed = l.installed();
        let games_there = its.iter().all(|g| g.depots.is_empty() || installed.contains(&g.id));
        rows.push(Row::text(m.about.clone(), INK));
        rows.push(Row::Header("What gets set up".into()));
        rows.push(Row::Item { icon: Icon::Bullet, color: CYAN, text: format!("{}, from its own releases on GitHub, checked against its published checksum", m.name),
                              detail: String::new(), note: format!("{} MB", m.mb as i64) });
        if m.has("mesa") || m.has("spinfix") {
            let t = if m.has("mesa") && m.has("spinfix") { "A graphics library its menus need (Mesa), and a fix that saves a CPU core" }
                    else if m.has("mesa") { "A graphics library its menus need (Mesa)" } else { "A fix that saves a CPU core" };
            rows.push(Row::Item { icon: Icon::Bullet, color: CYAN, text: t.into(), detail: String::new(), note: if m.has("mesa") { "70 MB".into() } else { String::new() } });
        }
        if m.has("msruntimes") && cfg!(target_os = "macos") {
            rows.push(Row::Item { icon: Icon::Bullet, color: CYAN, text: "Microsoft's Visual C++ runtime, Edge WebView2 and shader compiler its menus need".into(), detail: String::new(), note: "240 MB".into() });
        }
        if m.has("chromehoist") && cfg!(target_os = "macos") {
            rows.push(Row::Item { icon: Icon::Bullet, color: CYAN, text: "A fix that makes its menus show on a Mac".into(), detail: String::new(), note: String::new() });
        }
        for d in catalogue().depots.iter().filter(|d| m.depots.contains(&d.id)) {
            let there = l.present.contains(&d.id);
            rows.push(Row::Item { icon: if there { Icon::Ok } else { Icon::Bullet }, color: if there { OK } else { CYAN },
                                  text: if there { format!("{}'s files: already there", d.name) } else { format!("{}'s files, from your Steam account", d.name) },
                                  detail: String::new(), note: if there { String::new() } else { steam_size(d.gib) } });
        }
        if m.has("loopback") {
            rows.push(Row::item(Icon::Bullet, CYAN, "Local network addresses it needs. macOS asks for your Mac password once."));
        }
        if !games_there { rows.push(Row::item(Icon::Warn, WARN, format!("Get {} first: its tile has a Get button.", plays(m)))); }
        rows.push(Row::Gap(8.0));
        rows.push(Row::action(format!("Set up {}", m.name), games_there && !l.installing(), Id::SetUp));
        rows.push(Row::action("Cancel", true, Id::Cancel));
        if l.installing() { rows.push(Row::note("Wait for the download that's running to finish.")); }
        rows
    }

    fn disc_rows(&self, ctx: &egui::Context) -> Vec<Row<Id>> {
        let l = &self.lib;
        let s = &l.native;
        let steps = native_steps();
        let mut rows = vec![];
        match &s.page {
            NativePage::Explain => {
                rows.push(Row::Header("It needs your own Halo disc".into()));
                rows.push(Row::text(format!("The native port is ChupathingyCE, a fan project. It can't use the Steam game files: it plays the original Xbox game, so it needs that game's data from **your own Halo: Combat Evolved Xbox disc**, as a disc image (.iso or .xiso). Nobody can legally give you that file, so {} never downloads it. You make it from your disc, for example with an Xbox or a disc drive and a ripping tool.", app_name()), INK));
                rows.push(Row::Gap(6.0));
                for (i, t) in steps.iter().enumerate() {
                    let done = i == 2 && l.native_maps_there();
                    rows.push(Row::item(if done { Icon::Ok } else { Icon::Bullet }, if done { OK } else { CYAN }, t.clone()));
                }
                rows.push(Row::Gap(6.0));
                if l.native_maps_there() {
                    rows.push(Row::item(Icon::Ok, OK, format!("Halo's maps are already on this {COMPUTER}, from an earlier setup: no disc image needed.")));
                } else {
                    rows.push(Row::Tool {
                        title: s.image.as_ref().and_then(|i| i.file_name()).map_or("No disc image chosen yet".into(), |n| n.to_string_lossy().into_owned()),
                        detail: if s.checking { "Checking it…".into() } else { s.image_summary.clone().or(s.image_problem.clone()).unwrap_or_else(|| "The .iso or .xiso made from your own disc.".into()) },
                        button: if s.image.is_none() { "Choose Disc Image…".into() } else { "Choose Another…".into() },
                        enabled: true, indent: false, id: Id::ChooseImage,
                    });
                    if let Some(pr) = &s.image_problem { rows.push(Row::item(Icon::Fail, WARN, pr.clone())); }
                    else if s.image_summary.is_some() { rows.push(Row::item(Icon::Ok, OK, "That's the one.")); }
                }
                rows.push(Row::Gap(8.0));
                rows.push(Row::action("Set up", l.native_can_start(), Id::SetUp));
                rows.push(Row::action("Cancel", true, Id::Cancel));
            }
            NativePage::Working => {
                for (i, t) in steps.iter().enumerate() {
                    let (ic, color) = if i < s.step { (Icon::Ok, OK) } else if i == s.step { (Icon::Working, CYAN) } else { (Icon::Pending, FAINT) };
                    rows.push(Row::Item { icon: ic, color, text: if i == s.step { format!("**{t}**") } else { t.clone() },
                                          detail: if i == s.step { s.status.clone().unwrap_or_default() } else { String::new() }, note: String::new() });
                    if i == s.step { if let Some(v) = s.progress { rows.push(progress_row(v, None)); } }
                }
                rows.push(Row::Gap(8.0));
                rows.push(Row::note("Keep this open. It takes a few minutes."));
                rows.push(Row::action("Stop", true, Id::Stop));
            }
            NativePage::Failed(e) => {
                rows.push(Row::item(Icon::Warn, WARN, e.clone()));
                rows.push(Row::Gap(8.0));
                rows.push(Row::action("Try again", l.native_can_start(), Id::TryAgain));
                rows.push(Row::action("Choose another disc image…", true, Id::ChooseImage));
                rows.push(Row::action("Close", true, Id::Close));
                if l.mods_there.contains(&disc_mod().id) {
                    rows.push(Row::action("Or let the port copy the maps itself", true, Id::LetPort));
                    rows.push(Row::note("It asks for the disc image when it opens."));
                }
            }
            NativePage::Done => {
                rows.push(Row::item(Icon::Ok, OK, "**Native Halo CE is ready.**"));
                rows.push(Row::text("The port is set up and Halo's maps from your disc are in place. Press Play on the Halo CE tile.", DIM));
                rows.push(Row::Gap(8.0));
                rows.push(Row::action("Done", true, Id::Done));
            }
        }
        let _ = ctx;
        rows
    }

    // MARK: setup

    fn setup_screen(&mut self, ui: &mut egui::Ui, c: Rect, nav: Vec<Nav>) {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let s = &self.setup;
        let n = app_name();
        let subtitle = match s.page {
            Page::Checks => "Let's get Halo running",
            Page::Games => "Step 2 of 3: which games?",
            Page::Files => "Step 3 of 3: where are the game files?",
            _ => "Setting up",
        };
        title(&p, c, &format!("SET UP {}", n.to_uppercase()), subtitle);
        let area = Rect::from_min_max(Pos2::new(c.left() + 190.0, c.top() + 140.0), Pos2::new(c.right() - 190.0, c.bottom() - 70.0));
        let mut rows: Vec<Row<Id>> = vec![];
        let mut prefer = Some(Id::Continue);
        match &s.page {
            Page::Checks => {
                for ch in &s.checks {
                    let (ic, color) = match ch.ok { Some(true) => (Icon::Ok, OK), Some(false) => (Icon::Fail, BAD), None => (Icon::Info, DIM) };
                    rows.push(Row::Item { icon: ic, color, text: format!("**{}**", ch.title), detail: ch.detail.clone(), note: String::new() });
                }
                if s.needs_dev_tools {
                    let waiting = s.waiting_for_dev_tools.is_some();
                    rows.push(Row::action(if waiting { "Waiting for the install to finish…" } else { "Install Apple's developer tools" }, !waiting, Id::DevTools));
                    prefer = Some(Id::DevTools);
                }
                if !s.checks_passed && !s.checks.is_empty() {
                    rows.push(Row::action("Check again", s.waiting_for_dev_tools.is_none(), Id::CheckAgain));
                }
                rows.push(Row::Gap(10.0));
                rows.push(Row::text("Each game plays on a community-made mod; you choose the games next. Halo 3 plays on Project Reclaimer, from **Halo: The Master Chief Collection on Steam**: you need the **Steam app on your phone** (you scan a code with it to download from your own account) and your **Mac password** once. Halo CE plays on ChupathingyCE, from your own **Xbox disc**, added at the end: no Steam needed.", DIM));
                rows.push(Row::Gap(10.0));
                rows.push(Row::action("Continue", s.checks_passed, Id::Continue));
            }
            Page::Games => {
                for g in games() {
                    let on = s.chosen.contains(&g.id) && g.available();
                    rows.push(Row::Check {
                        title: g.title.clone(),
                        sub: match &g.unavailable {
                            Some(why) => why.clone(),
                            None => g.options.first().map_or(String::new(), |m| format!("Plays on {} · {}", m.name, m.caption)),
                        },
                        right: if !g.available() { format!("Not on this {COMPUTER} yet") }
                               else if g.depots.is_empty() { format!("{} MB", g.options.first().map_or(0, |m| m.mb as i64)) } else { steam_size(g.gib()) },
                        on, enabled: g.available(), indent: false, id: Id::Toggle(g.id.clone()),
                    });
                    for a in &g.addons {
                        rows.push(Row::Check { title: a.name.clone(), sub: a.about.clone(), right: steam_size(a.gib()),
                                               on: s.chosen.contains(&a.id), enabled: on, indent: true, id: Id::Toggle(a.id.clone()) });
                    }
                }
                if s.needs_files() {
                    rows.push(Row::Item { icon: Icon::Info, color: DIM,
                        text: format!("Plus {} of files the Steam games share.", steam_size(base_depot().gib)), detail: String::new(),
                        note: format!("Total: {}", steam_size(DownloadPlan::new(&s.chosen, &Default::default()).gib())) });
                }
                for g in games().iter().filter(|g| g.depots.is_empty() && g.options.first().is_some_and(|m| m.setup == "xbox-disc")) {
                    rows.push(Row::text(format!("{} needs no Steam download: it plays from your own Xbox disc, added at the end.", g.short), DIM));
                }
                rows.push(Row::Gap(6.0));
                if s.needs_files() || s.chosen_games().is_empty() {
                    rows.push(Row::action("Continue", !s.chosen_games().is_empty(), Id::Continue));
                } else { // (nothing comes from MCC's files: no game files page)
                    rows.push(Row::action("Install", s.can_install(), Id::Install));
                    prefer = Some(Id::Install);
                }
                rows.push(Row::action("Back", true, Id::Back));
            }
            Page::Files => {
                rows.push(Row::Radio { title: "Download them from Steam".into(), on: s.source == Source::Steam, id: Id::FromSteam,
                                       detail: "Sign in with the Steam app on your phone; they download from your own Steam account.".into() });
                if s.source == Source::Steam {
                    rows.push(Row::Tool { title: paths::tilde(&s.download_folder), detail: "Where they go.".into(), button: "Change…".into(), enabled: true, indent: true, id: Id::ChangeFolder });
                }
                rows.push(Row::Radio { title: "Use a game files folder I already have".into(), on: s.source == Source::Existing, id: Id::FromFolder,
                                       detail: "Games stay where they are. Anything missing downloads into the folder.".into() });
                if s.source == Source::Existing {
                    match &s.existing_folder {
                        Some(f) => {
                            let found: Vec<String> = installed_games(&s.existing_found).iter().filter_map(|g| game(g).map(|g| g.short.clone())).collect();
                            rows.push(Row::Tool { title: paths::tilde(f), button: "Choose…".into(), enabled: true, indent: true, id: Id::ChooseFolder,
                                detail: s.folder_problem.clone().unwrap_or_else(|| format!("Found: {}", if found.is_empty() { "the shared files only".into() } else { found.join(", ") })) });
                        }
                        None => rows.push(Row::action("Choose the folder…", true, Id::ChooseFolder)),
                    }
                }
                rows.push(Row::Gap(6.0));
                let plan = s.plan();
                let volume = s.files_free;
                rows.push(Row::Item {
                    icon: if s.space_ok() { Icon::Ok } else { Icon::Fail }, color: if s.space_ok() { OK } else { BAD },
                    text: format!("**{}**", if plan.is_empty() { "Nothing to download: every game you chose is there".into() }
                        else if s.source == Source::Existing { format!("Downloads {} into this folder", steam_size(plan.gib())) }
                        else { format!("Downloads {} from your Steam account", steam_size(plan.gib())) }),
                    detail: if plan.is_empty() { format!("{} free there.", gigabytes(volume)) }
                        else if s.space_ok() { format!("{} free, so it fits. Keep this window open: it can take a few hours.", gigabytes(volume)) }
                        else { format!("Only {} free there. Free up {} or choose another folder, like an external drive.", gigabytes(volume), gigabytes(plan.bytes() - volume)) },
                    note: String::new(),
                });
                rows.push(Row::Gap(6.0));
                rows.push(Row::action("Install", s.can_install(), Id::Install));
                rows.push(Row::action("Back", true, Id::Back));
                prefer = Some(Id::Install);
            }
            Page::Installing | Page::Failed(_) | Page::Done => {
                if s.page == Page::Done {
                    rows.push(Row::item(Icon::Ok, OK, format!("**Everything's set up: {} steps done.**", s.steps.len())));
                    rows.push(Row::Gap(4.0));
                    rows.push(Row::text("**All set!**", OK));
                    if !paths::is_installed_app() {
                        rows.push(Row::text(format!("{n} is in your Applications folder now, so you can eject the disk image and move it to the Trash."), DIM));
                    }
                    rows.push(Row::Gap(8.0));
                    let disc_chosen = s.chosen.iter().any(|g| game(g).and_then(|g| g.options.first()).is_some_and(|m| m.setup == "xbox-disc"));
                    rows.push(Row::Item { icon: Icon::Info, color: CYAN,
                        text: format!("**{}**", if disc_chosen { "Next: your Halo CE disc" } else { "Optional: native Halo CE" }),
                        detail: "The original Xbox Halo, rebuilt to run natively, with split-screen. It needs a disc image of your own Halo: Combat Evolved Xbox disc. You can also do this later from the Halo CE tile.".into(),
                        note: String::new() });
                    rows.push(Row::Gap(6.0));
                    rows.push(Row::action("Open my games", true, Id::OpenGames));
                    rows.push(Row::action("Set up native Halo CE…", true, Id::NativeCe));
                    prefer = Some(Id::OpenGames);
                } else {
                    let phase = match &s.page { Page::Failed(e) => Phase::Failed(e.clone()), _ => Phase::Running };
                    rows.extend(steps_rows(&ctx, &s.steps, &phase, s.status.clone(), s.progress, s.remaining.clone(), s.qr.clone(), Some(5)));
                    if s.steps.is_empty() && s.page == Page::Installing { rows.push(Row::item(Icon::Working, CYAN, "Starting…")); }
                    rows.push(Row::Gap(8.0));
                    match &s.page {
                        Page::Failed(e) => {
                            rows.push(Row::item(Icon::Warn, WARN, e.clone()));
                            rows.push(Row::action("Try again", true, Id::TryAgain));
                            rows.push(Row::action("Back", true, Id::Back));
                            rows.push(Row::note("It picks up where it stopped."));
                            prefer = Some(Id::TryAgain);
                        }
                        _ => rows.push(Row::note(format!("Installing… keep this window open. Your {COMPUTER} can stay in use meanwhile."))),
                    }
                }
            }
        }
        let page_key = format!("setup-{:?}", std::mem::discriminant(&s.page));
        let back = nav.contains(&Nav::Back);
        let chosen = list(ui, &page_key, area, &rows, &nav, prefer.as_ref());
        let s = &mut self.setup;
        let go_back = |s: &mut Setup| {
            s.page = match &s.page { Page::Games => Page::Checks, Page::Files => Page::Games, Page::Failed(_) => if s.needs_files() { Page::Files } else { Page::Games }, p => p.clone() };
        };
        if back && matches!(s.page, Page::Games | Page::Files | Page::Failed(_)) { sound::play(Sound::Back); go_back(s); }
        match chosen {
            Some(Id::DevTools) => s.install_dev_tools(),
            Some(Id::CheckAgain) => s.run_checks(),
            Some(Id::Continue) => s.page = if s.page == Page::Checks { Page::Games } else { Page::Files },
            Some(Id::Toggle(id)) => s.toggle(&id),
            Some(Id::Back) => go_back(s),
            Some(Id::Install) | Some(Id::TryAgain) => s.install(),
            Some(Id::FromSteam) => s.set_source(Source::Steam),
            Some(Id::FromFolder) => s.set_source(Source::Existing),
            Some(Id::ChangeFolder) => s.choose_folder(false),
            Some(Id::ChooseFolder) => s.choose_folder(true),
            Some(Id::OpenGames) => s.finish(false),
            Some(Id::NativeCe) => s.finish(true),
            _ => {}
        }
        let hints: &[(&str, &str)] = match self.setup.page {
            Page::Games | Page::Files | Page::Failed(_) => &[("A", "SELECT"), ("B", "BACK")],
            Page::Installing => &[],
            _ => &[("A", "SELECT")],
        };
        footer(&p, c, hints);
    }
}

/// The games a mod plays, by their short names.
fn plays(m: &Mod) -> String {
    games().iter().filter(|g| g.options.contains(m)).map(|g| g.short.as_str()).collect::<Vec<_>>().join(" and ")
}

/// An installer run's steps: done ones ticked, the current one with its status, progress bar and QR code. `fold`:
/// only the last few, the ones before folded into one line, so it fits the screen.
#[allow(clippy::too_many_arguments)]
fn steps_rows(ctx: &egui::Context, steps: &[String], phase: &Phase, status: Option<String>, progress: Option<f64>,
              remaining: Option<String>, qr: Option<QrGrid>, fold: Option<usize>) -> Vec<Row<Id>> {
    let mut rows = vec![];
    let first = fold.map_or(0, |k| steps.len().saturating_sub(k));
    if first > 0 { rows.push(Row::item(Icon::Ok, FAINT, format!("{first} earlier steps done"))); }
    for (i, t) in steps.iter().enumerate().skip(first) {
        let last = i + 1 == steps.len();
        let current = last && *phase == Phase::Running;
        let (ic, color) = if current { (Icon::Working, CYAN) } else if last && matches!(phase, Phase::Failed(_)) { (Icon::Fail, BAD) } else { (Icon::Ok, OK) };
        rows.push(Row::Item { icon: ic, color, text: if current { format!("**{t}**") } else { t.clone() },
                              detail: if current { status.clone().unwrap_or_default() } else { String::new() }, note: String::new() });
        if current {
            if let Some(v) = progress { rows.push(progress_row(v, remaining.clone())); }
            if qr.is_some() { rows.push(qr_row(ctx, qr.clone())); }
        }
    }
    if steps.is_empty() && *phase == Phase::Running && fold.is_none() { rows.push(Row::item(Icon::Working, CYAN, "Starting…")); }
    rows
}

fn progress_row(v: f64, remaining: Option<String>) -> Row<Id> {
    Row::Custom { height: 40.0, draw: Box::new(move |p, r| {
        progress(p, Rect::from_min_size(r.min + Vec2::new(34.0, 6.0), Vec2::new(r.width() - 34.0, 5.0)), v);
        let t = format!("{}%{}", (v * 100.0) as i64, remaining.as_ref().map_or(String::new(), |s| format!("  ·  {s}")));
        text(p, r.min + Vec2::new(34.0, 18.0), Align2::LEFT_TOP, &t, body(14.0), DIM, 0.0);
    }) }
}

fn qr_row(ctx: &egui::Context, qr: Option<QrGrid>) -> Row<Id> {
    let ctx = ctx.clone();
    Row::Custom { height: 214.0, draw: Box::new(move |p, r| {
        qr_card(p, &ctx, Rect::from_min_size(Pos2::new(r.center().x - 280.0, r.top() + 6.0), Vec2::new(560.0, 200.0)), qr.as_ref());
    }) }
}

/// A question: its title, what it means, its buttons. Returns the button's act once chosen.
fn dialog(ui: &mut egui::Ui, c: Rect, d: &Dialog, nav: Vec<Nav>) -> Option<Act> {
    let p = ui.painter().clone();
    let w = 640.0;
    let msg_h = rich_height(&p, w - 100.0, &d.message, 16.0);
    let h = 70.0 + msg_h + 30.0 + d.buttons.len() as f32 * 52.0 + 30.0;
    let r = Rect::from_center_size(c.center(), Vec2::new(w, h));
    panel(&p, r);
    text(&p, Pos2::new(r.center().x, r.top() + 40.0), Align2::CENTER_CENTER, &d.title.to_uppercase(), menu(15.0), INK, 4.0);
    rich(&p, Pos2::new(r.center().x, r.top() + 70.0), w - 100.0, &d.message, 16.0, DIM, true);
    let rows: Vec<Row<Id>> = d.buttons.iter().enumerate().map(|(i, (label, _))| Row::action(label.clone(), true, Id::Button(i))).collect();
    let area = Rect::from_min_max(Pos2::new(r.left() + 40.0, r.top() + 70.0 + msg_h + 24.0), Pos2::new(r.right() - 40.0, r.bottom() - 16.0));
    let cancel = d.buttons.iter().position(|(_, a)| *a == Act::Close);
    if nav.contains(&Nav::Back) { if let Some(i) = cancel { sound::play(Sound::Back); return Some(d.buttons[i].1.clone()); } }
    let key = format!("dialog-{}", d.title);
    match list(ui, &key, area, &rows, &nav, None) {
        Some(Id::Button(i)) => Some(d.buttons[i].1.clone()),
        _ => None,
    }
}
