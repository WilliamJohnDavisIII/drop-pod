// SPDX-License-Identifier: GPL-3.0-or-later
// Reads each game's menu music out of the player's own MCC files and turns it into standard Vorbis packets (decoded by
// lewton in music.rs); nothing of MCC's is in the app. Two formats, both read in place:
// - Wwise Vorbis (Halo CE, Halo 2, Reach): MCC's menu music, streamed .wem files in its UE4 pak
//   (MCC/Content/Paks/MCC-WindowsNoEditor.pak, format v7, entries stored uncompressed). Their setup headers name
//   codebooks by number in Wwise's packed codebook library, a table inside MCC-Win64-Shipping.exe, and their audio
//   packets lack the packet-type and window bits.
// - FMOD FSB5 Vorbis (Halo 3): <game>/fmod/pc/sfx.fsb, packets stored as [u16 size][packet] with the three Vorbis
//   headers left out; the setup header comes from FMOD's table in the game's DLL. sfx.fsb.info names the samples.
// (The open-source ww2ogg and vgmstream read the same formats.)
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One piece of music as standard Vorbis: its three header packets and its audio packets.
pub struct VorbisStream {
    pub sample_rate: u32,
    pub frames: u32, // to play; the decoder's output is cut there
    pub headers: [Vec<u8>; 3], // identification, comment, setup
    pub packets: Vec<Vec<u8>>,
}

fn u16_at(b: &[u8], o: usize) -> u16 { u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) }
fn u32_at(b: &[u8], o: usize) -> u32 { u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) }
fn u64_at(b: &[u8], o: usize) -> u64 { u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) }

pub fn read_at(f: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut v = vec![0; len];
    f.seek(SeekFrom::Start(offset)).ok()?;
    f.read_exact(&mut v).ok()?;
    Some(v)
}

fn ilog(v: u32) -> u32 { 32 - v.leading_zeros() }

// MARK: Vorbis bit packing (least significant bit first)

struct BitReader<'a> { p: &'a [u8], bit: usize, overrun: bool }

impl<'a> BitReader<'a> {
    fn new(p: &'a [u8]) -> Self { BitReader { p, bit: 0, overrun: false } }
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0;
        for i in 0..n {
            let Some(b) = self.p.get(self.bit >> 3) else { self.overrun = true; return v };
            v |= (((b >> (self.bit & 7)) & 1) as u32) << i;
            self.bit += 1;
        }
        v
    }
}

#[derive(Default)]
struct BitWriter { out: Vec<u8>, bit: u32 }

impl BitWriter {
    fn write(&mut self, v: u32, n: u32) {
        for i in 0..n {
            if self.bit == 0 { self.out.push(0); }
            *self.out.last_mut().unwrap() |= (((v >> i) & 1) as u8) << self.bit;
            self.bit = (self.bit + 1) & 7;
        }
    }
    fn bytes(&mut self, b: &[u8]) { for &x in b { self.write(x as u32, 8); } }
    /// Copies n bits from r and returns them.
    fn copy(&mut self, r: &mut BitReader, n: u32) -> u32 { let v = r.read(n); self.write(v, n); v }
}

fn id_header(channels: u16, rate: u32, block0: u32, block1: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.write(1, 8);
    w.bytes(b"vorbis");
    w.write(0, 32); // version
    w.write(channels as u32, 8);
    w.write(rate, 32);
    w.write(0, 32); w.write(0, 32); w.write(0, 32); // bitrates
    w.write(block0, 4);
    w.write(block1, 4);
    w.write(1, 8); // framing
    w.out
}

fn comment_header() -> Vec<u8> {
    let mut w = BitWriter::default();
    w.write(3, 8);
    w.bytes(b"vorbis");
    w.write(8, 32);
    w.bytes(b"Drop Pod");
    w.write(0, 32); // no comments
    w.write(1, 8);
    w.out
}

// MARK: PE images (the game DLLs and MCC's exe, read from disk, never loaded)

