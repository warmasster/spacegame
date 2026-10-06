//! What the app hands the renderer: instanced models, effects and structures.
use super::Renderer;
use glam::DVec3;
use lunar_core::{
    model::Animation,
    particles::{Particle, Particles, StyleDef},
    structure::{Library, state::Structure},
};
use crate::{globals::MAX_LIGHTS, scene::SceneGpu, uniforms::Light};

impl Renderer {
    /// Instanced objects: register models, place instances.
    pub fn scene(&mut self) -> &mut SceneGpu {
        &mut self.scene
    }

    /// Baked skeletal animation shared by the skinned models.
    pub fn set_animation(&mut self, a: &Animation) {
        self.scene.set_animation(&self.gpu.device, &self.gpu.queue, a.joints, &a.frames, &a.clips);
    }

    /// Replace every instance: model, world position, rotation, scale, flags.
    pub fn set_instances(&mut self, desc: &[(u32, DVec3, glam::Quat, f32, u32)]) {
        self.scene.set_instances(&self.gpu.device, desc);
    }

    /// How particles look (index = `Particle::style`).
    pub fn set_particle_styles(&self, styles: &[StyleDef]) {
        self.particles.set_styles(&self.gpu.queue, styles);
    }

    /// Particles alive at once on the GPU (the settings' budget).
    pub fn particle_capacity(&self) -> usize {
        self.particles.capacity()
    }

    /// This frame's effects, seen from `eye`: the particles, `extra` drawn as particles (rounds in flight) and the flashes.
    pub fn set_effects(&mut self, particles: &Particles, extra: &[Particle], lights: &[Light], eye: DVec3) {
        self.particles.upload(&self.gpu.queue, particles, extra, eye);
        self.flash_count = lights.len().min(MAX_LIGHTS);
        self.light_count = self.flash_count;
        self.lights[..self.light_count].copy_from_slice(&lights[..self.light_count]);
    }

    /// How exhaust plumes look (index = `Plume::style`).
    pub fn set_plume_styles(&self, styles: &lunar_core::plumes::Styles) {
        self.plumes.set_styles(&self.gpu.queue, styles);
    }

    /// This frame's exhaust plumes, seen from `eye` (none: nothing is sent nor drawn).
    pub fn set_plumes(&mut self, plumes: &[lunar_core::plumes::Plume], eye: DVec3) {
        self.plumes.upload(&self.gpu.queue, plumes, eye);
    }

    /// More lights of the moment after the explosions' flashes (a jet's glow on what is round
    /// it): call after `set_effects` and before `set_lamps`; as many as fit.
    pub fn add_flashes(&mut self, lights: &[Light]) {
        let n = lights.len().min(MAX_LIGHTS - self.flash_count);
        self.lights[self.flash_count..self.flash_count + n].copy_from_slice(&lights[..n]);
        self.flash_count += n;
        self.light_count = self.flash_count;
    }

    /// Lamps of structures this frame, after the flashes (call after `set_effects`): as many as
    /// fit, in the order given (nearest first).
    pub fn set_lamps(&mut self, lamps: &[Light]) {
        let room = MAX_LIGHTS - self.flash_count;
        let n = lamps.len().min(room);
        self.lights[self.flash_count..self.flash_count + n].copy_from_slice(&lamps[..n]);
        self.light_count = self.flash_count + n;
    }

    /// How many lamps fit after this frame's flashes.
    pub fn lamp_room(&self) -> usize {
        MAX_LIGHTS - self.flash_count
    }

    /// A model props may be instances of (`Prop::mesh`: the number this gives; None, no room).
    pub fn prop_mesh(&mut self, mesh: &lunar_core::mesh::Mesh) -> Option<u8> {
        self.props.add_mesh(&self.gpu.device, mesh)
    }

    /// A mesh bodies may be drawn as (`anim::BodyDraw::mesh`: the number this gives; None if its
    /// vertices follow no bones).
    pub fn body_mesh(&mut self, mesh: &lunar_core::mesh::Mesh) -> Option<u16> {
        self.figures.add_mesh(&self.gpu.device, mesh)
    }

    /// This frame's bodies and every bone of each.
    pub fn set_bodies(&mut self, scene: &lunar_core::anim::BodyScene) {
        self.figures.set(scene);
    }

    /// This frame's props (controls, gauges, screens, rods) and text.
    pub fn set_props(&mut self, scene: &crate::props::PropScene) {
        self.props.set(scene);
    }

    /// The structures' finishes: `layers` RGBA8 textures of `size`² (`mesh::FINISHES` from 1).
    pub fn set_finishes(&mut self, size: u32, layers: u32, data: &[u8]) {
        self.structures.set_finishes(&self.gpu.device, &self.gpu.queue, size, layers, data);
    }

    /// The decal atlas (posters, signs, logos): `size`² RGBA8 sRGB.
    pub fn set_decal_atlas(&mut self, size: u32, data: &[u8]) {
        self.props.set_decals(&self.gpu.device, &self.gpu.queue, size, data);
    }

    /// The silkscreen font atlas (R8 signed distances).
    pub fn set_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        self.props.set_atlas(&self.gpu.device, &self.gpu.queue, width, height, data);
    }

    /// This frame's structures seen from `eye` (the changed ones are remeshed).
    pub fn set_structures(&mut self, list: &[Structure], lib: &Library, eye: DVec3) {
        self.structures.sync(&self.gpu.device, &self.gpu.queue, list, lib, eye);
    }

    /// Structures shown by their integrity this frame (the scanner's view): what is sound a dim
    /// shape, what is damaged glowing red by how much, what is gone purple where it should be.
    pub fn set_integrity_view(&mut self, ids: &[u64]) {
        self.structures.set_integrity(ids);
    }

    /// Looks of structures on the GPU (shared by a blueprint's, a structure's own), looks meshed
    /// and part words written since the start: what breaking things costs, for tools and tests.
    pub fn structure_looks(&self) -> (usize, usize, u64, u64) {
        let (shared, own) = self.structures.looks();
        (shared, own, self.structures.meshed, self.structures.words_written)
    }

    /// Structures not seen this frame (behind the ground, inside a closed hull): only their
    /// shadows are drawn.
    pub fn set_hidden_structures(&mut self, ids: &[u64]) {
        self.structures.set_hidden(ids);
    }

    /// The level of detail structure `id` was drawn with last frame (0 full), if drawn.
    pub fn structure_level(&self, id: u64) -> Option<u8> {
        self.structures.shown.iter().find(|(s, _)| *s == id).map(|&(_, l)| l)
    }

    /// Tool: structures tinted by their level of detail.
    pub fn set_lod_tint(&mut self, on: bool) {
        self.structures.tint_levels = on;
    }

    pub fn lod_tint(&self) -> bool {
        self.structures.tint_levels
    }

    /// Vertices of structure looks on the GPU: all, and of full looks.
    pub fn structure_pool(&self) -> (u64, u64) {
        self.structures.pool_use()
    }
}
