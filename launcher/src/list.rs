// SPDX-License-Identifier: GPL-3.0-or-later
// A screen's list: lines to read and things to choose, one above the other. Up and down (keys, D-pad, stick) move the
// bar between the things to choose, the mouse picks one by pointing, Return/A or a click chooses it. Long lists
// scroll to keep the chosen one in view. Menu sounds play here.
use crate::input::Nav;
use crate::ring::*;
use crate::sound::{self, Sound};
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Vec2};

pub type Draw = Box<dyn Fn(&egui::Painter, Rect)>;

/// Off for scripted test runs (DROPPOD_TEST_STEPS), so a mouse resting over the window can't move the bar.
pub static MOUSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn mouse_on() -> bool { MOUSE.load(std::sync::atomic::Ordering::Relaxed) }

pub enum Row<A> {
    /// A section's name.
    Header(String),
    /// Sentences, wrapped (with **bold** parts).
    Text { text: String, color: Color32, size: f32, center: bool },
    /// A line with an icon in front and a note at the right (a check's result, an item of a guided setup, a step).
    Item { icon: Icon, color: Color32, text: String, detail: String, note: String },
    /// A choice in a centred list (CONTINUE, BACK, PLAY).
    Action { label: String, enabled: bool, id: A },
    /// A row with a title, what it does, and its button (Settings).
    Tool { title: String, detail: String, button: String, enabled: bool, indent: bool, id: A },
    /// A tick box with a title, a line under it and a size at the right; `indent`: under the one before.
    Check { title: String, sub: String, right: String, on: bool, enabled: bool, indent: bool, id: A },
    /// One of a few choices, with what it means; `extra` rows show under it while it's the chosen one.
    Radio { title: String, detail: String, on: bool, id: A },
    /// Anything else, drawn by the screen into a rect this tall.
    Custom { height: f32, draw: Draw },
    Gap(f32),
}

impl<A> Row<A> {
    fn id(&self) -> Option<&A> {
        match self {
            Row::Action { id, .. } | Row::Tool { id, .. } | Row::Check { id, .. } | Row::Radio { id, .. } => Some(id),
            _ => None,
        }
    }
    fn enabled(&self) -> bool {
        match self {
            Row::Action { enabled, .. } | Row::Tool { enabled, .. } | Row::Check { enabled, .. } => *enabled,
            _ => true,
        }
    }
    pub fn text(text: impl Into<String>, color: Color32) -> Self { Row::Text { text: text.into(), color, size: 16.0, center: false } }
    pub fn note(text: impl Into<String>) -> Self { Row::Text { text: text.into(), color: DIM, size: 14.0, center: true } }
    pub fn action(label: impl Into<String>, enabled: bool, id: A) -> Self { Row::Action { label: label.into(), enabled, id } }
    pub fn item(icon: Icon, color: Color32, text: impl Into<String>) -> Self {
        Row::Item { icon, color, text: text.into(), detail: String::new(), note: String::new() }
    }
}

#[derive(Clone)]
struct State<A> { sel: Option<A>, scroll: f32, target: f32 }

impl<A> Default for State<A> { fn default() -> Self { State { sel: None, scroll: 0.0, target: 0.0 } } }

/// What the list called `key` has chosen now (its bar), if it has been shown.
pub fn selected<A: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: &str) -> Option<A> {
    ctx.data(|d| d.get_temp::<State<A>>(egui::Id::unique(("list", key)))).and_then(|s| s.sel)
}

const ACTION_H: f32 = 46.0;

