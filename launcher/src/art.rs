// SPDX-License-Identifier: GPL-3.0-or-later
// The pictures behind the menus: MCC's own tile pictures (Data/UI/BackgroundVideos/Images), read from the player's
// game files at run time; nothing of MCC's is in the app. Decoded on a thread, so the window opens at once and the
// art fades in; a blurred copy of each serves as the backdrop.
use crate::proc::Waker;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

type Key = (String, bool); // (file name, blurred)

pub struct Art {
    folder: Option<PathBuf>,
    loaded: HashMap<Key, egui::TextureHandle>,
    asked: HashSet<Key>,
    tx: Sender<(Key, Option<egui::ColorImage>)>,
    rx: Receiver<(Key, Option<egui::ColorImage>)>,
    waker: Option<Waker>, // None: decode right away (snapshots)
}

/// The picture, blurred for a backdrop.
fn decode(folder: &Path, name: &str, blur: bool) -> Option<egui::ColorImage> {
    let img = image::open(folder.join("Data/UI/BackgroundVideos/Images").join(name)).ok()?.to_rgba8();
    let img = if blur { image::imageops::blur(&img, 3.0) } else { img };
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

impl Art {
    pub fn new(waker: Option<Waker>) -> Self {
        let (tx, rx) = channel();
        Art { folder: None, loaded: HashMap::new(), asked: HashSet::new(), tx, rx, waker }
    }

    /// Where the game files are (their pictures come from there); a new folder starts afresh.
    pub fn set_folder(&mut self, folder: Option<&Path>) {
        if self.folder.as_deref() == folder { return; }
        self.folder = folder.map(Path::to_path_buf);
        self.loaded.clear();
        self.asked.clear();
    }

    /// The picture, once it's ready (asked for the first time, it starts loading).
    pub fn get(&mut self, ctx: &egui::Context, name: &str, blur: bool) -> Option<egui::TextureHandle> {
        for ((n, b), img) in self.rx.try_iter() {
            if let Some(img) = img { self.loaded.insert((n.clone(), b), ctx.load_texture(format!("{n}{b}"), img, egui::TextureOptions::LINEAR)); }
        }
        let key = (name.to_string(), blur);
        if let Some(t) = self.loaded.get(&key) { return Some(t.clone()); }
        let folder = self.folder.clone()?;
        if self.asked.insert(key.clone()) {
            match &self.waker {
                None => {
                    let img = decode(&folder, name, blur)?;
                    let t = ctx.load_texture(format!("{name}{blur}"), img, egui::TextureOptions::LINEAR);
                    self.loaded.insert(key, t.clone());
                    return Some(t);
                }
                Some(w) => {
                    let (tx, w, n) = (self.tx.clone(), w.clone(), name.to_string());
                    std::thread::spawn(move || { let _ = tx.send(((n.clone(), blur), decode(&folder, &n, blur))); w(); });
                }
            }
        }
        None
    }
}
