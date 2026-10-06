//! Starting up, as a list of stages: what the loader is doing, what is done and how long each
//! took. The loader (a thread of its own) says when it goes from one to the next; whoever shows
//! it (`splash`) reads a copy. How long each stage took last time is kept (`out/arranque.json`)
//! so that the next time the whole can be shown going forward evenly.
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

/// A stage of starting up: what the loader calls it, how it is written and how long it takes if
/// nothing better is known (s).
pub struct StageDef {
    pub id: &'static str,
    pub name: &'static str,
    pub secs: f32,
}

/// The stages, in the order they happen.
pub const STAGES: &[StageDef] = &[
    StageDef { id: "defs", name: "DEFINICIONES", secs: 4.3 },
    StageDef { id: "render", name: "MOTOR GRÁFICO", secs: 1.6 },
    StageDef { id: "modelos", name: "MODELOS", secs: 2.0 },
    StageDef { id: "mundo", name: "MUNDO Y TRÁFICO", secs: 4.1 },
    StageDef { id: "equipo", name: "ESTRUCTURAS Y EQUIPO", secs: 1.4 },
    StageDef { id: "naves", name: "NAVES Y TRAJE", secs: 0.2 },
    StageDef { id: "sonido", name: "SONIDO E INTERFAZ", secs: 0.1 },
];

/// How a stage is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Waiting,
    /// Under way for so long (s).
    Running(f32),
    /// Done in so long (s).
    Done(f32),
}

/// What whoever shows the start reads: every stage, how much of the whole is done (0..1, even
/// in time) and how long it has been (s).
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub stages: Vec<Stage>,
    pub done: f32,
    pub secs: f32,
    pub finished: bool,
}

struct Inner {
    began: Instant,
    /// When the stage under way began, and which it is.
    at: Instant,
    now: usize,
    took: Vec<f32>,
    /// How long each is expected to take (s).
    expect: Vec<f32>,
}

/// The start under way. Cloned freely: the loader has one end, the screen the other.
#[derive(Clone)]
pub struct Boot(Arc<Mutex<Inner>>);

impl Boot {
    /// A start beginning now; `last`: where the times of the last one are kept, if anywhere.
    pub fn new(last: Option<&Path>) -> Boot {
        let mut expect: Vec<f32> = STAGES.iter().map(|s| s.secs).collect();
        if let Some(text) = last.and_then(|p| std::fs::read_to_string(p).ok())
            && let Ok(v) = serde_json::from_str::<std::collections::BTreeMap<String, f32>>(&text)
        {
            for (k, s) in STAGES.iter().enumerate() {
                if let Some(t) = v.get(s.id).filter(|t| t.is_finite() && **t > 0.0) {
                    expect[k] = *t;
                }
            }
        }
        let now = Instant::now();
        Boot(Arc::new(Mutex::new(Inner { began: now, at: now, now: 0, took: Vec::new(), expect })))
    }

    /// The stage called `id` is done (the ones before it too): the next one begins.
    pub fn done(&self, id: &str) {
        let Some(k) = STAGES.iter().position(|s| s.id == id) else { return };
        let Ok(mut b) = self.0.lock() else { return };
        let t = b.at.elapsed().as_secs_f32();
        while b.took.len() <= k {
            // (stages gone through without a word took no time of their own)
            let own = if b.took.len() == k { t } else { 0.0 };
            b.took.push(own);
        }
        b.now = k + 1;
        b.at = Instant::now();
    }

    pub fn snapshot(&self) -> Snapshot {
        let Ok(b) = self.0.lock() else { return Snapshot { stages: Vec::new(), done: 0.0, secs: 0.0, finished: false } };
        let running = b.at.elapsed().as_secs_f32();
        let stages: Vec<Stage> = (0..STAGES.len()).map(|k| if k < b.took.len() { Stage::Done(b.took[k]) } else if k == b.now { Stage::Running(running) } else { Stage::Waiting }).collect();
        Snapshot { done: share(&stages, &b.expect), secs: b.began.elapsed().as_secs_f32(), finished: b.now >= STAGES.len(), stages }
    }

    /// What each stage took this time, kept for the next start.
    pub fn save(&self, path: &Path) {
        let Ok(b) = self.0.lock() else { return };
        let map: std::collections::BTreeMap<&str, f32> = STAGES.iter().zip(&b.took).map(|(s, t)| (s.id, (*t * 1000.0).round() / 1000.0)).collect();
        if let (Some(dir), Ok(text)) = (path.parent(), serde_json::to_string_pretty(&map)) {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(path, text);
        }
    }
}

/// How much of the whole is done (0..1): each stage counts as long as it is expected to take;
/// the one under way, as far as its time has gone — never quite all of it until it says so (a
/// stage slower than last time creeps on, it does not stand still).
pub fn share(stages: &[Stage], expect: &[f32]) -> f32 {
    let total: f32 = expect.iter().sum::<f32>().max(1e-3);
    let mut done = 0.0;
    for (s, e) in stages.iter().zip(expect) {
        done += match s {
            Stage::Done(_) => *e,
            Stage::Running(t) => *e * (1.0 - (-t / e.max(0.05) * 1.6).exp()) * 0.97,
            Stage::Waiting => 0.0,
        };
    }
    (done / total).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_go_by_in_order_and_the_share_never_goes_back() {
        let b = Boot::new(None);
        let s = b.snapshot();
        assert!(matches!(s.stages[0], Stage::Running(_)) && s.stages[1..].iter().all(|x| *x == Stage::Waiting) && !s.finished);
        let mut last = s.done;
        for (k, st) in STAGES.iter().enumerate() {
            b.done(st.id);
            let s = b.snapshot();
            assert!(s.done >= last, "after {}: {} < {last}", st.id, s.done);
            last = s.done;
            assert!(s.stages[..=k].iter().all(|x| matches!(x, Stage::Done(_))));
            assert_eq!(s.finished, k + 1 == STAGES.len());
        }
        assert!((last - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_stage_under_way_counts_as_far_as_its_time_has_gone_and_never_whole() {
        let expect = [2.0, 2.0];
        let at = |t: f32| share(&[Stage::Running(t), Stage::Waiting], &expect);
        assert!(at(0.0) == 0.0 && at(1.0) > 0.2 && at(1.0) < at(2.0) && at(2.0) < at(60.0));
        assert!(at(600.0) < 0.5, "a stage that never ends does not pass for done");
        assert!((share(&[Stage::Done(9.0), Stage::Running(0.0)], &expect) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_stage_skipped_is_done_with_the_one_after_it() {
        let b = Boot::new(None);
        b.done(STAGES[2].id);
        let s = b.snapshot();
        assert!(s.stages[..3].iter().all(|x| matches!(x, Stage::Done(_))) && matches!(s.stages[3], Stage::Running(_)));
        // an unknown name changes nothing
        b.done("nada");
        assert_eq!(b.snapshot().stages[..3], s.stages[..3]);
    }

    #[test]
    fn the_times_of_a_start_are_kept_for_the_next() {
        let dir = std::env::temp_dir().join(format!("luna_arranque_{}", std::process::id()));
        let path = dir.join("arranque.json");
        let b = Boot::new(None);
        for st in STAGES {
            b.done(st.id);
        }
        b.save(&path);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(STAGES.iter().all(|s| text.contains(s.id)), "{text}");
        // (read back: a start that knows what the last one took)
        let _ = Boot::new(Some(&path));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
