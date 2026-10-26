// SPDX-License-Identifier: GPL-3.0-or-later
// Each game's title music behind the menus, like MCC's: the selected game's menu theme (its intro, then its loop pieces
// in shuffled order) fades in, the one before fades out, and it all fades away when a game starts. Read from the
// player's own game files at run time (mcc_audio.rs); without them it stays quiet. A thread loads and starts the
// music; the sound device's mixer decodes, fades and plays it, and the device closes again once it's quiet.
// DROPPOD_SOUNDS=0 turns it off with the menu sounds.
use crate::mcc_audio::{self, Codebooks, Pak, VorbisStream};
use std::collections::HashMap;
use std::fs::File;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const FADE_IN: f32 = 1.5; // seconds
const CROSSFADE_OUT: f32 = 1.2;
const STOP_FADE: f32 = 1.5; // when a game starts
const SETTLE: Duration = Duration::from_millis(500); // moving through the tiles doesn't start every game's music
const LEVEL: f32 = 0.22; // MCC's menu music sits at -13 dB

enum Source { Wwise { intro: u32, loops: &'static [u32] }, Fsb { dir: &'static str, intro: &'static str, loops: &'static [&'static str] } }

/// The menu themes MCC plays: Halo CE, Halo 2 and Reach from MCC's own (its pak's media ids), Halo 3 from its engine's.
fn source(game: &str) -> Option<Source> {
    Some(match game {
        "halo1" => Source::Wwise { intro: 0, loops: &[474658064, 343278117, 151348246] },
        "halo2" => Source::Wwise { intro: 0, loops: &[224398914, 329892580, 603591577, 265954197, 239015343] },
        "haloreach" => Source::Wwise { intro: 113236351, loops: &[233624762, 414003491, 1058682765, 444023618, 108282205] },
        "halo3" => Source::Fsb { dir: "data\\sound\\music\\main_menu_delta\\delta_menu\\", intro: "in\\opening_monks.aif",
                                 loops: &["loop\\from_spot.aif", "loop\\monks_and_fx.aif", "loop\\new_dreamy.aif", "loop\\unforgotten.aif", "loop\\edh1.aif"] },
        _ => return None,
    })
}

pub struct Track { pieces: Vec<VorbisStream>, intro_loops: bool, rate: u32 } // pieces[0]: the intro, or (intro_loops) a loop

/// Asks for a game's music (None: quiet), from the game files in `folder`. Call it every frame; it only acts on changes.
pub fn want(folder: Option<&Path>, game: Option<&str>) {
    static LAST: Mutex<Option<(Option<PathBuf>, Option<String>)>> = Mutex::new(None);
    static TX: OnceLock<Option<Sender<(Option<PathBuf>, Option<String>)>>> = OnceLock::new();
    let now = (folder.map(Path::to_path_buf), game.map(str::to_owned));
    let mut last = LAST.lock().unwrap();
    if last.as_ref() == Some(&now) { return; }
    *last = Some(now.clone());
    let tx = TX.get_or_init(|| {
        if crate::paths::var("DROPPOD_SOUNDS").as_deref() == Some("0") { return None; }
        let (tx, rx) = channel();
        std::thread::spawn(move || Player::default().run(rx));
        Some(tx)
    });
    if let Some(tx) = tx { let _ = tx.send(now); }
}

/// A voice's fade, shared by the player thread (sets the target) and the mixer (moves toward it).
struct Fade { target: AtomicU32, seconds_out: AtomicU32, done: AtomicBool }

impl Fade {
    fn to(&self, target: f32, seconds_out: f32) {
        self.seconds_out.store(seconds_out.to_bits(), Ordering::Relaxed);
        self.target.store(target.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Player {
    folder: Option<PathBuf>,
    pak: Option<Pak>,
    books: Option<Option<Codebooks>>,
    tracks: HashMap<String, Option<Arc<Track>>>,
    voices: Vec<(String, Arc<Fade>)>,
    sink: Option<rodio::MixerDeviceSink>,
    quiet_since: Option<Instant>,
}

impl Player {
    fn run(mut self, rx: std::sync::mpsc::Receiver<(Option<PathBuf>, Option<String>)>) {
        let mut want: Option<String> = None;
        let mut since = Instant::now();
        let mut playing: Option<String> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok((folder, game)) => {
                    if folder != self.folder { self.folder = folder; self.pak = None; self.books = None; self.tracks.clear(); }
                    if game != want { want = game; since = Instant::now(); }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            // fading out happens at once; a new game's music waits until the selection settles
            if want != playing && (want.is_none() || playing.is_none() || since.elapsed() >= SETTLE) {
                let out = if want.is_none() { STOP_FADE } else { CROSSFADE_OUT };
                // (read first, so the music before only fades out as this fades in)
                let alive = want.as_ref().and_then(|g| self.voices.iter().find(|(v, f)| v == g && !f.done.load(Ordering::Relaxed)).map(|(_, f)| f.clone()));
                let track = if alive.is_none() { want.clone().and_then(|g| self.track(&g)) } else { None };
                for (g, f) in &self.voices { if Some(g) != want.as_ref() { f.to(0.0, out); } }
                if let Some(f) = alive { f.to(1.0, out); } // still fading out: bring it back
                if let (Some(g), Some(t)) = (&want, track) { self.start(g, t); }
                playing = want.clone();
            } else if want == playing && playing.is_some() {
                // settled: read the other games' music now, one a moment, so moving to their tiles is instant
                if let Some(g) = crate::core::games().iter().map(|g| g.id.clone()).find(|g| source(g).is_some() && !self.tracks.contains_key(g)) {
                    self.track(&g);
                }
            }
            self.voices.retain(|(_, f)| !f.done.load(Ordering::Relaxed));
            // a game is running: give back the memory (about 150 MB with every game's music read) until it ends
            if want.is_none() && self.voices.is_empty() && !self.tracks.is_empty() {
                self.tracks.clear();
                self.pak = None;
                self.books = None;
            }
            // let the device sleep once nothing has played for a few seconds
            if self.voices.is_empty() && self.sink.is_some() {
                if self.quiet_since.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(3) { self.sink = None; }
            } else {
                self.quiet_since = None;
            }
        }
    }

    fn start(&mut self, game: &str, track: Arc<Track>) {
        if self.sink.is_none() {
            self.sink = rodio::DeviceSinkBuilder::open_default_sink().ok();
            if let Some(k) = self.sink.as_mut() { k.log_on_drop(false); }
        }
        let Some(sink) = &self.sink else { return };
        let fade = Arc::new(Fade { target: AtomicU32::new(1f32.to_bits()), seconds_out: AtomicU32::new(CROSSFADE_OUT.to_bits()), done: AtomicBool::new(false) });
        sink.mixer().add(Voice::new(track, fade.clone()));
        self.voices.push((game.to_string(), fade));
    }

    /// A game's music, read from the game files the first time it's asked for.
    fn track(&mut self, game: &str) -> Option<Arc<Track>> {
        if let Some(t) = self.tracks.get(game) { return t.clone(); }
        let started = Instant::now();
        let t = self.load(game).map(Arc::new);
        crate::proc::log("music", &match &t {
            Some(t) => format!("{game}: {} pieces read in {:.1} s", t.pieces.len(), started.elapsed().as_secs_f32()),
            None => format!("{game}: no menu music in the game files"),
        });
        self.tracks.insert(game.to_string(), t.clone());
        t
    }

    fn load(&mut self, game: &str) -> Option<Track> {
        let root = self.folder.clone()?;
        let (pieces, intro_loops) = match source(game)? {
            Source::Wwise { intro, loops } => {
                if self.pak.is_none() { self.pak = Pak::open(&root.join("MCC/Content/Paks/MCC-WindowsNoEditor.pak")); }
                let books = self.books.get_or_insert_with(|| Codebooks::from_exe(&root.join("MCC/Binaries/Win64/MCC-Win64-Shipping.exe"))).as_ref()?;
                let pak = self.pak.as_mut()?;
                let mut pieces = vec![];
                for (i, id) in std::iter::once(intro).filter(|&id| id != 0).chain(loops.iter().copied()).enumerate() {
                    match pak.read(&format!("wwiseaudio/windows/{id}.wem")).ok_or("not in the pak").and_then(|w| mcc_audio::wwise_vorbis(&w, books)) {
                        Ok(s) => pieces.push(s),
                        Err(e) => { crate::proc::log("music", &format!("{game}: {id}.wem: {e}")); if i == 0 && intro != 0 { return None; } }
                    }
                }
                (pieces, intro == 0)
            }
            Source::Fsb { dir, intro, loops } => {
                let fmod = root.join(game).join("fmod/pc");
                let paths: Vec<String> = std::iter::once(intro).chain(loops.iter().copied()).map(|p| format!("{dir}{p}")).collect();
                let found = mcc_audio::fsb_info_find(&mut File::open(fmod.join("sfx.fsb.info")).ok()?, &paths);
                let indexes: Vec<usize> = found.iter().map(|h| h.map_or(usize::MAX, |(i, _)| i)).collect();
                let mut fsb = File::open(fmod.join("sfx.fsb")).ok()?;
                let samples = mcc_audio::fsb_samples(&mut fsb, &indexes)?;
                let ids: Vec<u32> = samples.iter().flatten().map(|s| s.crc).collect();
                let setups = mcc_audio::fmod_vorbis_setups(&root.join(game).join(format!("{game}.dll")), &ids);
                let mut pieces = vec![];
                for (i, (s, hit)) in samples.iter().zip(&found).enumerate() {
                    // loop pieces end at their loop end, so they join seamlessly
                    let stream = s.as_ref().zip(*hit).and_then(|(s, (_, loop_end))| {
                        let frames = if loop_end > 0 && loop_end <= s.frames { loop_end } else { s.frames };
                        mcc_audio::fsb_vorbis(&mut fsb, s, setups.get(&s.crc)?, frames)
                    });
                    match stream { Some(s) => pieces.push(s), None if i == 0 => return None, None => {} }
                }
                (pieces, false)
            }
        };
        let rate = pieces.first()?.sample_rate;
        let pieces: Vec<VorbisStream> = pieces.into_iter().filter(|p| p.sample_rate == rate).collect();
        Some(Track { pieces, intro_loops, rate })
    }
}

/// One game's music playing: stereo, decoded a packet at a time in the mixer, with its fade applied.
struct Voice {
    track: Arc<Track>,
    fade: Arc<Fade>,
    gain: f32,
    piece: usize,
    decoder: Option<(lewton::header::IdentHeader, lewton::header::SetupHeader, lewton::audio::PreviousWindowRight)>,
    packet: usize,
    played: u32, // frames of this piece so far
    buf: Vec<f32>,
    at: usize,
    bag: Vec<usize>, // loop pieces still to come this round
    last_loop: Option<usize>,
    seed: u64,
}

impl Voice {
    fn new(track: Arc<Track>, fade: Arc<Fade>) -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        let last_loop = track.intro_loops.then_some(0);
        let mut v = Voice { track, fade, gain: 0.0, piece: 0, decoder: None, packet: 0, played: 0, buf: vec![], at: 0, bag: vec![], last_loop, seed };
        v.open(0);
        v
    }

    fn random(&mut self) -> u64 { self.seed ^= self.seed << 13; self.seed ^= self.seed >> 7; self.seed ^= self.seed << 17; self.seed }

    /// The next loop piece: each once a round, in shuffled order, never the same twice in a row.
    fn next_loop(&mut self) -> usize {
        let first = if self.track.intro_loops { 0 } else { 1 };
        let n = self.track.pieces.len();
        if n <= first { return 0; } // no loops: the intro again
        if self.bag.is_empty() {
            self.bag = (first..n).collect();
            for i in (1..self.bag.len()).rev() { let j = (self.random() % (i as u64 + 1)) as usize; self.bag.swap(i, j); }
            if self.bag.len() > 1 && self.bag.last().copied() == self.last_loop { let k = self.bag.len() - 1; self.bag.swap(0, k); }
        }
        let next = self.bag.pop().unwrap();
        self.last_loop = Some(next);
        next
    }

    fn open(&mut self, piece: usize) {
        let s = &self.track.pieces[piece];
        self.piece = piece;
        self.packet = 0;
        self.played = 0;
        self.decoder = (|| {
            let ident = lewton::header::read_header_ident(&s.headers[0]).ok()?;
            let setup = lewton::header::read_header_setup(&s.headers[2], ident.audio_channels, (ident.blocksize_0, ident.blocksize_1)).ok()?;
            Some((ident, setup, lewton::audio::PreviousWindowRight::new()))
        })();
    }

    /// Decodes the next packet into `buf` as stereo, moving on to the next piece at the end of one. False: nothing
    /// in this track decodes.
    fn refill(&mut self) -> bool {
        self.buf.clear();
        self.at = 0;
        for _ in 0..self.track.pieces.len() * 2 + 64 {
            let s = &self.track.pieces[self.piece];
            let left = s.frames.saturating_sub(self.played) as usize;
            let packet = (left > 0).then(|| s.packets.get(self.packet)).flatten();
            let decoded = match (packet, self.decoder.as_mut()) {
                (Some(p), Some((ident, setup, pwr))) => lewton::audio::read_audio_packet_generic::<lewton::samples::InterleavedSamples<f32>>(ident, setup, p, pwr).ok(),
                _ => None,
            };
            let Some(d) = decoded else {
                let next = self.next_loop();
                self.open(next);
                continue;
            };
            self.packet += 1;
            let ch = d.channel_count.max(1);
            let frames = (d.samples.len() / ch).min(left);
            self.played += frames as u32;
            if frames == 0 { continue; } // (the first packet of a piece only primes the decoder)
            // fold to stereo in Vorbis channel order: 4 = L R BL BR, 6 = L C R BL BR LFE (LFE dropped)
            for f in d.samples.chunks_exact(ch).take(frames) {
                let (l, r) = match ch {
                    1 => (f[0], f[0]),
                    2 => (f[0], f[1]),
                    3 => (f[0] + 0.7071 * f[1], f[2] + 0.7071 * f[1]),
                    4 | 5 => ((f[0] + 0.7071 * f[2]) * 0.7, (f[1] + 0.7071 * f[3]) * 0.7),
                    _ => ((f[0] + 0.7071 * (f[1] + f[3])) * 0.7, (f[2] + 0.7071 * (f[1] + f[4])) * 0.7),
                };
                self.buf.push(l);
                self.buf.push(r);
            }
            return true;
        }
        false
    }
}

impl Iterator for Voice {
    type Item = rodio::Sample;
    fn next(&mut self) -> Option<f32> {
        if self.at >= self.buf.len() {
            if !self.refill() { self.fade.done.store(true, Ordering::Relaxed); return None; }
        }
        if self.at % 2 == 0 { // once a frame: move the fade toward its target
            let target = f32::from_bits(self.fade.target.load(Ordering::Relaxed));
            let seconds = if target > self.gain { FADE_IN } else { f32::from_bits(self.fade.seconds_out.load(Ordering::Relaxed)) };
            let step = 1.0 / (seconds * self.track.rate as f32);
            self.gain = if target > self.gain { (self.gain + step).min(target) } else { (self.gain - step).max(target) };
            if self.gain <= 0.0 && target <= 0.0 { self.fade.done.store(true, Ordering::Relaxed); return None; }
        }
        let v = self.buf[self.at] * LEVEL * self.gain * self.gain; // (squared: the fade sounds even to the ear)
        self.at += 1;
        Some(v)
    }
}

impl rodio::Source for Voice {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> rodio::ChannelCount { NonZero::new(2).unwrap() }
    fn sample_rate(&self) -> rodio::SampleRate { NonZero::new(self.track.rate).unwrap() }
    fn total_duration(&self) -> Option<Duration> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With MCC's files on this computer (DROPPOD_TEST_MCC, else the developer's usual place), every game's menu
    /// music reads and decodes: a few seconds of each, not silent.
    #[test]
    fn menu_music_decodes_from_real_game_files() {
        let root = PathBuf::from(crate::paths::var("DROPPOD_TEST_MCC").unwrap_or("/Volumes/Library/Games/Halo MCC".into()));
        if !root.join("MCC/Content/Paks/MCC-WindowsNoEditor.pak").exists() { eprintln!("no MCC files: skipped"); return; }
        let mut p = Player { folder: Some(root.clone()), ..Default::default() };
        for game in ["halo1", "halo2", "halo3", "haloreach"] {
            if game != "halo1" && !root.join(game).exists() { continue; }
            let t = p.track(game).unwrap_or_else(|| panic!("{game}: no music"));
            assert!(t.pieces.len() >= 3, "{game}: {} pieces", t.pieces.len());
            let fade = Arc::new(Fade { target: AtomicU32::new(1f32.to_bits()), seconds_out: AtomicU32::new(1f32.to_bits()), done: AtomicBool::new(false) });
            let samples: Vec<f32> = Voice::new(t.clone(), fade).take(t.rate as usize * 2 * 4).collect();
            assert_eq!(samples.len(), t.rate as usize * 2 * 4, "{game}: ended early");
            let loud = samples[samples.len() / 2..].iter().map(|s| s.abs()).fold(0.0, f32::max);
            assert!(loud > 0.001 && loud <= 1.0, "{game}: peak {loud}");
        }
    }
}
