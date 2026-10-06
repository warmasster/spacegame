//! Movements written in data, key by key: a clip is so many seconds long and has channels — a
//! name and its keys, each a time and up to four numbers (an angle, a point, a share) — and
//! events, names said as their time goes by (a click, "loaded"). Whoever plays one reads the
//! channels it knows by name and does with the numbers what they mean to it; nothing here knows
//! what a hatch or a hand is.
use serde::Deserialize;
use std::collections::BTreeMap;

/// How a channel goes from one key to the next.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    /// Slow out of a key and slow into the next (things with weight moved by a hand).
    #[default]
    Suave,
    Lineal,
    /// Slow out, fast in (something let fall, a latch snapping shut).
    Entra,
    /// Fast out, slow in (something thrown, pushed and left to stop).
    Sale,
    /// Past the key and back (something springy brought up against its stop).
    Rebote,
    /// It holds each key until the next (a switch).
    Salto,
}

impl Ease {
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Suave => t * t * (3.0 - 2.0 * t),
            Ease::Lineal => t,
            Ease::Entra => t * t,
            Ease::Sale => 1.0 - (1.0 - t) * (1.0 - t),
            Ease::Rebote => {
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            Ease::Salto => 0.0,
        }
    }
}

/// A channel as written: its keys alone (`[[t, a, b, c], ...]`), or with how it goes between
/// them (`{ "curva": "entra", "claves": [...] }`).
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ChannelDef {
    Keys(Vec<Vec<f32>>),
    Curved { curva: Ease, claves: Vec<Vec<f32>> },
}

/// A clip as written (whoever has more to say of a movement than channels and events writes
/// these four among its own and makes the clip with `Clip::try_from`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipDef {
    pub dura: f32,
    #[serde(default)]
    pub bucle: bool,
    #[serde(default)]
    pub canales: BTreeMap<String, ChannelDef>,
    #[serde(default)]
    pub eventos: Vec<(f32, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub ease: Ease,
    /// Time, then its numbers (those it does not give are 0).
    pub keys: Vec<(f32, [f32; 4])>,
}

impl Channel {
    /// Its numbers at `t` s: its first key's before it, its last's after it.
    pub fn at(&self, t: f32) -> [f32; 4] {
        let Some(first) = self.keys.first() else { return [0.0; 4] };
        if t <= first.0 {
            return first.1;
        }
        for w in self.keys.windows(2) {
            let ((t0, a), (t1, b)) = (w[0], w[1]);
            if t < t1 {
                let k = self.ease.at((t - t0) / (t1 - t0).max(1e-6));
                return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
            }
        }
        self.keys[self.keys.len() - 1].1
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(try_from = "ClipDef")]
pub struct Clip {
    pub secs: f32,
    pub looped: bool,
    pub channels: BTreeMap<String, Channel>,
    /// In the order of their times.
    pub events: Vec<(f32, String)>,
}

impl TryFrom<ClipDef> for Clip {
    type Error = String;

    fn try_from(d: ClipDef) -> Result<Clip, String> {
        if !(d.dura > 0.0) {
            return Err("un clip dura más que nada".into());
        }
        let mut channels = BTreeMap::new();
        for (name, c) in d.canales {
            let (ease, keys) = match c {
                ChannelDef::Keys(k) => (Ease::default(), k),
                ChannelDef::Curved { curva, claves } => (curva, claves),
            };
            let mut out = Vec::with_capacity(keys.len());
            for k in keys {
                if k.is_empty() || k.len() > 5 {
                    return Err(format!("canal '{name}': una clave es un tiempo y de uno a cuatro números"));
                }
                if out.last().is_some_and(|l: &(f32, [f32; 4])| k[0] < l.0) {
                    return Err(format!("canal '{name}': las claves van en orden de tiempo"));
                }
                out.push((k[0], std::array::from_fn(|i| k.get(i + 1).copied().unwrap_or(0.0))));
            }
            if out.is_empty() {
                return Err(format!("canal '{name}': sin claves"));
            }
            channels.insert(name, Channel { ease, keys: out });
        }
        let mut events = d.eventos;
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Clip { secs: d.dura, looped: d.bucle, channels, events })
    }
}

impl Clip {
    /// Channel `name` at `t` s, if it has it.
    pub fn at(&self, name: &str, t: f32) -> Option<[f32; 4]> {
        self.channels.get(name).map(|c| c.at(t))
    }

    /// The same, or `or` if it has no such channel.
    pub fn value(&self, name: &str, t: f32, or: [f32; 4]) -> [f32; 4] {
        self.at(name, t).unwrap_or(or)
    }