fn height<A>(p: &egui::Painter, row: &Row<A>, w: f32) -> f32 {
    match row {
        Row::Header(_) => 40.0,
        Row::Text { text, size, .. } => rich_height(p, w, text, *size) + 8.0,
        Row::Item { text, detail, note, .. } => {
            let tw = w - 34.0 - if note.is_empty() { 0.0 } else { 110.0 };
            let mut h = rich_height(p, tw, text, 16.0).max(20.0);
            if !detail.is_empty() { h += 2.0 + rich_height(p, tw, detail, 14.0); }
            h + 12.0
        }
        Row::Action { .. } => ACTION_H + 6.0,
        Row::Tool { title, detail, indent, .. } => {
            let tw = w - 230.0 - if *indent { 42.0 } else { 0.0 };
            16.0 + strong_height(p, tw, title, 16.0) + 3.0 + rich_height(p, tw, detail, 14.0) + 14.0
        }
        Row::Check { title, sub, indent, .. } => {
            let tw = w - 150.0 - if *indent { 32.0 } else { 0.0 };
            14.0 + strong_height(p, tw, title, 16.0) + if sub.is_empty() { 0.0 } else { 2.0 + rich_height(p, tw, sub, 14.0) } + 14.0
        }
        Row::Radio { title, detail, .. } => 16.0 + strong_height(p, w - 60.0, title, 17.0) + 4.0 + rich_height(p, w - 60.0, detail, 14.0) + 16.0,
        Row::Custom { height, .. } => *height,
        Row::Gap(h) => *h,
    }
}

/// Draws the rows into `rect` and returns what was chosen this frame. `key` keeps each list's place; `prefer` is
/// where the bar starts (else the first thing that can be chosen).
pub fn list<A: Clone + PartialEq + Send + Sync + 'static>(ui: &egui::Ui, key: &str, rect: Rect, rows: &[Row<A>], nav: &[Nav],
                                                          prefer: Option<&A>) -> Option<A> {
    let ctx = ui.ctx().clone();
    let p = ui.painter().with_clip_rect(rect.expand2(Vec2::new(30.0, 4.0)));
    let w = rect.width();
    let id = egui::Id::unique(("list", key));
    let mut st: State<A> = ctx.data(|d| d.get_temp(id)).unwrap_or_default();

    // layout
    let mut tops = Vec::with_capacity(rows.len());
    let mut y = 0.0;
    for r in rows { tops.push(y); y += height(&p, r, w); }
    let content_h = y;
    let focusable: Vec<usize> = (0..rows.len()).filter(|i| rows[*i].id().is_some()).collect();
    let mut sel = st.sel.as_ref().and_then(|s| focusable.iter().position(|i| rows[*i].id() == Some(s)))
        .or_else(|| prefer.and_then(|pr| focusable.iter().position(|i| rows[*i].id() == Some(pr))))
        .or_else(|| focusable.iter().position(|i| rows[*i].enabled()))
        .unwrap_or(0);

    // moving and choosing
    let mut chosen = None;
    let n = focusable.len();
    let mut moved = false;
    for nv in nav {
        if n == 0 { break; }
        match nv {
            Nav::Up if n > 1 => { sel = (sel + n - 1) % n; moved = true; }
            Nav::Down if n > 1 => { sel = (sel + 1) % n; moved = true; }
            Nav::Accept => {
                let r = &rows[focusable[sel]];
                if r.enabled() { chosen = r.id().cloned(); sound::play(Sound::Select); } else { sound::play(Sound::Error); }
            }
            _ => {}
        }
    }
    if moved { sound::play(Sound::Move); }
    let (pointer, moving, clicked, wheel) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.delta() != Vec2::ZERO,
                                                          i.pointer.primary_clicked(), i.smooth_scroll_delta.y));
    let row_rect = |i: usize, scroll: f32| {
        let top = rect.top() + tops[i] - scroll;
        Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(w, height(&p, &rows[i], w)))
    };
    if let Some(pos) = pointer.filter(|p| rect.contains(*p) && mouse_on()) {
        if let Some(k) = focusable.iter().position(|i| hit(&rows[*i], row_rect(*i, st.scroll), w).contains(pos)) {
            if moving && k != sel { sel = k; sound::play(Sound::Move); }
            if clicked {
                sel = k;
                let r = &rows[focusable[k]];
                if r.enabled() { chosen = r.id().cloned(); sound::play(Sound::Select); } else { sound::play(Sound::Error); }
            }
        }
        if wheel != 0.0 { st.target -= wheel; }
    }

    // scrolling: keep the chosen one in view
    let max = (content_h - rect.height()).max(0.0);
    if n > 0 && (moved || st.sel.is_none() || chosen.is_some()) {
        let i = focusable[sel];
        let (top, h) = (tops[i], height(&p, &rows[i], w));
        // a heading or text just above the chosen one comes into view with it
        let lead = if i > 0 && !matches!(rows[i - 1], Row::Gap(_)) && rows[i - 1].id().is_none() { height(&p, &rows[i - 1], w) } else { 0.0 };
        if top - lead < st.target { st.target = top - lead - 8.0; }
        if top + h > st.target + rect.height() { st.target = top + h - rect.height() + 8.0; }
    }
    st.target = st.target.clamp(0.0, max);
    st.scroll = ctx.animate_value_with_time(id.with("scroll"), st.target, 0.15);

    // the bar, then the rows on it
    if n > 0 {
        let i = focusable[sel];
        let r = row_rect(i, st.scroll);
        let bh = ctx.animate_value_with_time(id.with("bh"), r.height(), 0.12);
        let by = ctx.animate_value_with_time(id.with("by"), r.top() + st.scroll, 0.12) - st.scroll;
        let alpha = if rows[i].enabled() { 1.0 } else { 0.45 };
        match &rows[i] {
            Row::Action { label, .. } => {
                let tw = text_width(&p, &label.to_uppercase(), menu(17.0), 4.5);
                bar(&p, Pos2::new(rect.center().x, by + bh / 2.0 - 3.0), Vec2::new(w.min(520.0), ACTION_H), tw, alpha)
            }
            _ => row_bar(&p, Rect::from_min_size(Pos2::new(rect.left() - 14.0, by), Vec2::new(w + 28.0, bh)), alpha),
        }
    }
    let selected = focusable.get(sel).copied();
    for (i, row) in rows.iter().enumerate() {
        let r = row_rect(i, st.scroll);
        if r.bottom() < rect.top() - 60.0 || r.top() > rect.bottom() + 60.0 { continue; }
        paint(&p, &ctx, row, r, Some(i) == selected);
    }
    if max > 0.0 {
        // where in the list this is
        let track = Rect::from_min_size(Pos2::new(rect.right() + 22.0, rect.top()), Vec2::new(2.0, rect.height()));
        p.rect_filled(track, 1.0, rgba(90, 140, 190, 40));
        let th = track.height() * rect.height() / content_h;
        let ty = track.top() + (track.height() - th) * st.scroll / max;
        p.rect_filled(Rect::from_min_size(Pos2::new(track.left(), ty), Vec2::new(2.0, th)), 1.0, rgba(120, 200, 255, 160));
    }

    st.sel = focusable.get(sel).and_then(|i| rows[*i].id().cloned());
    ctx.data_mut(|d| d.insert_temp(id, st));
    chosen
}

