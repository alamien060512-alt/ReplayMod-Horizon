// Replay: record a world session and play it back. An original Horizon native
// mod (no code from any Java replay mod). While recording, each server tick it
// notes what changed: mobs and players appearing, moving and leaving, and
// every block change with its old and new state. Playback re-creates those as
// stand-ins (mobs with no AI; players as named villagers), re-applies the
// block changes, and on `end` puts every changed block back. The game-facing
// glue is in the other .rs files; this file is pure logic and the mod's Cargo
// library (its tests), and the module the bundle adds to the game.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const ID: &str = "replay";
pub const NAME: &str = "Replay";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str = "Record a world session with /replay start and /replay stop, and play it back with /replay play <name>. Use spectator mode for a free camera.";
const MAGIC: &str = "replay 1";
pub const TICKS_PER_SECOND: u32 = 20;
/// 30 minutes of ticks; recording stops (and keeps what it has) after that.
pub const MAX_TICKS: u32 = 36_000;
pub const MAX_EVENTS: usize = 2_000_000;
pub const MAX_SPEED: u32 = 8;

/// A mob or player as the recording knows it. `key` is `m<id>` for a mob and
/// `p<uuid>` for a player.
#[derive(Clone, Debug, PartialEq)]
pub struct Ent {
    pub key: String,
    pub kind: String,
    pub name: Option<String>,
    pub pos: [f64; 3],
    pub yaw: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Spawn(Ent),
    Move { key: String, pos: [f64; 3], yaw: f32 },
    Despawn(String),
    Block { pos: [i32; 3], old: u32, new: u32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    pub dimension: String,
    /// (tick, event), in tick order.
    pub events: Vec<(u32, Event)>,
    pub ticks: u32,
}

// ---------------------------------------------------------------- file format

fn clean_field(s: &str) -> String {
    s.chars().map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c }).collect()
}

pub fn encode(r: &Recording) -> String {
    let mut out = format!("{MAGIC}\n{}\t{}\n", clean_field(&r.dimension), r.ticks);
    for (t, e) in &r.events {
        match e {
            Event::Spawn(s) => out.push_str(&format!(
                "{t}\tS\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                clean_field(&s.key),
                clean_field(&s.kind),
                s.name.as_deref().map(clean_field).unwrap_or_default(),
                s.pos[0],
                s.pos[1],
                s.pos[2],
                s.yaw
            )),
            Event::Move { key, pos, yaw } => out.push_str(&format!(
                "{t}\tM\t{}\t{}\t{}\t{}\t{}\n",
                clean_field(key),
                pos[0],
                pos[1],
                pos[2],
                yaw
            )),
            Event::Despawn(key) => out.push_str(&format!("{t}\tD\t{}\n", clean_field(key))),
            Event::Block { pos, old, new } => {
                out.push_str(&format!("{t}\tB\t{}\t{}\t{}\t{old}\t{new}\n", pos[0], pos[1], pos[2]))
            }
        }
    }
    out
}

pub fn decode(text: &str) -> Option<Recording> {
    let mut lines = text.lines();
    if lines.next()? != MAGIC {
        return None;
    }
    let mut head = lines.next()?.split('\t');
    let dimension = head.next()?.to_owned();
    let ticks: u32 = head.next()?.parse().ok()?;
    let mut events = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let mut f = line.split('\t');
        let t: u32 = f.next()?.parse().ok()?;
        let event = match f.next()? {
            "S" => {
                let key = f.next()?.to_owned();
                let kind = f.next()?.to_owned();
                let name = Some(f.next()?.to_owned()).filter(|n| !n.is_empty());
                let pos = [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?];
                Event::Spawn(Ent { key, kind, name, pos, yaw: f.next()?.parse().ok()? })
            }
            "M" => Event::Move {
                key: f.next()?.to_owned(),
                pos: [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?],
                yaw: f.next()?.parse().ok()?,
            },
            "D" => Event::Despawn(f.next()?.to_owned()),
            "B" => Event::Block {
                pos: [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?],
                old: f.next()?.parse().ok()?,
                new: f.next()?.parse().ok()?,
            },
            _ => return None,
        };
        events.push((t, event));
    }
    Some(Recording { dimension, events, ticks })
}

