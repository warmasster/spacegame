//! Damage: how a hit spreads over a structure's parts and joints. A `DamageModel` only says who
//! takes how much (and pushed which way); breaking and fracturing what it hurts is someone else's
//! job (`fracture`, `breakup`), so either side can change alone.
use super::{catalog::Catalog, state::Structure};
use glam::Vec3;

/// A hit on a structure, in its frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    /// Travel direction (unit) of a projectile; ignored by blasts.
    pub dir: Vec3,
    /// Energy (J).
    pub energy: f32,
    /// Blast reach (m); 0 for a projectile.
    pub radius: f32,
    /// Cross-section of a projectile (m²): how much material it meets per metre.
    pub area: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PartHit {
    pub part: u32,
    /// Energy the part takes (J).
    pub energy: f32,
    /// Where (structure frame) and which way it is pushed.
    pub at: Vec3,
    pub push: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct JointHit {
    pub joint: u32,
    pub energy: f32,
}

pub trait DamageModel: Send + Sync {
    /// Who takes how much of `hit` (appended to the lists).
    fn spread(&self, s: &Structure, cat: &Catalog, hit: &Hit, parts: &mut Vec<PartHit>, joints: &mut Vec<JointHit>);
}

/// Blasts: the energy flux at each part (falling with the square of the distance, gone at the
/// reach) times the area it shows the blast, less behind other parts. Projectiles: the parts along
/// the path in order, each soaking up the energy it takes to cut through it.
pub struct StandardDamage {
    /// What one part in the way lets through of a blast.
    pub shadow: f32,
    /// Area of a joint the blast acts on (m²).
    pub joint_area: f32,
}

impl Default for StandardDamage {
    fn default() -> Self {
        StandardDamage { shadow: 0.35, joint_area: 0.2 }
    }
}

impl StandardDamage {
    fn blast(&self, s: &Structure, hit: &Hit, parts: &mut Vec<PartHit>, joints: &mut Vec<JointHit>) {
        let flux = |d: f32| {
            let fall = (1.0 - (d / hit.radius).powi(2)).max(0.0);
            hit.energy / (4.0 * std::f32::consts::PI * d.max(0.5).powi(2)) * fall
        };
        // the parts in reach (the structure's index), each shadowed by what stands before it
        let mut reached: Vec<u32> = Vec::new();
        s.index.sphere(hit.point, hit.radius, |i| reached.push(i));
        reached.sort_unstable();
        for &i in &reached {
            let p = &s.parts[i as usize];
            if !p.alive {
                continue;
            }
            let inv = p.local.inverse();
            let near = p.shape.distance(inv.transform_point3(hit.point)).max(0.0);
            if near > hit.radius {
                continue;
            }
            let to = p.center - hit.point;
            let dir = to.normalize_or(Vec3::Y);
            // what stands between the blast and this part
            let mut through = 1.0;
            let len = to.length();
            let stop = len - p.radius * 0.5;
            if stop > 0.0 {
                s.index.ray(hit.point, dir, stop, |j, reach| {
                    let q = &s.parts[j as usize];
                    if j == i || !q.alive || through < 0.05 {
                        return reach;
                    }
                    let qi = q.local.inverse();
                    if let Some((t, _)) = q.shape.raycast(qi.transform_point3(hit.point), qi.transform_vector3(dir), stop) {
                        if t > 0.0 {
                            through *= self.shadow;
                        }
                    }
                    reach
                });
            }
            let area = p.shape.projected_area(inv.transform_vector3(dir).normalize_or(Vec3::Y));
            let e = flux(near) * area * through;
            if e > 0.0 {
                parts.push(PartHit { part: i, energy: e, at: p.center - dir * p.radius * 0.5, push: dir });
            }
        }
        let r2 = hit.radius * hit.radius;
        for (k, j) in s.joints.iter().enumerate().filter(|(_, j)| j.alive && j.at.distance_squared(hit.point) < r2) {
            let e = flux(j.at.distance(hit.point)) * self.joint_area;
            if e > 0.0 {
                joints.push(JointHit { joint: k as u32, energy: e });
            }
        }
    }

    fn projectile(&self, s: &Structure, cat: &Catalog, hit: &Hit, parts: &mut Vec<PartHit>) {
        // every live part on the path, nearest first, with the length of path inside it
        let first = parts.len();
        s.index.ray(hit.point, hit.dir, 1e4, |i, reach| {
            let p = &s.parts[i as usize];
            if !p.alive {
                return reach;
            }
            let inv = p.local.inverse();
            let (o, d) = (inv.transform_point3(hit.point), inv.transform_vector3(hit.dir));
            let Some((enter, _)) = p.shape.raycast(o, d, 1e4) else {
                return reach;
            };
            let far = enter + p.radius * 2.0 + 1.0;
            let exit = p.shape.raycast(o + d * far, -d, far).map_or(enter, |(t, _)| far - t);
            parts.push(PartHit { part: i, energy: enter, at: hit.point + hit.dir * enter, push: hit.dir * (exit - enter).max(0.01) });
            reach
        });
        parts[first..].sort_by(|a, b| a.energy.total_cmp(&b.energy));
        // each soaks up what cutting its length of path takes, the rest goes on
        let mut left = hit.energy;
        let mut kept = first;
        for k in first..parts.len() {
            let mut h = parts[k];
            let p = &s.parts[h.part as usize];
            let chord = h.push.length();
            let toughness = cat.material(p.kind).toughness * p.fill;
            let soak = (toughness * chord * hit.area).min(left);
            left -= soak;
            h.energy = soak;
            h.push = hit.dir;
            parts[kept] = h;
            kept += 1;
            if left <= 0.0 {
                break;
            }
        }
        parts.truncate(kept);
    }
}

impl DamageModel for StandardDamage {
    fn spread(&self, s: &Structure, cat: &Catalog, hit: &Hit, parts: &mut Vec<PartHit>, joints: &mut Vec<JointHit>) {
        if hit.radius > 0.0 {
            self.blast(s, hit, parts, joints);
        } else {
            self.projectile(s, cat, hit, parts);
        }
    }
}
