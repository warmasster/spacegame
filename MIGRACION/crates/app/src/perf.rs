//! What a frame costs, part by part. Always measured (a few timestamps a frame; F3 shows them);
//! recorded on request (`perfil` in a camera script) and written out as JSON with a summary:
//! mean, median, 95th and 99th percentile and worst of the frame, the mean and worst of each
//! part, and how much the renderer had to draw and rebuild. The tests of performance
//! (`tools/rendimiento/*.jsonc`) are scripts that fill the world with ships, make them fall and
//! shoot at them, and record this.
use serde::Serialize;
use std::{fmt::Write as _, path::PathBuf, time::Instant};

/// The parts of a frame, in the order they run.
pub const PARTS: [&str; 8] = ["mundo", "disparos", "naves", "fisica", "aire", "visibilidad", "escena", "render"];
pub const WORLD: usize = 0;
pub const SHOTS: usize = 1;
pub const SHIPS: usize = 2;
pub const PHYSICS: usize = 3;
pub const AIR: usize = 4;
pub const VISIBILITY: usize = 5;
pub const SCENE: usize = 6;
pub const RENDER: usize = 7;

/// One frame.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Sample {
    /// The whole frame (ms), from one to the next.
    pub ms: f32,
    /// Each part (ms), `PARTS`.
    pub parts: [f32; 8],
    /// The GPU's passes together (ms), when it times them.
    pub gpu: f32,
    pub structures: u32,
    /// The ones that moved as bodies this frame (loose, not asleep, not held).
    pub awake: u32,
    pub ships: u32,
    /// Ships whose systems ran every tick.
    pub ships_full: u32,
    pub draws: u32,
    pub triangles: u64,
    /// Looks meshed and part words written since the start.
    pub meshed: u64,
    pub words: u64,
}

pub struct Perf {
    /// This frame's parts so far (ms).
    pub now: [f32; 8],
    mark: Instant,
    rec: Option<(PathBuf, String, Vec<Sample>)>,
}

impl Default for Perf {
    fn default() -> Perf {
        Perf { now: [0.0; 8], mark: Instant::now(), rec: None }
    }
}

#[derive(Serialize)]
struct Report<'a> {
    nombre: &'a str,
    resumen: Summary,
    partes: Vec<Part>,
    fotogramas: &'a [Sample],
}

#[derive(Serialize)]
struct Summary {
    fotogramas: usize,
    segundos: f32,
    fps: f32,
    ms_media: f32,
    ms_mediana: f32,
    ms_p95: f32,
    ms_p99: f32,
    ms_peor: f32,
    gpu_ms_media: f32,
    estructuras_max: u32,
    despiertas_media: f32,
    naves_max: u32,
    naves_a_todo_ritmo_max: u32,
    dibujos_max: u32,
    triangulos_max: u64,
    mallas_rehechas: u64,
    palabras_de_pieza_escritas: u64,
}

#[derive(Serialize)]
struct Part {
    parte: &'static str,
    ms_media: f32,
    ms_peor: f32,
}

impl Perf {
    /// A frame starts.
    pub fn begin(&mut self) {
        self.now = [0.0; 8];
        self.mark = Instant::now();
    }

    /// What ran since the last mark was `part`.
    pub fn lap(&mut self, part: usize) {
        let t = Instant::now();
        self.now[part] += (t - self.mark).as_secs_f32() * 1e3;
        self.mark = t;
    }

    pub fn recording(&self) -> bool {
        self.rec.is_some()
    }

    /// Record from now into `path` (JSON), under `name`.
    pub fn start(&mut self, path: PathBuf, name: &str) {
        self.rec = Some((path, name.to_string(), Vec::new()));
    }

    /// The frame that just ended.
    pub fn sample(&mut self, s: Sample) {
        if let Some((_, _, list)) = &mut self.rec {
            list.push(s);
        }
    }

