// SPDX-License-Identifier: GPL-3.0-or-later
// The Ring look (Halo 3's console menus): navy and cyan, the art blurred behind, a soft bar that slides to what's
// chosen. The pieces every screen uses: backdrop, title, panel, bars, progress, icons, wrapped text, the QR card.
use crate::core::QrGrid;
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Vec2};

pub const INK: Color32 = Color32::from_rgb(232, 244, 255);
pub const DIM: Color32 = Color32::from_rgb(140, 170, 200);
pub const FAINT: Color32 = Color32::from_rgb(88, 112, 138);
pub const CYAN: Color32 = Color32::from_rgb(70, 190, 255);
pub const OK: Color32 = Color32::from_rgb(96, 222, 150);
pub const WARN: Color32 = Color32::from_rgb(255, 178, 84);
pub const BAD: Color32 = Color32::from_rgb(255, 104, 104);
pub const NAVY: Color32 = Color32::from_rgb(3, 9, 20);

/// The design is laid out on this canvas, scaled to the window.
pub const CANVAS: Vec2 = Vec2::new(1280.0, 720.0);

/// Behind everything: a blurred picture (cross-fading from the one before), darkened, scanlines, a vignette. With no
/// picture (no game files yet), a deep blue glow.
pub fn backdrop(p: &egui::Painter, r: Rect, art: Option<&egui::TextureHandle>, before: Option<(&egui::TextureHandle, f32)>) {
    p.rect_filled(r, 0.0, NAVY);
    // the glow that's there without art, and under it while it fades in
    let glow = Rect::from_center_size(r.center() - Vec2::new(r.width() * 0.18, r.height() * 0.08), r.size() * 1.1);
    radial(p, glow.center(), glow.width() * 0.45, rgba(30, 90, 170, 70));
    radial(p, r.center() + Vec2::new(r.width() * 0.3, r.height() * 0.2), r.width() * 0.3, rgba(20, 70, 140, 50));
    if let Some((t, a)) = before { cover(p, r, t, Color32::from_gray(200).gamma_multiply(a)); }
    if let Some(t) = art {
        let a = before.map_or(1.0, |(_, a)| 1.0 - a);
        cover(p, r, t, Color32::from_gray(200).gamma_multiply(a.max(0.0)));
    }
    p.rect_filled(r, 0.0, rgba(2, 10, 24, 120));
    vgrad(p, r, &[rgba(2, 8, 18, 230), rgba(2, 8, 18, 40), rgba(2, 8, 18, 60), rgba(2, 8, 18, 245)]);
    scanlines(p, r, 3.0, rgba(0, 0, 0, 40));
    vignette(p, r, Vec2::new(r.width() * 0.2, r.height() * 0.22), rgba(0, 0, 0, 170));
}

/// A soft round glow: a fan of triangles from a coloured centre to clear edges.
pub fn radial(p: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, color);
    let n = 48;
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        m.colored_vertex(c + Vec2::new(a.cos(), a.sin()) * radius, Color32::TRANSPARENT);
    }
    for i in 1..=n as u32 { m.add_triangle(0, i, i + 1); }
    p.add(egui::Shape::mesh(m));
}

/// The page's title, centred at the top, with its fading line and a line under it.
pub fn title(p: &egui::Painter, c: Rect, title: &str, subtitle: &str) {
    let cx = c.center().x;
    text(p, Pos2::new(cx, c.top() + 58.0), Align2::CENTER_CENTER, title, menu(22.0), INK, 8.0);
    fade_line(p, Pos2::new(cx, c.top() + 86.0), 380.0, rgba(110, 205, 255, 160));
    if !subtitle.is_empty() {
        rich(p, Pos2::new(cx, c.top() + 98.0), 900.0, subtitle, 15.0, DIM, true);
    }
}