struct Section { name: [u8; 8], va: u32, vsize: u32, raw: u32, raw_size: u32 }
struct Pe { base: u64, sections: Vec<Section> }

impl Pe {
    fn read(f: &mut File) -> Option<Pe> {
        let h = read_at(f, 0, 4096)?;
        if &h[..2] != b"MZ" { return None; }
        let e = u32_at(&h, 0x3C) as usize;
        if e + 24 + 32 > h.len() || &h[e..e + 4] != b"PE\0\0" || u16_at(&h, e + 24) != 0x20B { return None; }
        let count = u16_at(&h, e + 6) as usize;
        let mut so = e + 24 + u16_at(&h, e + 20) as usize;
        let mut sections = vec![];
        for _ in 0..count {
            if so + 40 > h.len() { break; }
            sections.push(Section { name: h[so..so + 8].try_into().unwrap(), vsize: u32_at(&h, so + 8), va: u32_at(&h, so + 12),
                                    raw_size: u32_at(&h, so + 16), raw: u32_at(&h, so + 20) });
            so += 40;
        }
        Some(Pe { base: u64_at(&h, e + 24 + 24), sections })
    }
    fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name.split(|&b| b == 0).next() == Some(name.as_bytes()))
    }
    fn file_offset(&self, va: u64) -> Option<u64> {
        let rva = va.checked_sub(self.base)?;
        let s = self.sections.iter().find(|s| rva >= s.va as u64 && rva < s.va as u64 + s.vsize.max(s.raw_size) as u64)?;
        let d = rva - s.va as u64;
        (d < s.raw_size as u64).then(|| s.raw as u64 + d)
    }
}

// MARK: MCC's pak

pub struct Pak { file: File, entries: HashMap<String, (u64, u64)> } // lower-case path -> (offset, size), uncompressed only

impl Pak {
    pub fn open(path: &Path) -> Option<Pak> {
        let mut file = File::open(path).ok()?;
        let size = file.metadata().ok()?.len();
        let footer = read_at(&mut file, size.checked_sub(61)?, 61)?;
        let (index_offset, index_size) = (u64_at(&footer, 25), u64_at(&footer, 33));
        if u32_at(&footer, 17) != 0x5A6F12E1 || footer[16] != 0 || index_size == 0 || index_size > 64 << 20 { return None; }
        let index = read_at(&mut file, index_offset, index_size as usize)?;
        let mut r = Cursor { b: &index, o: 0 };
        let mount = r.fstring()?;
        let count = r.u32()?;
        let mut entries = HashMap::new();
        for _ in 0..count {
            let name = (mount.clone() + &r.fstring()?).replace('\\', "/").to_lowercase();
            let (offset, size) = (r.u64()?, r.u64()?);
            r.u64()?; // uncompressed size
            let compression = r.u32()?;
            r.skip(20)?; // sha1
            if compression != 0 { let blocks = r.u32()? as usize; r.skip(blocks * 16)?; }
            r.skip(5)?; // encrypted, block size
            if compression == 0 { entries.insert(name, (offset, size)); }
        }
        Some(Pak { file, entries })
    }

    /// The bytes of the entry whose path ends with `suffix` (lower case, '/' separators).
    pub fn read(&mut self, suffix: &str) -> Option<Vec<u8>> {
        let &(offset, size) = self.entries.iter().find(|(k, _)| k.ends_with(suffix))?.1;
        if size > 256 << 20 { return None; }
        read_at(&mut self.file, offset + 53, size as usize) // after the entry's 53-byte inline header
    }
}

struct Cursor<'a> { b: &'a [u8], o: usize }

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.o..self.o.checked_add(n)?)?;
        self.o += n;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> { self.take(n).map(|_| ()) }
    fn u32(&mut self) -> Option<u32> { self.take(4).map(|s| u32_at(s, 0)) }
    fn u64(&mut self) -> Option<u64> { self.take(8).map(|s| u64_at(s, 0)) }
    fn fstring(&mut self) -> Option<String> {
        let n = self.u32()? as i32;
        let s = if n >= 0 { String::from_utf8_lossy(self.take(n as usize)?).into_owned() } else {
            self.take(n.unsigned_abs() as usize * 2)?.chunks(2).map(|c| c[0] as char).collect() // UTF-16; MCC's paths are ASCII
        };
        Some(s.trim_end_matches('\0').to_string())
    }
}