pub fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    (!name.is_empty()).then_some(name)
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.replay"))
}

pub fn duration(ticks: u32) -> String {
    let s = ticks / TICKS_PER_SECOND;
    format!("{}:{:02}", s / 60, s % 60)
}

// ------------------------------------------------------------------ recorder

#[derive(Debug)]
pub struct Recorder {
    rec: Recording,
    last: HashMap<String, Ent>,
    full: bool,
}

impl Recorder {
    pub fn new(dimension: &str) -> Self {
        Self {
            rec: Recording { dimension: dimension.to_owned(), ..Default::default() },
            last: HashMap::new(),
            full: false,
        }
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    /// One server tick: the entities now present. Only changes are stored.
    pub fn tick(&mut self, now: &[Ent]) {
        if self.rec.ticks >= MAX_TICKS || self.rec.events.len() >= MAX_EVENTS {
            self.full = true;
            return;
        }
        let t = self.rec.ticks;
        let present: HashSet<&str> = now.iter().map(|e| e.key.as_str()).collect();
        let mut gone: Vec<String> =
            self.last.keys().filter(|k| !present.contains(k.as_str())).cloned().collect();
        gone.sort();
        for key in gone {
            self.last.remove(&key);
            self.rec.events.push((t, Event::Despawn(key)));
        }
        for e in now {
            match self.last.get(&e.key) {
                None => self.rec.events.push((t, Event::Spawn(e.clone()))),
                Some(old) if old.pos != e.pos || old.yaw != e.yaw => {
                    self.rec.events.push((t, Event::Move { key: e.key.clone(), pos: e.pos, yaw: e.yaw }))
                }
                _ => continue,
            }
            self.last.insert(e.key.clone(), e.clone());
        }
        self.rec.ticks += 1;
    }

    /// A block change during the current tick (a change between ticks counts
    /// for the next one).
    pub fn block(&mut self, pos: [i32; 3], old: u32, new: u32) {
        if self.rec.events.len() < MAX_EVENTS {
            self.rec.events.push((self.rec.ticks, Event::Block { pos, old, new }));
        } else {
            self.full = true;
        }
    }

    pub fn ticks(&self) -> u32 {
        self.rec.ticks
    }

    pub fn finish(self) -> Recording {
        self.rec
    }
}

// ------------------------------------------------------------------ playback

#[derive(Debug)]
pub struct Playback {
    pub name: String,
    rec: Recording,
    /// Next event to apply.
    at: usize,
    pub tick: u32,
    pub paused: bool,
    pub speed: u32,
    live: HashSet<String>,
    applied_blocks: Vec<([i32; 3], u32)>,
}

impl Playback {
    pub fn new(name: &str, rec: Recording) -> Self {
        Self {
            name: name.to_owned(),
            rec,
            at: 0,
            tick: 0,
            paused: false,
            speed: 1,
            live: HashSet::new(),
            applied_blocks: Vec::new(),
        }
    }

    pub fn dimension(&self) -> &str {
        &self.rec.dimension
    }
    pub fn total(&self) -> u32 {
        self.rec.ticks
    }
    pub fn finished(&self) -> bool {
        self.tick >= self.rec.ticks
    }

    /// The events of the next `speed` ticks (none while paused or finished).
    /// The caller carries them out and tells `Runtime` the ids it got.
    pub fn advance(&mut self) -> Vec<Event> {
        if self.paused {
            return Vec::new();
        }
        self.advance_by(self.speed)
    }