/// The selection bar of a centred list: a cyan glow fading out at both ends, bright hairlines, two small diamonds.
/// `text_w`: how wide the words on it are (the diamonds stay clear of them).
pub fn bar(p: &egui::Painter, center: Pos2, size: Vec2, text_w: f32, alpha: f32) {
    let size = Vec2::new(size.x.max(text_w + 140.0), size.y);
    let bar = Rect::from_center_size(center, size);
    let a = |v: f32| (v * alpha) as u8;
    hgrad(p, bar, &[Color32::TRANSPARENT, rgba(60, 170, 255, a(70.0)), rgba(90, 200, 255, a(95.0)), rgba(60, 170, 255, a(70.0)), Color32::TRANSPARENT]);
    for y in [bar.top(), bar.bottom() - 1.0] {
        hgrad(p, Rect::from_min_size(Pos2::new(bar.left(), y), Vec2::new(bar.width(), 1.0)),
              &[Color32::TRANSPARENT, rgba(120, 215, 255, a(230.0)), Color32::TRANSPARENT]);
    }
    for dx in [-1.0f32, 1.0] {
        diamond(p, Pos2::new(center.x + dx * (size.x * 0.36).max(text_w / 2.0 + 26.0), center.y), 4.0, rgba(150, 225, 255, a(255.0)));
    }
}

/// The selection bar of a list of rows: brightest at the left edge, fading out to the right.
pub fn row_bar(p: &egui::Painter, r: Rect, alpha: f32) {
    let a = |v: f32| (v * alpha) as u8;
    hgrad(p, r, &[rgba(70, 180, 255, a(80.0)), rgba(60, 170, 255, a(45.0)), rgba(60, 170, 255, a(12.0))]);
    p.rect_filled(Rect::from_min_size(r.min, Vec2::new(2.0, r.height())), 0.0, rgba(140, 220, 255, a(255.0)));
    for y in [r.top(), r.bottom() - 1.0] {
        hgrad(p, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 1.0)),
              &[rgba(120, 215, 255, a(200.0)), rgba(120, 215, 255, a(60.0)), Color32::TRANSPARENT]);
    }
}

pub fn diamond(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    p.add(egui::Shape::convex_polygon(
        vec![c + Vec2::new(0.0, -s), c + Vec2::new(s, 0.0), c + Vec2::new(0.0, s), c + Vec2::new(-s, 0.0)], color, egui::Stroke::NONE));
}

/// A dark glass panel with hairline edges.
pub fn panel(p: &egui::Painter, r: Rect) {
    p.add(egui::Shadow { offset: [0, 8], blur: 40, spread: 0, color: rgba(0, 0, 0, 120) }.as_shape(r, 4.0));
    p.rect_filled(r, 4.0, rgba(4, 14, 30, 250));
    p.rect_stroke(r, 4.0, egui::Stroke::new(1.0, rgba(90, 170, 230, 70)), egui::StrokeKind::Inside);
    fade_line(p, Pos2::new(r.center().x, r.top()), r.width(), rgba(130, 215, 255, 200));
}

/// A progress bar: a dim track, a cyan fill with a bright tip.
pub fn progress(p: &egui::Painter, r: Rect, fraction: f64) {
    p.rect_filled(r, 2.0, rgba(70, 120, 170, 60));
    let f = fraction.clamp(0.0, 1.0) as f32;
    let fill = Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height()));
    hgrad(p, fill, &[rgba(40, 130, 220, 255), rgba(90, 205, 255, 255)]);
    if f > 0.0 {
        p.add(egui::Shadow { offset: [0, 0], blur: 10, spread: 0, color: rgba(80, 200, 255, 90) }.as_shape(fill, 2.0));
    }
}