    /// The events whose time is in `(from, to]` (those at 0 are in the first step, from < 0).
    pub fn events(&self, from: f32, to: f32) -> impl Iterator<Item = &str> {
        self.events.iter().filter(move |e| e.0 > from && e.0 <= to).map(|e| e.1.as_str())
    }
}

/// A clip being played: which (by its name in whoever owns the clips) and how far along.
#[derive(Clone, Debug, PartialEq)]
pub struct Playing {
    pub clip: String,
    pub t: f32,
    /// Where it was before the last step (events between the two are due).
    pub before: f32,
}

impl Playing {
    pub fn start(clip: &str) -> Playing {
        Playing { clip: clip.to_string(), t: 0.0, before: -1.0 }
    }

    /// `dt` s on. False once it is over (a looped one never is).
    pub fn step(&mut self, clip: &Clip, dt: f32) -> bool {
        // (its first step takes in what is due at 0)
        self.before = if self.before < 0.0 && self.t == 0.0 { -1.0 } else { self.t };
        self.t += dt;
        if clip.looped {
            if self.t >= clip.secs {
                self.t -= clip.secs;
                self.before -= clip.secs;
            }
            true
        } else {
            self.t < clip.secs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> Clip {
        serde_json::from_str(
            r#"{ "dura": 2.0,
                 "canales": {
                    "puerta": [[0.2, 0], [0.6, 80], [1.6, 80], [1.9, 0]],
                    "mano": { "curva": "lineal", "claves": [[0, 0, 0, 0], [1, 1, 2, 3]] },
                    "luz": { "curva": "salto", "claves": [[0, 0], [1.5, 1]] }
                 },
                 "eventos": [[1.9, "cerrar"], [0.2, "abrir"], [0.0, "empieza"]] }"#,
        )
        .unwrap()
    }

    #[test]
    fn channels_go_from_key_to_key_and_hold_at_their_ends() {
        let c = clip();
        assert_eq!(c.at("puerta", 0.0).unwrap()[0], 0.0);
        assert_eq!(c.at("puerta", 1.0).unwrap()[0], 80.0);
        assert!((c.at("puerta", 0.4).unwrap()[0] - 40.0).abs() < 1e-3, "half way through a smooth move it is half way");
        assert!(c.at("puerta", 0.3).unwrap()[0] < 20.0, "and it starts slowly");
        assert_eq!(c.at("puerta", 5.0).unwrap()[0], 0.0);
        assert_eq!(c.at("mano", 0.5).unwrap(), [0.5, 1.0, 1.5, 0.0]);
        assert_eq!((c.at("luz", 1.49).unwrap()[0], c.at("luz", 1.5).unwrap()[0]), (0.0, 1.0));
        assert!(c.at("nada", 0.0).is_none());
        assert_eq!(c.value("nada", 0.0, [7.0; 4]), [7.0; 4]);
    }

    #[test]
    fn events_are_said_once_as_their_time_goes_by() {
        let c = clip();
        let mut p = Playing::start("x");
        let mut said = Vec::new();
        while p.step(&c, 1.0 / 60.0) {
            said.extend(c.events(p.before, p.t).map(|s| (s.to_string(), p.t)));
        }
        said.extend(c.events(p.before, p.t).map(|s| (s.to_string(), p.t)));
        let names: Vec<&str> = said.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(names, ["empieza", "abrir", "cerrar"], "{said:?}");
        assert!(p.t >= 2.0);
    }

    #[test]
    fn what_is_badly_written_is_refused() {
        assert!(serde_json::from_str::<Clip>(r#"{ "dura": 0 }"#).is_err());
        assert!(serde_json::from_str::<Clip>(r#"{ "dura": 1, "canales": { "a": [[0.5, 1], [0.2, 2]] } }"#).is_err());
        assert!(serde_json::from_str::<Clip>(r#"{ "dura": 1, "canales": { "a": [] } }"#).is_err());
        assert!(serde_json::from_str::<Clip>(r#"{ "dura": 1, "canales": { "a": [[0, 1, 2, 3, 4, 5]] } }"#).is_err());
    }

    #[test]
    fn eases_start_at_nothing_and_end_at_all() {
        for e in [Ease::Suave, Ease::Lineal, Ease::Entra, Ease::Sale, Ease::Rebote] {
            assert!(e.at(0.0).abs() < 1e-5 && (e.at(1.0) - 1.0).abs() < 1e-5, "{e:?}");
        }
        assert!(Ease::Rebote.at(0.8) > 1.0, "it goes past and comes back");
        assert!(Ease::Entra.at(0.5) < 0.5 && Ease::Sale.at(0.5) > 0.5);
    }
}
