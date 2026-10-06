//! G: the catalog of everything that can be set in the world (ships with their systems, and every
//! structure blueprint), and the ghost that shows where it goes. On the ground of any body under
//! the crosshair, or floating ahead where there is no ground (orbit, deep space). Wheel turns it,
//! Shift+wheel moves it nearer or farther in free space, click sets it, right click or G drops it.
use crate::{builds::Builds, ships::Ships};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    body::{BodyId, BodyRegistry},
    props::{BOX, Prop, PropFrame, PropScene},
    scene::basis,
    structure::state::Structure,
};

/// Farthest ground the ghost goes to (m); free-space distance limits.
const GROUND_REACH: f64 = 600.0;
const FREE_MIN: f64 = 8.0;
const FREE_MAX: f64 = 2000.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Ship(String),
    Build(String),
}

struct Entry {
    pick: Pick,
    name: String,
    /// Bounds in its own frame (lowest point, highest point) and radius.
    lo: Vec3,
    hi: Vec3,
    radius: f32,
}

/// Where the ghost is: on a body's ground (`dir` from its centre, `yaw` from north) or free.
#[derive(Clone, Copy, Debug)]
enum Spot {
    Ground { body: BodyId, dir: DVec3, yaw: f64 },
    Free { pos: DVec3, rot: Quat },
}

pub struct Spawner {
    /// The catalog window is open (the mouse is free for it).
    pub open: bool,
    entries: Vec<Entry>,
    search: String,
    placing: Option<usize>,
    yaw: f64,
    dist: f64,
    spot: Option<(Spot, DVec3, Quat, bool)>,
    pub message: Option<String>,
    /// Ships only (demo).
    ships_only: bool,
}

impl Spawner {
    pub fn new(ships: &Ships, builds: &Builds, ships_only: bool) -> Spawner {
        let lib = &builds.set.lib;
        let bounds = |bp: &str| -> Option<(Vec3, Vec3, f32)> {
            let bp = lib.blueprint(bp)?;
            let s = Structure::new(0, bp, &lib.catalog, DVec3::ZERO, Quat::IDENTITY);
            let (lo, hi) = s.parts.iter().flat_map(|p| p.shape.verts().map(|v| p.local.transform_point3(v))).fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            Some((lo, hi, s.radius))
        };
        let mut entries = Vec::new();
        for k in &ships.kinds {
            if let Some((lo, hi, radius)) = bounds(&k.blueprint) {
                entries.push(Entry { pick: Pick::Ship(k.id.clone()), name: k.def.nombre.clone(), lo, hi, radius });
            }
        }
        if !ships_only {
            for (id, bp) in &lib.blueprints {
                if ships.kinds.iter().any(|k| k.blueprint == *id) {
                    continue;
                }
                if let Some((lo, hi, radius)) = bounds(id) {
                    entries.push(Entry { pick: Pick::Build(id.clone()), name: bp.name.clone(), lo, hi, radius });
                }
            }
        }
        Spawner { open: false, entries, search: String::new(), placing: None, yaw: 0.0, dist: 40.0, spot: None, message: None, ships_only }
    }

    /// G: open the catalog, or drop what is being placed.
    pub fn toggle(&mut self) {
        if self.placing.is_some() {
            self.placing = None;
            self.spot = None;
        } else {
            self.open = !self.open;
        }
    }

    pub fn placing(&self) -> bool {
        self.placing.is_some()
    }

    pub fn cancel(&mut self) {
        self.placing = None;
        self.spot = None;
        self.open = false;
    }

    /// Wheel while placing: turn it (or, with Shift, nearer / farther in free space).
    pub fn wheel(&mut self, notches: f64, shift: bool) -> bool {
        if self.placing.is_none() {
            return false;
        }
        if shift {
            self.dist = (self.dist * 1.15f64.powf(notches)).clamp(FREE_MIN, FREE_MAX);
        } else {
            self.yaw += notches * 15f64.to_radians();
        }
        true
    }

