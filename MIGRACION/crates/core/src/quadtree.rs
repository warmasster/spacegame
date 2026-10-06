//! Whole-body CDLOD selection, CPU only: six quadtrees over the cube-sphere, an LRU cache of
//! generated nodes (slots), the nodes to draw this frame and the ones to generate next. The GPU
//! terrain only uploads what this decides, so tests drive it without a GPU.
use crate::{
    body::Body,
    cube_sphere::{cube_arc, face_of},
    surface::Surface,
    terrain_gen::NodeKey,
};
use glam::DVec3;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadConfig {
    /// Cells per node side.
    pub grid: u32,
    /// Generated nodes kept (texture slots).
    pub capacity: u32,
    /// Finest cell (m): sets the deepest level.
    pub finest_cell: f64,
}

struct Cached {
    slot: u32,
    center: DVec3,
    radius: f64,
    last_used: u64,
    /// Queued for regeneration (once, however many changes touch it).
    stale: bool,
}

/// A node drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct Selected {
    pub key: NodeKey,
    pub slot: u32,
    /// Bounding sphere (world).
    pub center: DVec3,
    pub radius: f64,
}

/// Share of the split distance nodes off screen are refined to: the view gets the cache and the
/// generation budget; what is behind is one level coarser (it only casts shadows) until it turns
/// into view.
pub const OFF_VIEW_SPLIT: f64 = 0.5;

/// Bounding radius of a node on undisturbed ground: its extent, its relief and its skirt.
fn bound(arc: f64, grid: u32) -> f64 {
    arc * 0.78 + (arc * 0.3).min(4000.0) + skirt(arc, grid)
}

/// Skirt drop under a node's borders (m): hides the cracks the morph does not close.
pub fn skirt(arc: f64, grid: u32) -> f64 {
    arc / f64::from(grid) * 2.5 + 0.3
}

pub struct Quadtree {
    center: DVec3,
    radius: f64,
    horizon_depth: f64,
    surface: Arc<dyn Surface>,
    grid: u32,
    capacity: usize,
    max_level: u32,
    /// Share of the asked split distance the cache can hold: shrinks while wanted nodes find no
    /// room (LOD drops evenly instead of leaving coarse holes), grows back when there is room.
    scale: f64,
    nodes: HashMap<NodeKey, Cached>,
    free: Vec<u32>,
    requests: Vec<(f64, NodeKey)>,
    evict: Vec<(u64, NodeKey)>,
    stack: Vec<NodeKey>,
    frame: u64,
    /// Drawn this frame.
    pub selected: Vec<Selected>,
    /// To generate now: the nodes created this frame (key, slot).
    pub created: Vec<(NodeKey, u32)>,
    /// Cached nodes whose ground changed: regenerate them when a batch has room.
    stale: Vec<NodeKey>,
    /// Wanted nodes not generated yet.
    pub pending: usize,
    pub deepest: u32,
    /// Nodes dropped from the cache so far.
    pub evicted: u64,
}

impl Quadtree {
    /// The six roots are queued in `created` straight away.
    pub fn new(body: &Body, surface: Arc<dyn Surface>, cfg: QuadConfig) -> Quadtree {
        let capacity = cfg.capacity.max(6);
        let max_level = (cube_arc(2., body.radius) / (f64::from(cfg.grid) * cfg.finest_cell)).log2().ceil().max(0.0) as u32;
        let mut q = Quadtree {
            center: body.center,
            radius: body.radius,
            horizon_depth: body.horizon_depth,
            surface,
            grid: cfg.grid,
            capacity: capacity as usize,
            scale: 1.0,
            max_level: max_level.min(30),
            nodes: HashMap::with_capacity(capacity as usize),
            free: (0..capacity).rev().collect(),
            requests: Vec::with_capacity(1024),
            evict: Vec::with_capacity(capacity as usize),
            stack: Vec::with_capacity(256),
            frame: 0,
            selected: Vec::with_capacity(capacity as usize),
            created: Vec::with_capacity(64),
            stale: Vec::with_capacity(256),
            pending: 0,
            deepest: 0,
            evicted: 0,
        };
        for face in 0..6 {
            q.create(NodeKey::root(face));
        }
        q
    }