// MARK: Wwise

/// Wwise's packed codebook library, found in MCC-Win64-Shipping.exe.
pub struct Codebooks { blob: Vec<u8>, offsets: Vec<u32> } // count + 1 offsets; the last book's end is estimated

/// The Vorbis I spec's lookup1_values: the largest vals with vals^dimensions <= entries.
fn lookup1_values(entries: u32, dimensions: u32) -> u32 {
    if dimensions == 0 || entries == 0 { return 0; }
    let mut vals = 1u32;
    loop {
        let mut power = 1u64;
        for _ in 0..dimensions { power *= vals as u64 + 1; if power > entries as u64 { return vals; } }
        vals += 1;
    }
}

/// One packed codebook -> a standard Vorbis codebook.
fn rebuild_codebook(r: &mut BitReader, w: &mut BitWriter) -> bool {
    let dimensions = r.read(4);
    let entries = r.read(14);
    w.write(0x564342, 24);
    w.write(dimensions, 16);
    w.write(entries, 24);
    if w.copy(r, 1) == 1 { // ordered
        w.copy(r, 5);
        let mut current = 0;
        while current < entries {
            current += w.copy(r, ilog(entries - current));
            if r.overrun { return false; }
        }
        if current > entries { return false; }
    } else {
        let length_bits = r.read(3);
        let sparse = r.read(1);
        if length_bits == 0 || length_bits > 5 { return false; }
        w.write(sparse, 1);
        for _ in 0..entries {
            if r.overrun { break; }
            if sparse == 0 || w.copy(r, 1) != 0 { let v = r.read(length_bits); w.write(v, 5); }
        }
    }
    let lookup = r.read(1);
    w.write(lookup, 4);
    if lookup == 1 {
        w.copy(r, 32); // min
        w.copy(r, 32); // max
        let value_length = w.copy(r, 4);
        w.copy(r, 1); // sequence
        for _ in 0..lookup1_values(entries, dimensions) { if r.overrun { break; } w.copy(r, value_length + 1); }
    }
    !r.overrun
}

impl Codebooks {
    /// Finds the library: a NULL-terminated run of 400 or more rising pointers into .rdata in the exe's .data, books a
    /// few bytes to a few KB apart, every one of which (but the last) parses to exactly its size.
    pub fn from_exe(path: &Path) -> Option<Codebooks> {
        let mut f = File::open(path).ok()?;
        let pe = Pe::read(&mut f)?;
        let (data, rdata) = (pe.section(".data")?, pe.section(".rdata")?);
        if data.raw_size > 64 << 20 { return None; }
        let sec = read_at(&mut f, data.raw as u64, data.raw_size as usize)?;
        let (lo, hi) = (pe.base + rdata.va as u64, pe.base + rdata.va as u64 + rdata.vsize as u64);
        let q = |i: usize| u64_at(&sec, i * 8);
        let n = sec.len() / 8;
        let mut i = 0;
        while i < n {
            let a = q(i);
            if a < lo || a >= hi { i += 1; continue; }
            let mut j = i;
            while j + 1 < n { let b = q(j + 1); if b <= q(j) || b - q(j) >= 4096 || b >= hi { break; } j += 1; }
            let run = j - i + 1;
            if run >= 400 && j + 1 < n && q(j + 1) == 0 {
                let first = q(i);
                let span = (q(j) - first) as usize + 4096;
                if let Some(blob) = pe.file_offset(first).and_then(|fo| read_at(&mut f, fo, span)) {
                    let mut offsets: Vec<u32> = (i..=j).map(|k| (q(k) - first) as u32).collect();
                    offsets.push(span as u32);
                    let good = (0..run - 1).filter(|&k| {
                        let size = (offsets[k + 1] - offsets[k]) as usize;
                        let mut r = BitReader::new(&blob[offsets[k] as usize..offsets[k] as usize + size]);
                        rebuild_codebook(&mut r, &mut BitWriter::default()) && r.bit / 8 + 1 == size
                    }).count();
                    if good + 1 == run { return Some(Codebooks { blob, offsets }); }
                }
            }
            i = j + 1;
        }
        None
    }