/// Where a row answers the mouse (a centred choice: only around its words).
fn hit<A>(row: &Row<A>, r: Rect, w: f32) -> Rect {
    match row {
        Row::Action { .. } => Rect::from_center_size(r.center() - Vec2::new(0.0, 3.0), Vec2::new(w.min(520.0), ACTION_H)),
        _ => r,
    }
}

fn paint<A>(p: &egui::Painter, ctx: &egui::Context, row: &Row<A>, r: Rect, selected: bool) {
    let ink = |enabled: bool| if !enabled { FAINT } else if selected { Color32::WHITE } else { INK };
    match row {
        Row::Header(t) => {
            text(p, Pos2::new(r.left(), r.bottom() - 14.0), Align2::LEFT_CENTER, &t.to_uppercase(), menu(12.0), CYAN, 4.0);
            p.line_segment([Pos2::new(r.left(), r.bottom() - 3.0), Pos2::new(r.right(), r.bottom() - 3.0)],
                           egui::Stroke::new(1.0, rgba(90, 160, 220, 50)));
        }
        Row::Text { text, color, size, center } => {
            let pos = if *center { Pos2::new(r.center().x, r.top() + 2.0) } else { r.min + Vec2::new(0.0, 2.0) };
            rich(p, pos, r.width(), text, *size, *color, *center);
        }
        Row::Item { icon: ic, color, text: t, detail, note } => {
            icon(p, ctx, Pos2::new(r.left() + 10.0, r.top() + 15.0), *ic, *color);
            let tw = r.width() - 34.0 - if note.is_empty() { 0.0 } else { 110.0 };
            let tr = rich(p, Pos2::new(r.left() + 34.0, r.top() + 4.0), tw, t, 16.0, INK, false);
            if !detail.is_empty() { rich(p, Pos2::new(r.left() + 34.0, tr.bottom() + 2.0), tw, detail, 14.0, DIM, false); }
            if !note.is_empty() {
                text(p, Pos2::new(r.right(), r.top() + 15.0), Align2::RIGHT_CENTER, note, body(15.0), DIM, 0.0);
            }
        }
        Row::Action { label, enabled, .. } => {
            text(p, Pos2::new(r.center().x, r.center().y - 3.0), Align2::CENTER_CENTER, &label.to_uppercase(), menu(17.0),
                 if !*enabled { FAINT } else if selected { Color32::WHITE } else { DIM }, 4.5);
        }
        Row::Tool { title, detail, button, enabled, indent, .. } => {
            let x = if *indent { 42.0 } else { 0.0 };
            let tw = r.width() - 230.0 - x;
            let tr = strong(p, r.min + Vec2::new(x, 16.0), tw, title, 16.0, ink(*enabled));
            rich(p, Pos2::new(r.left() + x, tr.bottom() + 3.0), tw, detail, 14.0, if *enabled { DIM } else { FAINT }, false);
            let bw = text_width(p, &button.to_uppercase(), menu(11.0), 2.5).max(170.0) + 30.0;
            let b = Rect::from_min_size(Pos2::new(r.right() - bw, r.top() + 14.0), Vec2::new(bw, 30.0));
            let edge = if !*enabled { rgba(80, 110, 140, 90) } else if selected { rgba(150, 225, 255, 255) } else { rgba(90, 170, 230, 150) };
            p.rect_stroke(b, 2.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
            if selected && *enabled { p.rect_filled(b, 2.0, rgba(70, 180, 255, 45)); }
            text(p, b.center(), Align2::CENTER_CENTER, &button.to_uppercase(), menu(11.0), ink(*enabled), 2.5);
        }
        Row::Check { title, sub, right, on, enabled, indent, .. } => {
            let x = r.left() + if *indent { 32.0 } else { 0.0 } + 4.0;
            let bx = Rect::from_min_size(Pos2::new(x, r.top() + 14.0), Vec2::splat(18.0));
            let edge = if *enabled { if selected { rgba(170, 230, 255, 255) } else { rgba(110, 190, 240, 200) } } else { rgba(80, 110, 140, 120) };
            p.rect_stroke(bx, 2.0, egui::Stroke::new(1.4, edge), egui::StrokeKind::Inside);
            if *on {
                p.rect_filled(bx.shrink(3.5), 1.0, if *enabled { CYAN } else { FAINT });
            }
            let tw = r.width() - 150.0 - if *indent { 32.0 } else { 0.0 };
            let tr = strong(p, Pos2::new(x + 32.0, r.top() + 12.0), tw, title, 16.0, ink(*enabled));
            if !sub.is_empty() { rich(p, Pos2::new(x + 32.0, tr.bottom() + 2.0), tw, sub, 14.0, if *enabled { DIM } else { FAINT }, false); }
            text(p, Pos2::new(r.right() - 4.0, r.top() + 23.0), Align2::RIGHT_CENTER, right, body(16.0), if *enabled { DIM } else { FAINT }, 0.0);
        }
        Row::Radio { title, detail, on, .. } => {
            let c = Pos2::new(r.left() + 14.0, r.top() + 27.0);
            p.circle_stroke(c, 9.0, egui::Stroke::new(1.4, if selected { rgba(170, 230, 255, 255) } else { rgba(110, 190, 240, 200) }));
            if *on { p.circle_filled(c, 4.5, CYAN); }
            let tr = strong(p, Pos2::new(r.left() + 42.0, r.top() + 16.0), r.width() - 60.0, title, 17.0, ink(true));
            rich(p, Pos2::new(r.left() + 42.0, tr.bottom() + 4.0), r.width() - 60.0, detail, 14.0, DIM, false);
        }
        Row::Custom { draw, .. } => draw(p, r),
        Row::Gap(_) => {}
    }
}