    /// Stop, write the file and give the summary as text.
    pub fn stop(&mut self) -> Option<String> {
        let (path, name, list) = self.rec.take()?;
        if list.is_empty() {
            return Some(format!("perfil {name}: sin fotogramas"));
        }
        let n = list.len();
        let mut ms: Vec<f32> = list.iter().map(|s| s.ms).collect();
        ms.sort_by(f32::total_cmp);
        let pct = |p: f32| ms[((n as f32 * p) as usize).min(n - 1)];
        let total: f32 = ms.iter().sum();
        let mean = |f: &dyn Fn(&Sample) -> f32| list.iter().map(f).sum::<f32>() / n as f32;
        let (first, last) = (list[0], list[n - 1]);
        let summary = Summary {
            fotogramas: n,
            segundos: total / 1000.0,
            fps: n as f32 * 1000.0 / total.max(1e-3),
            ms_media: total / n as f32,
            ms_mediana: pct(0.5),
            ms_p95: pct(0.95),
            ms_p99: pct(0.99),
            ms_peor: ms[n - 1],
            gpu_ms_media: mean(&|s| s.gpu),
            estructuras_max: list.iter().map(|s| s.structures).max().unwrap_or(0),
            despiertas_media: mean(&|s| s.awake as f32),
            naves_max: list.iter().map(|s| s.ships).max().unwrap_or(0),
            naves_a_todo_ritmo_max: list.iter().map(|s| s.ships_full).max().unwrap_or(0),
            dibujos_max: list.iter().map(|s| s.draws).max().unwrap_or(0),
            triangulos_max: list.iter().map(|s| s.triangles).max().unwrap_or(0),
            mallas_rehechas: last.meshed - first.meshed,
            palabras_de_pieza_escritas: last.words - first.words,
        };
        let parts: Vec<Part> = (0..PARTS.len()).map(|k| Part { parte: PARTS[k], ms_media: mean(&|s| s.parts[k]), ms_peor: list.iter().map(|s| s.parts[k]).fold(0.0, f32::max) }).collect();
        let mut text = format!(
            "perfil {name}: {} fotogramas en {:.1} s = {:.1} FPS; fotograma {:.2} ms de media (mediana {:.2}, p95 {:.2}, p99 {:.2}, peor {:.2}); GPU {:.2} ms\n  hasta {} estructuras ({:.1} despiertas de media), {} naves ({} a todo ritmo), {} dibujos, {} triángulos; mallas rehechas {}, palabras de pieza {}\n ",
            summary.fotogramas,
            summary.segundos,
            summary.fps,
            summary.ms_media,
            summary.ms_mediana,
            summary.ms_p95,
            summary.ms_p99,
            summary.ms_peor,
            summary.gpu_ms_media,
            summary.estructuras_max,
            summary.despiertas_media,
            summary.naves_max,
            summary.naves_a_todo_ritmo_max,
            summary.dibujos_max,
            summary.triangulos_max,
            summary.mallas_rehechas,
            summary.palabras_de_pieza_escritas
        );
        for p in &parts {
            let _ = write!(text, " {} {:.2} (peor {:.1})", p.parte, p.ms_media, p.ms_peor);
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let report = Report { nombre: &name, resumen: summary, partes: parts, fotogramas: &list };
        match serde_json::to_string(&report) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    let _ = write!(text, "\n  ERROR al escribir {}: {e}", path.display());
                }
            }
            Err(e) => {
                let _ = write!(text, "\n  ERROR: {e}");
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_sums_up_its_frames() {
        let mut p = Perf::default();
        assert!(p.stop().is_none());
        let path = std::env::temp_dir().join("lunar_perf_test.json");
        p.start(path.clone(), "prueba");
        for k in 0..100u32 {
            let mut s = Sample { ms: 10.0 + (k % 10) as f32, structures: k, meshed: u64::from(k / 50), ..Sample::default() };
            s.parts[PHYSICS] = 2.0;
            p.sample(s);
        }
        let text = p.stop().unwrap();
        assert!(text.contains("100 fotogramas") && text.contains("fisica 2.00"), "{text}");
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["resumen"]["estructuras_max"], 99);
        assert_eq!(json["resumen"]["mallas_rehechas"], 1);
        assert!((json["resumen"]["ms_media"].as_f64().unwrap() - 14.5).abs() < 1e-3);
        assert_eq!(json["fotogramas"].as_array().unwrap().len(), 100);
        let _ = std::fs::remove_file(path);
    }
}