    fn rebuild(&self, id: u32, w: &mut BitWriter) -> bool {
        let id = id as usize;
        if id + 1 >= self.offsets.len() { return false; }
        rebuild_codebook(&mut BitReader::new(&self.blob[self.offsets[id] as usize..self.offsets[id + 1] as usize]), w)
    }
}

/// The compacted setup packet (after its codebook ids) -> the rest of a standard setup header, and each mode's
/// block flag.
fn rebuild_setup(r: &mut BitReader, w: &mut BitWriter, channels: u32, books: u32) -> Option<Vec<bool>> {
    w.write(0, 6); w.write(0, 16); // time domain transforms: one placeholder

    let floors = w.copy(r, 6) + 1;
    for _ in 0..floors {
        w.write(1, 16); // floor type 1
        let partitions = w.copy(r, 5);
        let class: Vec<u32> = (0..partitions).map(|_| w.copy(r, 4)).collect();
        let max_class = class.iter().copied().max().unwrap_or(0);
        let mut dims = vec![0; max_class as usize + 1];
        for d in dims.iter_mut() {
            *d = w.copy(r, 3) + 1;
            let subclasses = w.copy(r, 2);
            if subclasses != 0 && w.copy(r, 8) >= books { return None; } // masterbook
            for _ in 0..1u32 << subclasses { if w.copy(r, 8) as i32 - 1 >= books as i32 { return None; } }
        }
        w.copy(r, 2); // multiplier
        let rangebits = w.copy(r, 4);
        for c in &class { for _ in 0..dims[*c as usize] { w.copy(r, rangebits); } }
        if r.overrun { return None; }
    }

    let residues = w.copy(r, 6) + 1;
    for _ in 0..residues {
        let kind = r.read(2);
        if kind > 2 { return None; }
        w.write(kind, 16);
        w.copy(r, 24); w.copy(r, 24); w.copy(r, 24); // begin, end, partition size - 1
        let classifications = w.copy(r, 6) + 1;
        if w.copy(r, 8) >= books { return None; } // classbook
        let cascade: Vec<u32> = (0..classifications).map(|_| {
            let low = w.copy(r, 3);
            let high = if w.copy(r, 1) == 1 { w.copy(r, 5) } else { 0 };
            high * 8 + low
        }).collect();
        for c in cascade { for k in 0..8 { if c & (1 << k) != 0 && w.copy(r, 8) >= books { return None; } } }
        if r.overrun { return None; }
    }

    let mappings = w.copy(r, 6) + 1;
    for _ in 0..mappings {
        w.write(0, 16); // mapping type 0
        let submaps = if w.copy(r, 1) == 1 { w.copy(r, 4) + 1 } else { 1 };
        if w.copy(r, 1) == 1 { // square polar
            let steps = w.copy(r, 8) + 1;
            let bits = ilog(channels - 1);
            for _ in 0..steps {
                let (m, a) = (w.copy(r, bits), w.copy(r, bits));
                if m == a || m >= channels || a >= channels { return None; }
            }
        }
        if w.copy(r, 2) != 0 { return None; } // reserved
        if submaps > 1 { for _ in 0..channels { if w.copy(r, 4) >= submaps { return None; } } }
        for _ in 0..submaps {
            w.copy(r, 8); // time config
            if w.copy(r, 8) >= floors || w.copy(r, 8) >= residues { return None; }
        }
        if r.overrun { return None; }
    }

    let modes = w.copy(r, 6) + 1;
    let mut flags = vec![];
    for _ in 0..modes {
        flags.push(w.copy(r, 1) == 1);
        w.write(0, 16); w.write(0, 16); // window and transform type
        if w.copy(r, 8) >= mappings { return None; }
    }
    w.write(1, 1); // framing
    (!r.overrun).then_some(flags)
}

