//! What the ships look like this frame: their props (panels, screens, lettering, rams), their
//! lamps and the lights the renderer gets of them, seen from the eye. The ships themselves (their
//! systems, what they do) are the game's (`lunar_play::ships`); this only draws them.
use crate::visibility::Visibility;
use glam::DVec3;
use lunar_core::props::{Lamp, PropScene};
use lunar_play::{builds::Builds, ships::Ships};
use lunar_render::{Light, Renderer};

/// Ships farther than this (m) show no props; lamps farther than this light nothing.
const PROPS_REACH: f64 = 70.0;
const LAMPS_REACH: f64 = 260.0;
/// How near a structure that is no ship shows what is written on it (m).
const LABELS_REACH: f64 = 40.0;

#[derive(Default)]
pub struct ShipsView {
    scene: PropScene,
    lamps: Vec<Lamp>,
    lights: Vec<Light>,
    order: Vec<(f64, usize)>,
    /// Lamps that are not of any ship, to light with the ships' this frame (the helmets of the
    /// other players): taken (and emptied) by `show`.
    pub guests: Vec<Lamp>,
}

impl ShipsView {
    /// This frame's props and lamps to the renderer, seen from `eye`; `extra` adds its own props
    /// (the spawner's ghost).
    /// `own` is the player's own lamp, lit before any other. Ships `vis` hides show nothing but
    /// the lamps that light their outside; seen from outside past twice their size, their inside
    /// lamps light nothing (their inside is not drawn).
    pub fn show(&mut self, ships: &Ships, r: &mut Renderer, builds: &Builds, eye: DVec3, own: Option<Lamp>, vis: &Visibility, extra: impl FnOnce(&mut PropScene)) {
        self.scene.clear();
        self.lamps.clear();
        for sh in &ships.list {
            let Some(s) = builds.set.list.iter().find(|s| s.id == sh.structure) else { continue };
            let to_center = s.to_world(s.center).distance(eye);
            let d = to_center - f64::from(s.radius);
            if d > LAMPS_REACH {
                continue;
            }
            let props = self.scene.props.len();
            let glyphs = self.scene.glyphs.len();
            let decals = self.scene.decals.len();
            let lamps = self.lamps.len();
            sh.scene(s, &builds.set.lib.catalog, &ships.font, eye, &mut self.scene, &mut self.lamps);
            let hidden = vis.is_hidden(s.id);
            if d > PROPS_REACH || hidden {
                // far or hidden: its lamps only
                self.scene.props.truncate(props);
                self.scene.glyphs.truncate(glyphs);
                self.scene.decals.truncate(decals);
            }
            if hidden || to_center > 2.0 * f64::from(s.radius) {
                let mut k = lamps;
                for i in lamps..self.lamps.len() {
                    if !self.lamps[i].inside {
                        self.lamps.swap(k, i);
                        k += 1;
                    }
                }
                self.lamps.truncate(k);
            }
        }
        // what is written on what is loose or built near the eye: a drum out of its hold keeps
        // its lettering (the ships' own was laid out with them)
        let mut placed = Vec::new();
        for s in &builds.set.list {
            if s.to_world(s.center).distance(eye) - f64::from(s.radius) > LABELS_REACH || vis.is_hidden(s.id) || ships.by_structure(s.id).is_some() {
                continue;
            }
            let frame = self.scene.frames.len() as u16;
            self.scene.frames.push(lunar_core::props::PropFrame { pos: s.pos, rot: s.rot, inside: false });
            if lunar_core::structure::labels::show(s, &builds.set.lib.catalog, &ships.font, s.to_local(eye), |_| frame, &mut placed, &mut self.scene) == 0 {
                self.scene.frames.pop();
            }
        }
        extra(&mut self.scene);
        r.set_props(&self.scene);
        self.lamps.append(&mut self.guests);
        // the lamps that matter most here: near and bright first
        self.order.clear();
        for (k, l) in self.lamps.iter().enumerate() {
            let dist = l.pos.distance(eye);
            let power = f64::from(l.color[0] + l.color[1] + l.color[2]) * f64::from(l.range);
            self.order.push((dist / power.max(1e-3).sqrt(), k));
        }
        self.order.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.lights.clear();
        if let Some(l) = own {
            self.lights.push(Light { pos: l.pos, color: l.color, range: l.range, dir: l.dir.to_array(), cone: l.cone, inside: l.inside, everywhere: true });
        }
        for &(_, k) in self.order.iter().take(r.lamp_room().saturating_sub(self.lights.len())) {
            let l = &self.lamps[k];
            self.lights.push(Light { pos: l.pos, color: l.color, range: l.range, dir: l.dir.to_array(), cone: l.cone, inside: l.inside, everywhere: false });
        }
        r.set_lamps(&self.lights);
    }

}