    /// The split distance (in node widths) used this frame: the asked one, scaled to the cache.
    pub fn split(&self, asked: f64) -> f64 {
        asked * self.scale
    }

    pub fn max_level(&self) -> u32 {
        self.max_level
    }

    pub fn cached(&self) -> usize {
        self.nodes.len()
    }

    fn create(&mut self, key: NodeKey) -> bool {
        let Some(slot) = self.free.pop() else {
            return false;
        };
        let arc = key.arc(self.radius);
        let frame = key.frame();
        let h = self.surface.sample(frame.dir.to_array(), arc / f64::from(self.grid)).height;
        let center = self.center + frame.dir * (self.radius + h);
        let radius = bound(arc, self.grid);
        self.nodes.insert(key, Cached { slot, center, radius, last_used: self.frame, stale: false });
        self.created.push((key, slot));
        true
    }

    /// Choose this frame's nodes from `eye`, request missing detail and create up to `budget`
    /// nodes, the most urgent first. `in_view(rel, radius)` says whether a bounding sphere
    /// (camera-relative) is on screen: those refine first.
    pub fn update(&mut self, eye: DVec3, split: f64, budget: usize, in_view: impl Fn(DVec3, f64) -> bool) {
        let split = self.split(split);
        self.frame += 1;
        self.requests.clear();
        self.selected.clear();
        self.created.clear();
        self.deepest = 0;
        self.stack.clear();
        for face in (0..6).rev() {
            self.stack.push(NodeKey::root(face));
        }
        let mc = self.center;
        let cam_rel = eye - mc;
        let dc = cam_rel.length();
        let r_occ = self.radius - self.horizon_depth;
        let cam_horizon = if dc > r_occ { (r_occ / dc).acos() } else { std::f64::consts::PI };
        let cam_dir = cam_rel / dc;
        let frame = self.frame;
        while let Some(key) = self.stack.pop() {
            let Some(n) = self.nodes.get_mut(&key) else {
                continue;
            };
            n.last_used = frame;
            let (center, radius, slot) = (n.center, n.radius, n.slot);
            // behind the horizon: neither drawn nor refined
            let to_node = center - mc;
            let dn = to_node.length();
            let angle = cam_dir.dot(to_node / dn).clamp(-1., 1.).acos();
            let reach = cam_horizon + (r_occ / (dn + radius).max(r_occ)).acos() + (radius / dn).min(1.0);
            if angle > reach && dc > r_occ {
                continue;
            }
            let rel = center - eye;
            let arc = key.arc(self.radius);
            let dist = rel.length() - radius;
            let seen = in_view(rel, radius);
            if u32::from(key.level) < self.max_level && dist < split * arc * if seen { 1.0 } else { OFF_VIEW_SPLIT } {
                let kids = [0, 1, 2, 3].map(|k| key.child(k));
                if kids.iter().all(|k| self.nodes.contains_key(k)) {
                    self.stack.extend(kids);
                    continue;
                }
                // the kids already made stay cached while their siblings are generated: dropping
                // them (they are not drawn yet) would keep this corner coarse for ever
                let priority = (dist.max(0.0) + 1.0) / arc * if seen { 1.0 } else { 4.0 };
                for k in kids {
                    match self.nodes.get_mut(&k) {
                        Some(c) => c.last_used = frame,
                        None => self.requests.push((priority, k)),
                    }
                }
            }
            self.deepest = self.deepest.max(u32::from(key.level));
            self.selected.push(Selected { key, slot, center, radius });
        }
        // generate: most urgent first, evicting the least recently used when full
        self.requests.sort_by(|a, b| a.0.total_cmp(&b.0));
        let want = budget.min(self.requests.len());
        if self.free.len() < want {
            self.evict.clear();
            self.evict.extend(self.nodes.iter().filter(|(k, n)| n.last_used < frame && k.level > 0).map(|(k, n)| (n.last_used, *k)));
            self.evict.sort_unstable_by_key(|e| (e.0, std::cmp::Reverse(e.1.level)));
            for i in 0..(want - self.free.len()).min(self.evict.len()) {
                if let Some(n) = self.nodes.remove(&self.evict[i].1) {
                    self.free.push(n.slot);
                    self.evicted += 1;
                }
            }
        }
        let mut starved = false;
        for i in 0..want {
            let key = self.requests[i].1;
            if !self.nodes.contains_key(&key) && !self.create(key) {
                starved = true;
                break;
            }
        }
        self.pending = self.requests.len() - self.created.len().min(self.requests.len());
        if !self.stale.is_empty() {
            // regenerate what is drawn first, nearest first (popped from the end)
            let nodes = &self.nodes;
            self.stale.sort_by_cached_key(|k| nodes.get(k).map_or((false, 0), |n| (n.last_used == frame, u64::MAX - n.center.distance(eye) as u64)));
        }
        if starved {
            self.scale = (self.scale * 0.97).max(0.2);
        } else if self.pending == 0 && self.nodes.len() * 5 < self.capacity * 4 {
            self.scale = (self.scale * 1.005).min(1.0);
        }
    }