/// A Wwise Vorbis .wem (RIFF, codec 0xFFFF, the 0x42-byte fmt MCC uses) as standard Vorbis.
pub fn wwise_vorbis(wem: &[u8], books: &Codebooks) -> Result<VorbisStream, &'static str> {
    if wem.len() < 12 || &wem[..4] != b"RIFF" || &wem[8..12] != b"WAVE" { return Err("not RIFF/WAVE"); }
    let (mut fmt, mut data): (&[u8], &[u8]) = (&[], &[]);
    let mut o = 12;
    while o + 8 <= wem.len() {
        let n = u32_at(wem, o + 4) as usize;
        let body = &wem[o + 8..(o + 8 + n).min(wem.len())];
        match &wem[o..o + 4] { b"fmt " => fmt = body, b"data" => data = body, _ => {} }
        o += 8 + n + (n & 1);
    }
    if fmt.is_empty() || data.is_empty() { return Err("missing fmt/data"); }
    if u16_at(fmt, 0) != 0xFFFF { return Err("not Wwise Vorbis"); }
    if fmt.len() != 0x42 { return Err("unsupported Wwise Vorbis header"); }
    let channels = u16_at(fmt, 2);
    let rate = u32_at(fmt, 4);
    let v = &fmt[0x18..];
    let (frames, mod_signal, setup_at, audio_at) = (u32_at(v, 0), u32_at(v, 4), u32_at(v, 0x10) as usize, u32_at(v, 0x14) as usize);
    let (block0, block1) = (v[0x28] as u32, v[0x29] as u32);
    let mod_packets = ![0x4A, 0x4B, 0x69, 0x70].contains(&mod_signal);
    if channels == 0 || channels > 8 || rate == 0 { return Err("bad channels/rate"); }
    if !(6..=13).contains(&block0) || block1 < block0 || block1 > 13 { return Err("bad block sizes"); }

    // setup: [u16 size][codebook count - 1 (8 bits)][codebook ids (10 bits each)][compacted rest]
    if setup_at + 2 > data.len() { return Err("setup out of range"); }
    let setup_size = u16_at(data, setup_at) as usize;
    let setup = data.get(setup_at + 2..setup_at + 2 + setup_size).ok_or("setup out of range")?;
    let mut r = BitReader::new(setup);
    let mut w = BitWriter::default();
    w.write(5, 8);
    w.bytes(b"vorbis");
    let count = w.copy(&mut r, 8) + 1;
    for _ in 0..count { let id = r.read(10); if !books.rebuild(id, &mut w) { return Err("bad codebook id"); } }
    let flags = rebuild_setup(&mut r, &mut w, channels as u32, count).ok_or("bad setup header")?;
    let mode_bits = ilog(flags.len() as u32 - 1);

    // audio packets: [u16 size][packet]; modified packets lack the packet-type bit and the long-window neighbour
    // flags, which come from the previous and next packets' modes
    let mut packets = vec![];
    let mut prev_long = false;
    let mut o = audio_at;
    while o + 2 <= data.len() {
        let n = u16_at(data, o) as usize;
        let (body, next) = (o + 2, o + 2 + n);
        if n == 0 || next > data.len() { break; }
        if !mod_packets { packets.push(data[body..next].to_vec()); o = next; continue; }
        let mut pr = BitReader::new(&data[body..next]);
        let mut pw = BitWriter { out: Vec::with_capacity(n + 1), bit: 0 };
        pw.write(0, 1); // audio packet
        let mode = pw.copy(&mut pr, mode_bits) as usize;
        let Some(&long) = flags.get(mode) else { return Err("bad packet mode") };
        let rest = pr.read(8 - mode_bits);
        if long {
            let next_long = next + 2 <= data.len() && {
                let nn = u16_at(data, next) as usize;
                nn > 0 && next + 2 + nn <= data.len()
                    && flags.get(BitReader::new(&data[next + 2..next + 2 + nn]).read(mode_bits) as usize) == Some(&true)
            };
            pw.write(prev_long as u32, 1);
            pw.write(next_long as u32, 1);
        }
        prev_long = long;
        pw.write(rest, 8 - mode_bits);
        for &b in &data[body + 1..next] { pw.write(b as u32, 8); }
        packets.push(pw.out);
        o = next;
    }
    if packets.is_empty() { return Err("no audio packets"); }
    Ok(VorbisStream { sample_rate: rate, frames, headers: [id_header(channels, rate, block0, block1), comment_header(), w.out], packets })
}