/// "Working": three dots lighting in turn (a few times a second, so a long download costs next to nothing).
pub fn dots(p: &egui::Painter, ctx: &egui::Context, left_center: Pos2, color: Color32) {
    let t = ctx.input(|i| i.time);
    let lit = ((t * 3.0) as usize) % 3;
    for i in 0..3 {
        let c = if i == lit { color } else { color.gamma_multiply(0.3) };
        diamond(p, left_center + Vec2::new(4.0 + i as f32 * 11.0, 0.0), 3.5, c);
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(1.0 / 3.0 - (t % (1.0 / 3.0))));
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon { Ok, Fail, Info, Warn, Pending, Bullet, Working }

/// A small icon centred at `c`, about 18 points across.
pub fn icon(p: &egui::Painter, ctx: &egui::Context, c: Pos2, icon: Icon, color: Color32) {
    let s = egui::Stroke::new(1.8, color);
    match icon {
        Icon::Ok => {
            p.circle_stroke(c, 8.5, egui::Stroke::new(1.4, color));
            p.line_segment([c + Vec2::new(-4.0, 0.2), c + Vec2::new(-1.2, 3.2)], s);
            p.line_segment([c + Vec2::new(-1.2, 3.2), c + Vec2::new(4.5, -3.2)], s);
        }
        Icon::Fail => {
            p.circle_stroke(c, 8.5, egui::Stroke::new(1.4, color));
            p.line_segment([c + Vec2::new(-3.5, -3.5), c + Vec2::new(3.5, 3.5)], s);
            p.line_segment([c + Vec2::new(3.5, -3.5), c + Vec2::new(-3.5, 3.5)], s);
        }
        Icon::Info => {
            p.circle_stroke(c, 8.5, egui::Stroke::new(1.4, color));
            text(p, c + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, "i", bold(13.0), color, 0.0);
        }
        Icon::Warn => {
            let pts = vec![c + Vec2::new(0.0, -8.5), c + Vec2::new(8.5, 7.0), c + Vec2::new(-8.5, 7.0)];
            p.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.4, color)));
            text(p, c + Vec2::new(0.0, 1.5), Align2::CENTER_CENTER, "!", bold(12.0), color, 0.0);
        }
        Icon::Pending => { p.circle_stroke(c, 7.0, egui::Stroke::new(1.2, color.gamma_multiply(0.6))); }
        Icon::Bullet => diamond(p, c, 4.0, color),
        Icon::Working => dots(p, ctx, c - Vec2::new(13.0, 0.0), color),
    }
}

/// Text in sentences, wrapped to `width`, with **bold** parts. `center`: centred on `pos.x`, else from `pos` left.
/// Returns the rect it took.
pub fn rich(p: &egui::Painter, pos: Pos2, width: f32, s: &str, size: f32, color: Color32, center: bool) -> Rect {
    let galley = p.layout_job(rich_job(s, size, color, width, center, false));
    let rect = galley.rect.translate(pos.to_vec2());
    p.galley(pos, galley, color);
    rect
}

/// `rich`, all in the semibold face (titles).
pub fn strong(p: &egui::Painter, pos: Pos2, width: f32, s: &str, size: f32, color: Color32) -> Rect {
    let galley = p.layout_job(rich_job(s, size, color, width, false, true));
    let rect = galley.rect.translate(pos.to_vec2());
    p.galley(pos, galley, color);
    rect
}

/// How tall `rich` text would be.
pub fn rich_height(p: &egui::Painter, width: f32, s: &str, size: f32) -> f32 {
    p.layout_job(rich_job(s, size, INK, width, false, false)).size().y
}

pub fn strong_height(p: &egui::Painter, width: f32, s: &str, size: f32) -> f32 {
    p.layout_job(rich_job(s, size, INK, width, false, true)).size().y
}

fn rich_job(s: &str, size: f32, color: Color32, width: f32, center: bool, all_bold: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (i, part) in s.split("**").enumerate() {
        if part.is_empty() { continue; }
        let (font, c) = if all_bold { (bold(size), color) } else if i % 2 == 1 { (bold(size), INK) } else { (body(size), color) };
        job.append(part, 0.0, egui::TextFormat { font_id: font, color: c, line_height: Some(size * 1.32), ..Default::default() });
    }
    job.wrap.max_width = width;
    if center { job.halign = egui::Align::Center; }
    job
}

/// Single-line text cut to `width` with "…" in the middle (paths).
pub fn middle_cut(p: &egui::Painter, s: &str, font: egui::FontId, width: f32) -> String {
    let fits = |t: &str| p.layout_no_wrap(t.to_string(), font.clone(), INK).size().x <= width;
    if fits(s) { return s.to_string(); }
    let chars: Vec<char> = s.chars().collect();
    let mut keep = chars.len();
    while keep > 4 {
        keep -= 1;
        let (a, b) = (keep / 2, keep - keep / 2);
        let t: String = chars[..a].iter().chain(std::iter::once(&'…')).chain(&chars[chars.len() - b..]).collect();
        if fits(&t) { return t; }
    }
    "…".into()
}