    /// Jumps forward `ticks` ticks, returning every event passed.
    pub fn advance_by(&mut self, ticks: u32) -> Vec<Event> {
        let target = self.tick.saturating_add(ticks).min(self.rec.ticks);
        let mut out = Vec::new();
        while let Some((t, e)) = self.rec.events.get(self.at) {
            if *t >= target {
                break;
            }
            match e {
                Event::Spawn(s) => {
                    self.live.insert(s.key.clone());
                }
                Event::Despawn(k) => {
                    self.live.remove(k);
                }
                Event::Block { pos, old, .. } => self.applied_blocks.push((*pos, *old)),
                Event::Move { .. } => {}
            }
            out.push(e.clone());
            self.at += 1;
        }
        self.tick = target;
        out
    }

    /// What ends the playback: despawn what is still standing in, and put the
    /// blocks back, last change first (so a block changed twice returns to
    /// what it was before the recording).
    pub fn finish(&mut self) -> Vec<Event> {
        let mut keys: Vec<String> = self.live.drain().collect();
        keys.sort();
        let mut out: Vec<Event> = keys.into_iter().map(Event::Despawn).collect();
        for (pos, old) in self.applied_blocks.drain(..).rev() {
            out.push(Event::Block { pos, old: 0, new: old });
        }
        out
    }
}

// --------------------------------------------------------------------- state

enum State {
    Idle,
    Recording(Recorder),
    Playing(Playback),
}

thread_local! {
    static STATE: RefCell<State> = const { RefCell::new(State::Idle) };
    /// Recorded key -> the live entity id standing in for it, and whether it
    /// is a mob (true) or ... always a mob; kept for clarity.
    static BOUND: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

pub fn bind(key: &str, id: u32) {
    BOUND.with(|b| b.borrow_mut().insert(key.to_owned(), id));
}
pub fn unbind(key: &str) -> Option<u32> {
    BOUND.with(|b| b.borrow_mut().remove(key))
}
pub fn live_id(key: &str) -> Option<u32> {
    BOUND.with(|b| b.borrow().get(key).copied())
}

pub fn is_recording() -> bool {
    STATE.with(|s| matches!(&*s.borrow(), State::Recording(_)))
}
pub fn is_playing() -> bool {
    STATE.with(|s| matches!(&*s.borrow(), State::Playing(_)))
}

/// The recorder takes this tick's snapshot (called from the server thread).
pub fn record_tick(dimension: &str, now: &[Ent]) {
    STATE.with(|s| {
        if let State::Recording(r) = &mut *s.borrow_mut()
            && r.rec.dimension == dimension
        {
            r.tick(now);
        }
    });
}

pub fn record_block(dimension: &str, pos: [i32; 3], old: u32, new: u32) {
    STATE.with(|s| {
        if let State::Recording(r) = &mut *s.borrow_mut()
            && r.rec.dimension == dimension
        {
            r.block(pos, old, new);
        }
    });
}

/// The playback's dimension, if one is playing.
pub fn playing_dimension() -> Option<String> {
    STATE.with(|s| match &*s.borrow() {
        State::Playing(p) => Some(p.dimension().to_owned()),
        _ => None,
    })
}

/// The events to carry out this tick; when the playback has run out, the
/// events that clean up after it and the playback ends.
pub fn playback_tick() -> Vec<Event> {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let State::Playing(p) = &mut *st else { return Vec::new() };
        let mut out = p.advance();
        if p.finished() {
            out.extend(p.finish());
            *st = State::Idle;
        }
        out
    })
}

/// What `end` (or a world closing) must undo.
pub fn end_playback() -> Vec<Event> {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let State::Playing(p) = &mut *st else { return Vec::new() };
        let out = p.finish();
        *st = State::Idle;
        out
    })
}

pub fn hud_line() -> Option<String> {
    STATE.with(|s| match &*s.borrow() {
        State::Idle => None,
        State::Recording(r) => Some(format!("REC {}", duration(r.ticks()))),
        State::Playing(p) => Some(format!(
            "{} {}{}/{} x{}",
            p.name,
            if p.paused { "|| " } else { "" },
            duration(p.tick),
            duration(p.total()),
            p.speed
        )),
    })
}

