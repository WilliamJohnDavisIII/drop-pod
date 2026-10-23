// SPDX-License-Identifier: GPL-3.0-or-later
// Short menu sounds, made here from sine waves (no recorded audio in the app). The sound device opens on the first
// sound and closes again after a few quiet seconds, so an idle window keeps the Mac's audio asleep.
use std::num::NonZero;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound { Move, Select, Back, Error, Launch, Attention }

const RATE: u32 = 44_100;

static SENDER: OnceLock<Option<Sender<Sound>>> = OnceLock::new();

/// Plays a sound (on its own thread; never waits). DROPPOD_SOUNDS=0 turns them off.
pub fn play(s: Sound) {
    let tx = SENDER.get_or_init(|| {
        if crate::paths::var("DROPPOD_SOUNDS").as_deref() == Some("0") { return None; }
        let (tx, rx) = channel::<Sound>();
        std::thread::spawn(move || {
            let mut sink: Option<rodio::MixerDeviceSink> = None;
            loop {
                let next = if sink.is_some() { rx.recv_timeout(Duration::from_secs(4)) } else { rx.recv().map_err(|_| RecvTimeoutError::Disconnected) };
                match next {
                    Ok(s) => {
                        if sink.is_none() {
                            sink = rodio::DeviceSinkBuilder::open_default_sink().ok();
                            if let Some(k) = sink.as_mut() { k.log_on_drop(false); }
                        }
                        if let Some(k) = &sink {
                            k.mixer().add(rodio::buffer::SamplesBuffer::new(NonZero::new(1).unwrap(), NonZero::new(RATE).unwrap(), samples(s)));
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => sink = None, // (quiet for a while: let the device sleep)
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        Some(tx)
    });
    if let Some(tx) = tx { let _ = tx.send(s); }
}

/// A tone sweeping from `f0` to `f1` Hz over `ms`, with a quick attack and an exponential fade, plus a little of its
/// third harmonic for edge.
fn tone(out: &mut Vec<f32>, f0: f32, f1: f32, ms: f32, gain: f32, edge: f32) {
    let n = (RATE as f32 * ms / 1000.0) as usize;
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let f = f0 + (f1 - f0) * t;
        phase += std::f32::consts::TAU * f / RATE as f32;
        let attack = (i as f32 / (RATE as f32 * 0.003)).min(1.0);
        let env = attack * (-4.0 * t).exp() * (1.0 - t);
        out.push(gain * env * (phase.sin() + edge * (3.0 * phase).sin()));
    }
}

fn samples(s: Sound) -> Vec<f32> {
    let mut v = Vec::new();
    match s {
        Sound::Move => tone(&mut v, 2400.0, 1700.0, 32.0, 0.10, 0.15),
        Sound::Select => { tone(&mut v, 880.0, 900.0, 45.0, 0.13, 0.25); tone(&mut v, 1320.0, 1360.0, 90.0, 0.13, 0.25); }
        Sound::Back => tone(&mut v, 1200.0, 620.0, 90.0, 0.12, 0.2),
        Sound::Error => { tone(&mut v, 190.0, 170.0, 90.0, 0.16, 0.6); tone(&mut v, 160.0, 150.0, 110.0, 0.14, 0.6); }
        // (a Steam QR code is waiting: three rising notes, a little louder)
        Sound::Attention => for f in [660.0, 880.0, 1320.0] { tone(&mut v, f, f, 160.0, 0.2, 0.15); },
        Sound::Launch => { tone(&mut v, 440.0, 1760.0, 260.0, 0.12, 0.3); tone(&mut v, 1760.0, 1700.0, 220.0, 0.08, 0.1); }
    }
    v
}