// MARK: FMOD FSB5

pub struct FsbSample { pub channels: u16, pub rate: u32, pub frames: u32, offset: u64, size: usize, pub crc: u32 }

/// The FSB5 samples at the given indexes (FMOD Vorbis only).
pub fn fsb_samples(f: &mut File, want: &[usize]) -> Option<Vec<Option<FsbSample>>> {
    const RATES: [u32; 11] = [4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000];
    let h = read_at(f, 0, 0x40)?;
    if &h[..4] != b"FSB5" || u32_at(&h, 24) != 15 { return None; }
    let total = u32_at(&h, 8) as usize;
    let (headers_size, names_size, data_size) = (u32_at(&h, 12) as usize, u32_at(&h, 16) as u64, u32_at(&h, 20) as u64);
    let base = if u32_at(&h, 4) == 0 { 0x40 } else { 0x3C };
    if headers_size > 64 << 20 { return None; }
    let hs = read_at(f, base, headers_size)?;
    let data_start = base + headers_size as u64 + names_size;
    // each sample header: a u64, then chunks while the low bit says more
    let mut starts = Vec::with_capacity(total);
    let mut o = 0;
    for _ in 0..total {
        if o + 8 > hs.len() { return None; }
        starts.push(o);
        let mut more = u64_at(&hs, o) & 1 == 1;
        o += 8;
        while more { if o + 4 > hs.len() { return None; } let c = u32_at(&hs, o); o += 4 + ((c >> 1) & 0xFFFFFF) as usize; more = c & 1 == 1; }
    }
    let offset_of = |i: usize| ((u64_at(&hs, starts[i]) >> 7) & 0x7FFFFFF) << 5;
    Some(want.iter().map(|&i| {
        if i >= total { return None; }
        let v = u64_at(&hs, starts[i]);
        let mut s = FsbSample { rate: *RATES.get(((v >> 1) & 0xF) as usize)?, channels: [1, 2, 6, 8][((v >> 5) & 3) as usize],
                                frames: (v >> 34) as u32, offset: 0, size: 0, crc: 0 };
        let (mut o, mut more) = (starts[i] + 8, v & 1 == 1);
        while more && o + 4 <= hs.len() {
            let c = u32_at(&hs, o);
            let (size, kind) = (((c >> 1) & 0xFFFFFF) as usize, (c >> 25) & 0x7F);
            more = c & 1 == 1;
            o += 4;
            let chunk = hs.get(o..o + size)?;
            match kind { 1 if size >= 1 => s.channels = chunk[0] as u16, 2 if size >= 4 => s.rate = u32_at(chunk, 0), 11 if size >= 4 => s.crc = u32_at(chunk, 0), _ => {} }
            o += size;
        }
        let end = if i + 1 < total { offset_of(i + 1) } else { data_size };
        let start = offset_of(i);
        if end < start || end - start > 256 << 20 { return None; }
        s.offset = data_start + start;
        s.size = (end - start) as usize;
        Some(s)
    }).collect())
}