// ------------------------------------------------------------------ commands

pub const USAGES: [&str; 9] = [
    "replay start",
    "replay stop [name:word]",
    "replay list",
    "replay play <name:word>",
    "replay pause",
    "replay speed <times:integer>",
    "replay skip <seconds:integer>",
    "replay end",
    "replay delete <name:word>",
];

/// What a command asks the game to do besides answering.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub say: Vec<String>,
    /// Events to carry out now (a skip, or an end's clean-up).
    pub events: Vec<Event>,
    /// The dimension a playback needs the game to be in.
    pub needs_dimension: Option<String>,
}

fn say(s: impl Into<String>) -> Outcome {
    Outcome { say: vec![s.into()], events: Vec::new(), needs_dimension: None }
}

/// Runs `replay <args>`. `dimension` is the level the player is in.
pub fn run_command(dir: &Path, dimension: &str, args: &[&str]) -> Outcome {
    let arg = args.get(1).copied();
    match args.first().copied() {
        Some("start") => STATE.with(|s| {
            let mut st = s.borrow_mut();
            match &*st {
                State::Idle => {
                    *st = State::Recording(Recorder::new(dimension));
                    say("Recording this dimension. /replay stop [name] to save")
                }
                State::Recording(_) => say("Already recording"),
                State::Playing(_) => say("End the playback first"),
            }
        }),
        Some("stop") => stop(dir, arg),
        Some("list") => {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".replay").map(str::to_owned))
                .collect();
            names.sort();
            say(if names.is_empty() { "No recordings yet".to_owned() } else { format!("Recordings: {}", names.join(", ")) })
        }
        Some("play") => play(dir, dimension, arg),
        Some("pause") => STATE.with(|s| match &mut *s.borrow_mut() {
            State::Playing(p) => {
                p.paused = !p.paused;
                say(if p.paused { "Paused" } else { "Resumed" })
            }
            _ => say("Nothing is playing"),
        }),
        Some("speed") => STATE.with(|s| match (&mut *s.borrow_mut(), arg.and_then(|a| a.parse::<u32>().ok())) {
            (State::Playing(p), Some(n)) if (1..=MAX_SPEED).contains(&n) => {
                p.speed = n;
                say(format!("Speed x{n}"))
            }
            (State::Playing(_), _) => say(format!("Speed is 1 to {MAX_SPEED}")),
            _ => say("Nothing is playing"),
        }),
        Some("skip") => STATE.with(|s| match (&mut *s.borrow_mut(), arg.and_then(|a| a.parse::<u32>().ok())) {
            (State::Playing(p), Some(secs)) => {
                let events = p.advance_by(secs.saturating_mul(TICKS_PER_SECOND));
                Outcome { say: vec![format!("At {}", duration(p.tick))], events, needs_dimension: None }
            }
            (State::Playing(_), None) => say("Usage: /replay skip <seconds> (forward only)"),
            _ => say("Nothing is playing"),
        }),
        Some("end") => {
            let was = is_playing();
            let events = end_playback();
            Outcome {
                say: vec![if was { "Playback ended; blocks put back".into() } else { "Nothing is playing".into() }],
                events,
                needs_dimension: None,
            }
        }
        Some("delete") => match arg.and_then(clean_name) {
            Some(n) if std::fs::remove_file(file(dir, &n)).is_ok() => say(format!("Deleted {n}")),
            _ => say("No such recording"),
        },
        _ => say("/replay start | stop [name] | list | play <name> | pause | speed <1-8> | skip <seconds> | end | delete <name>"),
    }
}