/// The Steam QR code with how to scan it (setup's download, a tile's Get, Repair game files). 520 × 196.
pub fn qr_card(p: &egui::Painter, ctx: &egui::Context, r: Rect, qr: Option<&QrGrid>) {
    p.rect_filled(r, 4.0, rgba(6, 22, 44, 230));
    p.rect_stroke(r, 4.0, egui::Stroke::new(1.0, rgba(90, 170, 230, 90)), egui::StrokeKind::Inside);
    let code = Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(r.height() - 28.0));
    p.rect_filled(code, 3.0, Color32::WHITE);
    match qr {
        Some(g) => {
            let inner = code.shrink(8.0);
            let cell = inner.width() / g.size() as f32;
            let mut m = egui::Mesh::default();
            for (y, row) in g.modules.iter().enumerate() {
                for (x, dark) in row.iter().enumerate() {
                    if *dark {
                        m.add_colored_rect(Rect::from_min_size(inner.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                                                               Vec2::splat(cell + 0.3)), Color32::BLACK);
                    }
                }
            }
            p.add(egui::Shape::mesh(m));
        }
        None => {
            dots(p, ctx, code.center() - Vec2::new(16.0, 8.0), rgba(20, 60, 110, 255));
            text(p, code.center() + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "Getting a code…", body(13.0), rgba(30, 50, 80, 255), 0.0);
        }
    }
    let x = code.right() + 22.0;
    let w = r.right() - x - 14.0;
    let mut y = r.top() + 16.0;
    text(p, Pos2::new(x, y), Align2::LEFT_TOP, "SCAN WITH YOUR PHONE", menu(13.0), INK, 3.0);
    y += 30.0;
    for (n, s) in [(1, "Open the **Steam** app on your phone."), (2, "Tap the **shield** (Steam Guard) at the bottom."),
                   (3, "Tap **Scan a QR code** and point your phone at this code.")] {
        let c = Pos2::new(x + 9.0, y + 10.0);
        p.circle_stroke(c, 9.0, egui::Stroke::new(1.3, CYAN));
        text(p, c + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, &n.to_string(), bold(12.0), CYAN, 0.0);
        y = rich(p, Pos2::new(x + 26.0, y), w - 26.0, s, 15.0, INK, false).bottom() + 8.0;
    }
    rich(p, Pos2::new(x, y + 2.0), w, "This moves on by itself once you approve. If the code runs out, a new one appears.", 13.0, DIM, false);
}

/// Button hints along the bottom right, and anything to say along the bottom left.
pub fn footer(p: &egui::Painter, c: Rect, hints: &[(&str, &str)]) {
    let widths: Vec<f32> = hints.iter().map(|(_, w)| p.layout_no_wrap(w.to_string(), bold(14.0), INK).size().x + 1.5 * w.len() as f32 + 54.0).collect();
    let mut x = c.right() - 56.0 - widths.iter().sum::<f32>();
    for (b, w) in hints { x = hint(p, Pos2::new(x, c.bottom() - 36.0), b, w, CYAN, INK); }
}

/// A status line at the bottom left: a coloured dot, a name, what it says.
pub fn status_line(p: &egui::Painter, pos: Pos2, color: Color32, name: &str, what: &str, max_w: f32) -> f32 {
    p.circle_filled(pos + Vec2::new(4.0, 0.0), 4.0, color);
    let mut x = pos.x + 16.0;
    if !name.is_empty() {
        x = text(p, Pos2::new(x, pos.y), Align2::LEFT_CENTER, name, bold(14.0), INK, 1.5).right() + 10.0;
    }
    let w = max_w - (x - pos.x);
    let s = middle_cut(p, what, body(15.0), w.max(40.0));
    text(p, Pos2::new(x, pos.y), Align2::LEFT_CENTER, &s, body(15.0), DIM, 0.0).right()
}

/// A dark veil over a screen, for a sheet or a question on top.
pub fn veil(p: &egui::Painter, r: Rect, alpha: f32) {
    p.rect_filled(r, 0.0, rgba(1, 4, 10, (205.0 * alpha) as u8));
}
