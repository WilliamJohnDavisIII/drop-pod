// SPDX-License-Identifier: GPL-3.0-or-later
// Moving through the menus: the arrow keys, Return/Space and Escape, or a game controller (its D-pad or left stick,
// A to choose, B to go back). The controller is read on a thread of its own that sleeps until something happens; a
// held direction repeats. Only while this window is in front, so a game being played never moves the menus.
use crate::proc::Waker;
use eframe::egui;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav { Up, Down, Left, Right, Accept, Back }

enum Pad { Press(Nav), Release(Nav) }

pub struct Input {
    pad: Option<Receiver<Pad>>,
    held: Option<(Nav, f64)>, // a held controller direction, and when it repeats next
}

impl Input {
    pub fn new(waker: Option<Waker>) -> Self {
        Input { pad: waker.map(start_pad_thread), held: None }
    }

    /// This frame's moves.
    pub fn frame(&mut self, ctx: &egui::Context) -> Vec<Nav> {
        let mut out = Vec::new();
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        ctx.input(|i| {
            for (key, nav) in [(egui::Key::ArrowUp, Nav::Up), (egui::Key::ArrowDown, Nav::Down),
                               (egui::Key::ArrowLeft, Nav::Left), (egui::Key::ArrowRight, Nav::Right),
                               (egui::Key::Enter, Nav::Accept), (egui::Key::Space, Nav::Accept),
                               (egui::Key::Escape, Nav::Back), (egui::Key::Backspace, Nav::Back)] {
                if i.key_pressed(key) { out.push(nav); }
            }
        });
        let now = ctx.input(|i| i.time);
        if let Some(rx) = &self.pad {
            for m in rx.try_iter() {
                if !focused { continue; }
                match m {
                    Pad::Press(n) => {
                        out.push(n);
                        if matches!(n, Nav::Up | Nav::Down | Nav::Left | Nav::Right) { self.held = Some((n, now + 0.42)); }
                    }
                    Pad::Release(n) => if self.held.is_some_and(|(h, _)| h == n) { self.held = None },
                }
            }
        }
        if !focused { self.held = None; }
        if let Some((n, next)) = self.held {
            if now >= next {
                out.push(n);
                self.held = Some((n, now + 0.11));
            }
            let wait = self.held.map_or(0.1, |(_, t)| t - now).max(0.0);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        out
    }
}

fn start_pad_thread(waker: Waker) -> Receiver<Pad> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let Ok(mut gilrs) = gilrs::Gilrs::new() else { return };
        let mut stick: Option<Nav> = None;
        let (mut x, mut y) = (0.0f32, 0.0f32);
        loop {
            let Some(ev) = gilrs.next_event_blocking(None) else { continue };
            use gilrs::{Axis, Button, EventType};
            let button = |b: Button| match b {
                Button::DPadUp => Some(Nav::Up), Button::DPadDown => Some(Nav::Down),
                Button::DPadLeft => Some(Nav::Left), Button::DPadRight => Some(Nav::Right),
                Button::South | Button::Start => Some(Nav::Accept), Button::East | Button::Select => Some(Nav::Back),
                _ => None,
            };
            let send = |m: Pad| { let _ = tx.send(m); waker(); };
            match ev.event {
                EventType::ButtonPressed(b, _) => if let Some(n) = button(b) { send(Pad::Press(n)) },
                EventType::ButtonReleased(b, _) => if let Some(n) = button(b) { send(Pad::Release(n)) },
                EventType::AxisChanged(axis, v, _) => {
                    match axis { Axis::LeftStickX => x = v, Axis::LeftStickY => y = v, _ => continue }
                    // a direction once the stick is well over; back to none near the middle
                    let dir = if y.abs() > 0.6 && y.abs() >= x.abs() { Some(if y > 0.0 { Nav::Up } else { Nav::Down }) }
                              else if x.abs() > 0.6 { Some(if x > 0.0 { Nav::Right } else { Nav::Left }) }
                              else if x.abs() < 0.3 && y.abs() < 0.3 { None } else { stick };
                    if dir != stick {
                        if let Some(old) = stick { send(Pad::Release(old)); }
                        if let Some(new) = dir { send(Pad::Press(new)); }
                        stick = dir;
                    }
                }
                _ => {}
            }
        }
    });
    rx
}