fn stop(dir: &Path, arg: Option<&str>) -> Outcome {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let State::Recording(r) = std::mem::replace(&mut *st, State::Idle) else {
            return say("Not recording");
        };
        let full = r.is_full();
        let rec = r.finish();
        if rec.ticks == 0 {
            return say("Nothing recorded");
        }
        let name = arg.and_then(clean_name).unwrap_or_else(|| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            format!("replay-{secs}")
        });
        let saved = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(file(dir, &name), encode(&rec)));
        let mut lines = match saved {
            Ok(()) => vec![format!("Saved {name}: {}, {} events. /replay play {name}", duration(rec.ticks), rec.events.len())],
            Err(e) => vec![format!("Could not save: {e}")],
        };
        if full {
            lines.push("The recording limit was reached; the rest was not recorded".into());
        }
        Outcome { say: lines, events: Vec::new(), needs_dimension: None }
    })
}

fn play(dir: &Path, dimension: &str, arg: Option<&str>) -> Outcome {
    let Some(name) = arg.and_then(clean_name) else {
        return say("Usage: /replay play <name>");
    };
    let Some(rec) = std::fs::read_to_string(file(dir, &name)).ok().and_then(|t| decode(&t)) else {
        return say(format!("Could not read {name}"));
    };
    if rec.dimension != dimension {
        return say(format!("{name} was recorded in {}; go there first", rec.dimension));
    }
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if !matches!(&*st, State::Idle) {
            return say("Stop recording or end the playback first");
        }
        let dim = rec.dimension.clone();
        *st = State::Playing(Playback::new(&name, rec));
        Outcome {
            say: vec![
                format!("Playing {name}. Its blocks are changed in this world until /replay end"),
                "Use spectator mode (/gamemode spectator) for a free camera".into(),
            ],
            events: Vec::new(),
            needs_dimension: Some(dim),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(key: &str, x: f64) -> Ent {
        Ent { key: key.into(), kind: "minecraft:cow".into(), name: None, pos: [x, 64.0, 0.0], yaw: 0.0 }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("replay_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn sample() -> Recording {
        let mut r = Recorder::new("minecraft:overworld");
        r.tick(&[ent("m1", 0.0)]);
        r.block([1, 2, 3], 10, 20);
        r.tick(&[ent("m1", 1.0), ent("m2", 5.0)]);
        r.block([1, 2, 3], 20, 30);
        r.tick(&[ent("m1", 1.0), ent("m2", 5.0)]);
        r.tick(&[ent("m2", 6.0)]);
        r.finish()
    }

    #[test]
    fn recorder_stores_only_changes() {
        let r = sample();
        assert_eq!(r.ticks, 4);
        let kinds: Vec<(u32, &str)> = r
            .events
            .iter()
            .map(|(t, e)| (*t, match e {
                Event::Spawn(_) => "S",
                Event::Move { .. } => "M",
                Event::Despawn(_) => "D",
                Event::Block { .. } => "B",
            }))
            .collect();
        // Block events land in the tick being recorded: before its snapshot is taken,
        // so a change between tick N-1 and N is stored at tick N.
        assert_eq!(
            kinds,
            [(0, "S"), (1, "B"), (1, "M"), (1, "S"), (2, "B"), (3, "D"), (3, "M")]
        );
    }

    #[test]
    fn file_roundtrip() {
        let mut r = sample();
        r.events.push((3, Event::Spawn(Ent { name: Some("Alex Q".into()), kind: "minecraft:villager".into(), ..ent("p9", 1.5) })));
        assert_eq!(decode(&encode(&r)), Some(r));
        assert_eq!(decode("junk"), None);
        assert_eq!(decode("replay 1\nx\t1\n"), Some(Recording { dimension: "x".into(), events: vec![], ticks: 1 }));
        assert_eq!(decode("replay 1\nx\t1\n0\tQ\n"), None);
    }

    #[test]
    fn recorder_limit() {
        let mut r = Recorder::new("d");
        for _ in 0..MAX_TICKS + 5 {
            r.tick(&[]);
        }
        assert!(r.is_full());
        assert_eq!(r.ticks(), MAX_TICKS);
    }

    #[test]
    fn playback_replays_then_undoes() {
        let mut p = Playback::new("t", sample());
        let first = p.advance();
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0], Event::Spawn(_)));
        p.speed = 2;
        let next = p.advance(); // ticks 1 and 2
        assert_eq!(next.len(), 4);
        p.paused = true;
        assert!(p.advance().is_empty());
        p.paused = false;
        let last = p.advance();
        assert!(!last.is_empty() && p.finished());
        let undo = p.finish();
        // m2 still stands; the block returns to its first old state, last change undone first.
        assert_eq!(
            undo,
            [
                Event::Despawn("m2".into()),
                Event::Block { pos: [1, 2, 3], old: 0, new: 20 },
                Event::Block { pos: [1, 2, 3], old: 0, new: 10 },
            ]
        );
    }

    #[test]
    fn skip_passes_events() {
        let mut p = Playback::new("t", sample());
        let all = p.advance_by(1000);
        assert_eq!(all.len(), 7);
        assert!(p.finished());
    }

    #[test]
    fn command_flow() {
        let dir = tmp("flow");
        let dim = "minecraft:overworld";
        STATE.with(|s| *s.borrow_mut() = State::Idle);
        assert_eq!(run_command(&dir, dim, &["stop"]).say, ["Not recording"]);
        assert_eq!(run_command(&dir, dim, &["start"]).say.len(), 1);
        assert!(is_recording());
        assert_eq!(run_command(&dir, dim, &["start"]).say, ["Already recording"]);
        record_tick("minecraft:the_nether", &[ent("m1", 0.0)]); // another dimension: ignored
        record_tick(dim, &[ent("m1", 0.0)]);
        record_block(dim, [0, 0, 0], 1, 2);
        for i in 1..40 {
            record_tick(dim, &[ent("m1", i as f64)]);
        }
        assert_eq!(hud_line().unwrap(), "REC 0:02");
        let out = run_command(&dir, dim, &["stop", "My Run"]);
        assert!(out.say[0].starts_with("Saved myrun: 0:02"), "{:?}", out.say);
        assert_eq!(run_command(&dir, dim, &["list"]).say, ["Recordings: myrun"]);
        assert!(run_command(&dir, "minecraft:the_end", &["play", "myrun"]).say[0].contains("go there first"));
        let out = run_command(&dir, dim, &["play", "myrun"]);
        assert_eq!(out.needs_dimension.as_deref(), Some(dim));
        assert!(is_playing());
        assert!(run_command(&dir, dim, &["start"]).say[0].contains("End the playback"));
        assert_eq!(run_command(&dir, dim, &["speed", "9"]).say, ["Speed is 1 to 8"]);
        assert_eq!(run_command(&dir, dim, &["speed", "4"]).say, ["Speed x4"]);
        let skip = run_command(&dir, dim, &["skip", "1"]);
        assert_eq!(skip.say, ["At 0:01"]);
        assert!(!skip.events.is_empty());
        let end = run_command(&dir, dim, &["end"]);
        assert!(end.events.contains(&Event::Block { pos: [0, 0, 0], old: 0, new: 1 }) || end.events.contains(&Event::Despawn("m1".into())));
        assert!(!is_playing());
        assert_eq!(run_command(&dir, dim, &["delete", "myrun"]).say, ["Deleted myrun"]);
        // Playing to the end cleans up by itself.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn playback_ends_on_its_own() {
        STATE.with(|s| *s.borrow_mut() = State::Playing(Playback::new("t", sample())));
        let mut all = Vec::new();
        for _ in 0..10 {
            all.extend(playback_tick());
        }
        assert!(!is_playing());
        assert!(all.contains(&Event::Block { pos: [1, 2, 3], old: 0, new: 10 }));
    }

    #[test]
    fn binds_ids() {
        bind("m1", 42);
        assert_eq!(live_id("m1"), Some(42));
        assert_eq!(unbind("m1"), Some(42));
        assert_eq!(live_id("m1"), None);
    }
}