/// <sfx>.fsb.info: 280-byte records in sample order, {u32 hash, u32 pcm bytes, u32 channels, u32, u32 loop start,
/// u32 loop end, char path[256]}. Each wanted path's (sample index, loop end), matched case-insensitively.
pub fn fsb_info_find(f: &mut File, paths: &[String]) -> Vec<Option<(usize, u32)>> {
    const RECORD: usize = 0x118;
    let mut out = vec![None; paths.len()];
    let mut all = vec![];
    if f.seek(SeekFrom::Start(0)).is_err() || f.read_to_end(&mut all).is_err() { return out; }
    for (i, rec) in all.chunks_exact(RECORD).enumerate() {
        let path = &rec[0x18..];
        let path = &path[..path.iter().position(|&b| b == 0).unwrap_or(path.len())];
        for (k, want) in paths.iter().enumerate() {
            if out[k].is_none() && path.eq_ignore_ascii_case(want.as_bytes()) { out[k] = Some((i, u32_at(rec, 0x14))); }
        }
    }
    out
}

/// FMOD's Vorbis setup headers for the given ids, rebuilt from the game DLL's .rdata: 32-byte entries {u64 data; u32
/// size; u32 id; u64 base; i32 offset; i32 patch size}. With no base the header is `size` bytes at `data`; otherwise
/// `size` bytes from `base` with `patch size` bytes from `data` written at `offset`.
pub fn fmod_vorbis_setups(dll: &Path, ids: &[u32]) -> HashMap<u32, Vec<u8>> {
    let mut out = HashMap::new();
    let Ok(mut f) = File::open(dll) else { return out };
    let Some(pe) = Pe::read(&mut f) else { return out };
    let Some(rdata) = pe.section(".rdata").filter(|s| s.raw_size <= 64 << 20) else { return out };
    let Some(sec) = read_at(&mut f, rdata.raw as u64, rdata.raw_size as usize) else { return out };
    let mut o = 0xC;
    while o + 0x14 <= sec.len() {
        let id = u32_at(&sec, o);
        if ids.contains(&id) && !out.contains_key(&id) {
            let e = &sec[o - 0xC..];
            let (data, size, base) = (u64_at(e, 0), u32_at(e, 8) as usize, u64_at(e, 0x10));
            let (offset, patch) = (u32_at(e, 0x18) as i32, u32_at(e, 0x1C) as i32);
            let setup = pe.file_offset(data).filter(|_| (8..=0x10000).contains(&size)).and_then(|data_fo| {
                if base == 0 { return read_at(&mut f, data_fo, size); }
                if offset < 0 || patch < 0 || offset as usize + patch as usize > size { return None; }
                let mut s = read_at(&mut f, pe.file_offset(base)?, size)?;
                if patch > 0 { s[offset as usize..][..patch as usize].copy_from_slice(&read_at(&mut f, data_fo, patch as usize)?); }
                Some(s)
            });
            if let Some(s) = setup.filter(|s| s.starts_with(b"\x05vorbis")) { out.insert(id, s); }
        }
        o += 4;
    }
    out
}

pub fn fsb_vorbis(f: &mut File, s: &FsbSample, setup: &[u8], frames: u32) -> Option<VorbisStream> {
    let raw = read_at(f, s.offset, s.size)?;
    let mut setup = setup.to_vec();
    while setup.len() > 8 && setup.last() == Some(&0) { setup.pop(); } // FMOD pads its table's headers with zeros
    let mut packets = vec![];
    let mut o = 0;
    while o + 2 <= raw.len() {
        let n = u16_at(&raw, o) as usize;
        if n == 0 || o + 2 + n > raw.len() { break; } // the rest is padding
        packets.push(raw[o + 2..o + 2 + n].to_vec());
        o += 2 + n;
    }
    (!packets.is_empty()).then(|| VorbisStream {
        sample_rate: s.rate, frames,
        headers: [id_header(s.channels, s.rate, 8, 11), comment_header(), setup], packets, // FMOD's Vorbis: 256/2048 blocks
    })
}