    /// The catalog window. True when something was picked (the mouse goes back to the game).
    pub fn window(&mut self, ctx: &egui::Context) -> bool {
        let mut picked = false;
        let mut open = self.open;
        let title = if self.ships_only { "NAVES  ·  G" } else { "CATÁLOGO  ·  G" };
        let shown = egui::Window::new(title).frame(crate::hud::frame()).open(&mut open).default_pos([60.0, 80.0]).default_width(340.0).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Buscar:");
                ui.text_edit_singleline(&mut self.search);
            });
            ui.separator();
            let q = self.search.to_lowercase();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for (i, e) in self.entries.iter().enumerate() {
                    let (tag, id) = match &e.pick {
                        Pick::Ship(id) => ("nave", id),
                        Pick::Build(id) => ("estructura", id),
                    };
                    if !q.is_empty() && !e.name.to_lowercase().contains(&q) && !id.to_lowercase().contains(&q) {
                        continue;
                    }
                    let size = e.hi - e.lo;
                    let text = format!("{}  ·  {tag}  ·  {:.0}×{:.0}×{:.0} m", e.name, size.x, size.y, size.z);
                    if ui.selectable_label(self.placing == Some(i), text).clicked() {
                        self.placing = Some(i);
                        self.yaw = 0.0;
                        picked = true;
                    }
                }
            });
            ui.separator();
            ui.label("Elige y apunta: clic para colocar · rueda: girar · Mayús+rueda: distancia en el espacio · clic derecho o G: soltar");
        });
        if let Some(shown) = shown {
            crate::hud::dress(ctx, shown.response.rect);
        }
        self.open = open && !picked;
        picked
    }

    /// Where the ghost goes this frame, from the eye along the crosshair; `ahead` is the way the
    /// player faces (world).
    pub fn update(&mut self, bodies: &BodyRegistry, builds: &Builds, eye: DVec3, fwd: DVec3, up: DVec3, ahead: DVec3) {
        let Some(i) = self.placing else { return };
        let e = &self.entries[i];
        let spot = match bodies.raycast(eye, fwd, GROUND_REACH) {
            Some((body, at)) => {
                let b = bodies.get(body);
                let dir = (at - b.center).normalize();
                // (as `Structures::place` counts a turn: from the body's north toward east)
                let north = b.turn_from(dir);
                let east = north.cross(dir);
                let yaw = ahead.dot(east).atan2(ahead.dot(north)) + self.yaw;
                let rot = basis(dir, north * yaw.cos() + east * yaw.sin());
                let pos = b.center + dir * (b.radius + b.height(dir)) - (rot * Vec3::Y * e.lo.y).as_dvec3();
                (Spot::Ground { body, dir, yaw }, pos, rot)
            }
            None => {
                let pos = eye + fwd * (self.dist + f64::from(e.radius));
                let f = fwd - up * fwd.dot(up);
                let (s, c) = self.yaw.sin_cos();
                let side = f.cross(up).normalize_or(up.any_orthonormal_vector());
                let rot = basis(up, f.normalize_or(side) * c + side * s);
                (Spot::Free { pos, rot }, pos, rot)
            }
        };
        // nothing else where it would stand
        let centre = spot.1 + (spot.2 * ((e.lo + e.hi) * 0.5)).as_dvec3();
        let r = (e.hi - e.lo).length() * 0.4;
        let clear = builds.set.list.iter().all(|s| s.to_world(s.center).distance(centre) > f64::from(r + s.radius * 0.6)) && eye.distance(centre) > f64::from(r);
        self.spot = Some((spot.0, spot.1, spot.2, clear));
    }

    /// The ghost: its bounding box drawn in light (green when it fits, red when not).
    pub fn ghost(&self, out: &mut PropScene) {
        let (Some(i), Some((_, pos, rot, ok))) = (self.placing, self.spot) else { return };
        let e = &self.entries[i];
        let frame = out.frames.len() as u16;
        out.frames.push(PropFrame { pos, rot, inside: false });
        let color = if ok { [80, 255, 140] } else { [255, 70, 60] };
        let (lo, hi) = (e.lo, e.hi);
        let c = (lo + hi) * 0.5;
        let h = (hi - lo) * 0.5;
        let t = (h.length() * 0.006).clamp(0.02, 0.2);
        let mut edge = |pos: Vec3, size: Vec3| {
            out.props.push(Prop { frame, mesh: BOX, pos, rot: Quat::IDENTITY, size, color, emissive: 3.0, rough: 200, metal: 0 });
        };
        for (sy, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            edge(c + Vec3::new(0.0, sy * h.y, sz * h.z), Vec3::new(h.x * 2.0, t, t));
        }
        for (sx, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            edge(c + Vec3::new(sx * h.x, 0.0, sz * h.z), Vec3::new(t, h.y * 2.0, t));
        }
        for (sx, sy) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            edge(c + Vec3::new(sx * h.x, sy * h.y, 0.0), Vec3::new(t, t, h.z * 2.0));
        }
        // the nose
        edge(Vec3::new(c.x, c.y, hi.z + t * 6.0), Vec3::splat(t * 6.0));
    }

    /// Click: set it there. Returns the structure made.
    pub fn place(&mut self, ships: &mut Ships, builds: &mut Builds, bodies: &BodyRegistry) -> Option<u64> {
        let (Some(i), Some((spot, _, _, ok))) = (self.placing, self.spot) else { return None };
        if !ok {
            self.message = Some("No cabe ahí".into());
            return None;
        }
        let e = &self.entries[i];
        let made = match (&e.pick, spot) {
            (Pick::Ship(id), Spot::Ground { body, dir, yaw }) => ships.spawn(builds, bodies, id, body, dir, yaw),
            (Pick::Ship(id), Spot::Free { pos, rot }) => ships.spawn_free(builds, id, pos, rot),
            (Pick::Build(id), Spot::Ground { body, dir, yaw }) => builds.set.place(id, bodies, body, dir, yaw, 0.0),
            (Pick::Build(id), Spot::Free { pos, rot }) => builds.set.spawn(id, pos, rot),
        };
        match made {
            Ok(sid) => {
                self.message = Some(format!("{} colocada", e.name));
                Some(sid)
            }
            Err(err) => {
                self.message = Some(err);
                None
            }
        }
    }
}
