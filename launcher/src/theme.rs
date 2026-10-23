// SPDX-License-Identifier: GPL-3.0-or-later
// The look: fonts (Michroma for menus and titles, Titillium Web for sentences, both OFL), colours, and the painting
// helpers the screens share (gradients, art that covers a rect, scanlines, vignette, chamfered panels).
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use std::sync::Arc;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::empty();
    let add = |fonts: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add(&mut fonts, "michroma", include_bytes!("../assets/fonts/Michroma-Regular.ttf"));
    add(&mut fonts, "titillium", include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf"));
    add(&mut fonts, "titillium-semibold", include_bytes!("../assets/fonts/TitilliumWeb-SemiBold.ttf"));
    fonts.families.insert(FontFamily::Proportional, vec!["titillium".into()]);
    fonts.families.insert(FontFamily::Monospace, vec!["titillium".into()]);
    fonts.families.insert(FontFamily::Name("menu".into()), vec!["michroma".into(), "titillium".into()]);
    fonts.families.insert(FontFamily::Name("bold".into()), vec!["titillium-semibold".into()]);
    ctx.set_fonts(fonts);
}

pub fn menu(size: f32) -> FontId { FontId::new(size, FontFamily::Name("menu".into())) }
pub fn body(size: f32) -> FontId { FontId::new(size, FontFamily::Proportional) }
pub fn bold(size: f32) -> FontId { FontId::new(size, FontFamily::Name("bold".into())) }

pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 { Color32::from_rgba_unmultiplied(r, g, b, a) }

/// Text with letter spacing, placed by `align` (egui's Align2) at `pos`. Returns its rect. Centred, it's the ink
/// that's centred: egui centres the line box, and Michroma's capitals sit low in theirs (measured: about 7% of the
/// size; Titillium's about 2%).
pub fn text(p: &egui::Painter, pos: Pos2, align: egui::Align2, s: &str, font: FontId, color: Color32, spacing: f32) -> Rect {
    let lift = if align.y() == egui::Align::Center {
        font.size * if font.family == FontFamily::Name("menu".into()) { 0.074 } else { 0.022 }
    } else { 0.0 };
    let job = egui::text::LayoutJob::single_section(
        s.to_owned(),
        egui::TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() },
    );
    let galley = p.layout_job(job);
    let rect = align.anchor_size(pos - Vec2::new(0.0, lift), galley.size());
    p.galley(rect.min, galley, color);
    rect
}

/// How wide `text` would be.
pub fn text_width(p: &egui::Painter, s: &str, font: FontId, spacing: f32) -> f32 {
    let job = egui::text::LayoutJob::single_section(
        s.to_owned(), egui::TextFormat { font_id: font, extra_letter_spacing: spacing, ..Default::default() });
    p.layout_job(job).size().x
}

/// Text with a soft glow around it: the same text a few times, offset, faint.
pub fn glow_text(p: &egui::Painter, pos: Pos2, align: egui::Align2, s: &str, font: FontId, color: Color32,
                 glow: Color32, spacing: f32) -> Rect {
    for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5), (-3.0, 0.0), (3.0, 0.0)] {
        text(p, pos + Vec2::new(dx, dy), align, s, font.clone(), glow, spacing);
    }
    text(p, pos, align, s, font, color, spacing)
}

/// A rect filled left to right through `stops` (evenly spaced).
pub fn hgrad(p: &egui::Painter, r: Rect, stops: &[Color32]) {
    let mut m = egui::Mesh::default();
    let n = stops.len() - 1;
    for (i, c) in stops.iter().enumerate() {
        let x = r.left() + r.width() * i as f32 / n as f32;
        m.colored_vertex(Pos2::new(x, r.top()), *c);
        m.colored_vertex(Pos2::new(x, r.bottom()), *c);
    }
    for i in 0..n as u32 {
        m.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
        m.add_triangle(2 * i + 1, 2 * i + 3, 2 * i + 2);
    }
    p.add(egui::Shape::mesh(m));
}

