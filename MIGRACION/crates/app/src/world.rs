//! The scenario running: the fleet and crowd the scenario declares, laid out round its site, and
//! its traffic (ships going from field to field, `lunar_core::traffic`), simulated in parallel
//! and written straight into the renderer's instance records. No content numbers live here:
//! counts, models and layout come from `scenario.jsonc` (or the command line).
use crate::content::{Defs, Models};
use glam::Quat;
use lunar_core::{
    body::BodyRegistry,
    scene::{ANIM_WALK, Crowd, Fleet, Site},
    traffic::Traffic,
};
use lunar_render::{
    Renderer, View,
    scene::{FLAG_MOVING, FLAG_STATIC},
};
use rayon::prelude::*;
use std::sync::Arc;

/// How many of everything (the scenario's, unless the command line or the menu say otherwise).
#[derive(Clone, Copy, Debug)]
pub struct Counts {
    pub ships: usize,
    pub flying: usize,
    pub npcs: usize,
    pub radius: f64,
}

pub struct World {
    pub bodies: Arc<BodyRegistry>,
    pub site: Site,
    pub fleet: Fleet,
    pub crowd: Crowd,
    /// How many ships of the traffic it draws (the traffic itself is the game's: `Game::traffic`).
    traffic_len: usize,
    moving_from: usize,
    crowd_from: usize,
    traffic_from: usize,
    pub time: f64,
    pub sim_ms: f32,
}

impl World {
    /// The scenario's traffic (if it has any and `on`), ten minutes of it already gone by, so the
    /// sky is busy from the start: the game's to run (`Game::traffic`), this world's to draw.
    pub fn traffic(defs: &Defs, on: bool) -> Result<Option<Traffic>, String> {
        let sc = &defs.scenario;
        let bodies = defs.system.bodies.clone();
        let site = Site::from_def(&sc.site, &bodies)?;
        Ok(match (&sc.trafico, on) {
            (Some(def), true) if !sc.fleet.models.is_empty() => Some(Traffic::new(&bodies, &site, def, sc.fleet.models.len(), 0x7a_f1c0, 600.0)),
            _ => None,
        })
    }

    /// `traffic`: the game's traffic, to draw (`World::traffic`).
    pub fn new(r: &mut Renderer, defs: &Defs, models: &Models, n: Counts, traffic: Option<&Traffic>) -> Result<World, String> {
        let sc = &defs.scenario;
        let bodies = defs.system.bodies.clone();
        let site = Site::from_def(&sc.site, &bodies)?;
        let layout = lunar_core::scenario::Layout { radius: n.radius, ..sc.layout };
        let fleet_models = sc.fleet.models.iter().map(|m| models.get(m)).collect::<Result<Vec<_>, _>>()?;
        let npc_model = models.get(&sc.crowd.model)?;
        let fleet = Fleet::new(&bodies, &site, &sc.fleet, &layout, n.ships, n.flying);
        let crowd = Crowd::new(&bodies, &site, &sc.crowd, &layout, n.npcs);
        let mut desc = Vec::with_capacity(n.ships + n.npcs);
        for k in 0..fleet.len() {
            let flags = if k < fleet.parked { FLAG_STATIC } else { FLAG_MOVING };
            desc.push((fleet_models[usize::from(fleet.kind[k]) % fleet_models.len().max(1)], fleet.pos[k], fleet.rot[k], 1.0, flags));
        }
        for k in 0..crowd.len() {
            desc.push((npc_model, crowd.pos[k], Quat::IDENTITY, 1.0, FLAG_MOVING));
        }
        let traffic_from = desc.len();
        if let Some(t) = traffic {
            for k in 0..t.len() {
                desc.push((fleet_models[usize::from(t.kind[k]) % fleet_models.len()], t.pos[k], t.rot[k], 1.0, FLAG_MOVING));
            }
        }
        r.set_instances(&desc);
        let w = World { bodies, site, moving_from: fleet.parked, crowd_from: fleet.len(), traffic_from, traffic_len: traffic.map_or(0, |t| t.len()), fleet, crowd, time: 0.0, sim_ms: 0.0 };
        w.write(r, traffic);
        Ok(w)
    }

    /// A view of the first parked ship of fleet kind `kind` from `dist` m, at azimuth `az` and
    /// elevation `el` (degrees; 0 = ahead of its nose), aimed a little above its keel.
    pub fn look_at(&self, (kind, dist, az, el): (u8, f64, f64, f64), base: View) -> Option<View> {
        let k = (0..self.fleet.parked).find(|&k| self.fleet.kind[k] == kind)?;
        let (pos, rot) = (self.fleet.pos[k], self.fleet.rot[k].as_dquat());
        let up = self.bodies.get(self.fleet.body[k]).up(pos);
        let (az, el) = (az.to_radians(), el.to_radians());
        let local = glam::DVec3::new(az.sin() * el.cos(), el.sin(), az.cos() * el.cos());
        let target = pos + up * dist * 0.08;
        let eye = target + rot * local * dist;
        Some(View { eye, forward: (target - eye).normalize(), up, ..base })
    }

    /// The frame drawn at `time` (the game's, between its last two steps), `dt` s after the last:
    /// the fleet and the crowd on, and they and the game's traffic into the renderer.
    pub fn update(&mut self, r: &mut Renderer, time: f64, dt: f64, traffic: Option<&Traffic>) {
        let t0 = std::time::Instant::now();
        self.time = time;
        self.fleet.update(&self.bodies, self.time);
        self.crowd.update(&self.bodies, dt);
        self.write(r, traffic);
        self.sim_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    /// Moving instances (flying ships, NPCs) into the renderer, in parallel.
    fn write(&self, r: &mut Renderer, traffic: Option<&Traffic>) {
        if let Some(t) = traffic.filter(|t| t.len() == self.traffic_len) {
            let (world, inst) = r.scene().poses(self.traffic_from..self.traffic_from + t.len());
            world.par_iter_mut().zip(inst.par_iter_mut()).enumerate().with_min_len(256).for_each(|(k, (w, i))| {
                *w = t.pos[k];
                i.rot = t.rot[k].to_array();
            });
        }
        let n = self.fleet.len() + self.crowd.len();
        if n <= self.moving_from {
            return;
        }
        let (world, inst) = r.scene().poses(self.moving_from..n);
        let flying = self.crowd_from - self.moving_from;
        let (ship_w, npc_w) = world.split_at_mut(flying);
        let (ship_i, npc_i) = inst.split_at_mut(flying);
        let parked = self.fleet.parked;
        ship_w.par_iter_mut().zip(ship_i.par_iter_mut()).enumerate().for_each(|(k, (w, i))| {
            *w = self.fleet.pos[parked + k];
            i.rot = self.fleet.rot[parked + k].to_array();
        });
        let c = &self.crowd;
        let bodies = &self.bodies;
        npc_w.par_iter_mut().zip(npc_i.par_iter_mut()).enumerate().with_min_len(128).for_each(|(k, (w, i))| {
            *w = c.pos[k];
            let up = bodies.get(c.body[k]).up(c.pos[k]);
            i.rot = lunar_core::scene::basis(up, c.fwd[k].as_dvec3()).to_array();
            i.extra[0] = c.phase[k];
            i.extra[1] = if c.anim[k] == ANIM_WALK { 0.0 } else { 1.0 };
        });
    }
}