    /// The ground of node `key` lies up to `moved` m off the generated surface (blast craters):
    /// its bounding sphere grows to hold it. Otherwise a node deep in a crater would be culled
    /// where its ground really is, and the hole would show the skirts of its neighbours.
    pub fn set_moved(&mut self, key: NodeKey, moved: f64) {
        let arc = key.arc(self.radius);
        if let Some(n) = self.nodes.get_mut(&key) {
            n.radius = bound(arc, self.grid) + moved.max(0.0);
        }
    }

    /// Mark every cached node whose ground the region (unit direction, radius m) touches; they
    /// are regenerated into the same slots (`next_stale`), the old data drawn meanwhile.
    pub fn invalidate(&mut self, dir: DVec3, radius: f64) {
        for (key, n) in &mut self.nodes {
            // measured at the node's own height: the ground can be kilometres off the datum
            // sphere, far more than a fine node's bounding radius
            let p = self.center + dir * (n.center - self.center).length();
            if !n.stale && n.center.distance(p) < n.radius + radius {
                n.stale = true;
                self.stale.push(*key);
            }
        }
    }

    /// Every cached node is stale.
    pub fn invalidate_all(&mut self) {
        for (key, n) in &mut self.nodes {
            if !n.stale {
                n.stale = true;
                self.stale.push(*key);
            }
        }
    }

    /// A stale node still cached: the drawn ones nearest the camera first (`update` orders them).
    pub fn next_stale(&mut self) -> Option<(NodeKey, u32)> {
        while let Some(k) = self.stale.pop() {
            if let Some(n) = self.nodes.get_mut(&k).filter(|n| n.stale) {
                n.stale = false;
                return Some((k, n.slot));
            }
        }
        None
    }

    pub fn stale_len(&self) -> usize {
        self.stale.len()
    }

    /// The drawn node holding unit direction `d` (None: none drawn there).
    pub fn selected_at(&self, d: DVec3) -> Option<&Selected> {
        let (face, [a, b]) = face_of(d.to_array());
        self.selected.iter().find(|s| {
            if usize::from(s.key.face) != face {
                return false;
            }
            let (ca, cb) = s.key.center_params();
            let h = s.key.size() * 0.5;
            (a - ca).abs() <= h && (b - cb).abs() <= h
        })
    }

    /// Largest level step between drawn neighbours: what the morph cannot close above 1.
    pub fn worst_neighbour_step(&self) -> u32 {
        let mut worst = 0;
        for s in &self.selected {
            let f = s.key.frame();
            let h = s.key.size() * 0.5 * 1.0001;
            for (da, db) in [(h, 0.0), (-h, 0.0), (0.0, h), (0.0, -h)] {
                let d = (f.dir + f.delta(da, db)).normalize();
                if let Some(o) = self.selected_at(d) {
                    worst = worst.max(u32::from(s.key.level.abs_diff(o.key.level)));
                }
            }
        }
        worst
    }
}