/// A rect filled top to bottom through `stops` (evenly spaced).
pub fn vgrad(p: &egui::Painter, r: Rect, stops: &[Color32]) {
    let mut m = egui::Mesh::default();
    let n = stops.len() - 1;
    for (i, c) in stops.iter().enumerate() {
        let y = r.top() + r.height() * i as f32 / n as f32;
        m.colored_vertex(Pos2::new(r.left(), y), *c);
        m.colored_vertex(Pos2::new(r.right(), y), *c);
    }
    for i in 0..n as u32 {
        m.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
        m.add_triangle(2 * i + 1, 2 * i + 3, 2 * i + 2);
    }
    p.add(egui::Shape::mesh(m));
}

/// A picture scaled to cover `r` (cropped, never stretched), tinted.
pub fn cover(p: &egui::Painter, r: Rect, tex: &egui::TextureHandle, tint: Color32) {
    let [w, h] = tex.size();
    let (img, area) = (w as f32 / h as f32, r.width() / r.height());
    let uv = if img > area {
        let f = area / img;
        Rect::from_min_max(Pos2::new(0.5 - f / 2.0, 0.0), Pos2::new(0.5 + f / 2.0, 1.0))
    } else {
        let f = img / area;
        Rect::from_min_max(Pos2::new(0.0, 0.5 - f / 2.0), Pos2::new(1.0, 0.5 + f / 2.0))
    };
    p.image(tex.id(), r, uv, tint);
}

/// Faint horizontal lines every `gap` points, like an old screen.
pub fn scanlines(p: &egui::Painter, r: Rect, gap: f32, color: Color32) {
    let mut m = egui::Mesh::default();
    let mut y = r.top();
    while y < r.bottom() {
        m.add_colored_rect(Rect::from_min_max(Pos2::new(r.left(), y), Pos2::new(r.right(), y + 1.0)), color);
        y += gap;
    }
    p.add(egui::Shape::mesh(m));
}

/// Darker towards the edges: a clear inner rect fading to `edge` at the outer one.
pub fn vignette(p: &egui::Painter, r: Rect, inset: Vec2, edge: Color32) {
    let inner = r.shrink2(inset);
    let mut m = egui::Mesh::default();
    let outer = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    let inn = [inner.left_top(), inner.right_top(), inner.right_bottom(), inner.left_bottom()];
    for i in 0..4 {
        m.colored_vertex(outer[i], edge);
        m.colored_vertex(inn[i], Color32::TRANSPARENT);
    }
    for i in 0..4u32 {
        let (o0, i0, o1, i1) = (2 * i, 2 * i + 1, (2 * i + 2) % 8, (2 * i + 3) % 8);
        m.add_triangle(o0, i0, o1);
        m.add_triangle(i0, i1, o1);
    }
    p.add(egui::Shape::mesh(m));
}

/// A controller-style button glyph (a ring with its letter) and what it does, left to right from `pos`; returns the
/// x after it.
pub fn hint(p: &egui::Painter, pos: Pos2, button: &str, what: &str, ring: Color32, label: Color32) -> f32 {
    let c = Pos2::new(pos.x + 11.0, pos.y);
    p.circle_stroke(c, 10.0, egui::Stroke::new(1.5, ring));
    // a direction pad: two small triangles, across or up and down
    let arrows: &[(f32, f32)] = match button { "<>" => &[(-1.0, 0.0), (1.0, 0.0)], "^v" => &[(0.0, -1.0), (0.0, 1.0)], _ => &[] };
    for &(dx, dy) in arrows {
        let tip = c + Vec2::new(dx * 7.0, dy * 7.0);
        let back = c + Vec2::new(dx * 2.5, dy * 2.5);
        let side = Vec2::new(dy * 3.5, dx * 3.5);
        p.add(egui::Shape::convex_polygon(vec![tip, back + side, back - side], ring, egui::Stroke::NONE));
    }
    if arrows.is_empty() {
        text(p, c + Vec2::new(0.0, 0.5), egui::Align2::CENTER_CENTER, button, bold(13.0), ring, 0.0);
    }
    let r = text(p, Pos2::new(pos.x + 28.0, pos.y), egui::Align2::LEFT_CENTER, what, bold(14.0), label, 1.5);
    r.right() + 26.0
}

/// A horizontal line that fades out at both ends.
pub fn fade_line(p: &egui::Painter, center: Pos2, width: f32, color: Color32) {
    let r = Rect::from_center_size(center, Vec2::new(width, 1.0));
    hgrad(p, r, &[Color32::TRANSPARENT, color, Color32::TRANSPARENT]);
}
