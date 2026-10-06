//! Tileable regolith detail (normal xy, albedo factor, cavity), generated once on the CPU in
//! parallel with its whole mip chain; sampled at two scales by the terrain shader.
use rayon::prelude::*;

const SIZE: usize = 512;

fn hash(x: i32, y: i32, s: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4eb2d) ^ (y as u32).wrapping_mul(0x165667b1) ^ s.wrapping_mul(0x9e3779b9);
    h = (h ^ (h >> 15)).wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2ae35);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// Periodic value noise with smooth interpolation; `period` lattice cells over the tile.
fn vnoise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * t * (t * (t * 6. - 15.) + 10.);
    let w = |i: i32| i.rem_euclid(period);
    let h = |i, j| hash(w(i), w(j), seed);
    let (a, b, c, d) = (h(xi, yi), h(xi + 1, yi), h(xi, yi + 1), h(xi + 1, yi + 1));
    let (sx, sy) = (s(fx), s(fy));
    (a + (b - a) * sx) + ((c + (d - c) * sx) - (a + (b - a) * sx)) * sy
}

/// Height (m-ish, relative) at tile coordinates (u, v) in [0, 1).
fn height(u: f32, v: f32) -> (f32, f32) {
    let mut h = 0.0;
    let mut amp = 0.5;
    for (k, p) in [4, 8, 16, 32, 64, 128].iter().enumerate() {
        h += amp * (vnoise(u, v, *p, 11 + k as u32) - 0.5);
        amp *= 0.55;
    }
    // clods: lumpy, billowy regolith
    for (k, p) in [24, 48, 96].iter().enumerate() {
        h += 0.03 / (k + 1) as f32 * (0.5 - (vnoise(u, v, *p, 31 + k as u32) - 0.5).abs() * 2.0);
    }
    // a few faint, soft pits (the real ground close up shows clods and pebbles, not craters)
    let mut albedo = 1.0 + 0.25 * (vnoise(u, v, 16, 91) - 0.5) + 0.12 * (vnoise(u, v, 64, 92) - 0.5);
    for (cells, depth, seed) in [(6, 0.1f32, 101u32), (14, 0.04, 102)] {
        let (x, y) = (u * cells as f32, v * cells as f32);
        let (xi, yi) = (x.floor() as i32, y.floor() as i32);
        for j in yi - 1..=yi + 1 {
            for i in xi - 1..=xi + 1 {
                let (wi, wj) = (i.rem_euclid(cells), j.rem_euclid(cells));
                if hash(wi, wj, seed) > 0.4 {
                    continue;
                }
                let cx = i as f32 + hash(wi, wj, seed + 1);
                let cy = j as f32 + hash(wi, wj, seed + 2);
                let r = 0.18 + 0.3 * hash(wi, wj, seed + 3);
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r;
                if d < 2.0 {
                    let bowl = if d < 1.0 { (d * d - 1.0) * (1.0 - d * d) } else { 0.0 };
                    h += bowl * depth * r;
                }
            }
        }
    }
    // pebbles and small rocks at three sizes: rounded, a little brighter, with a dark footing
    // (from the cavity term) like the photographs
    for (cells, chance, rmin, rmax, seed) in [(64, 0.3f32, 0.12f32, 0.35f32, 201u32), (40, 0.25, 0.12, 0.4, 211), (16, 0.12, 0.15, 0.45, 221)] {
        let (x, y) = (u * cells as f32, v * cells as f32);
        let (xi, yi) = (x.floor() as i32, y.floor() as i32);
        for j in yi - 1..=yi + 1 {
            for i in xi - 1..=xi + 1 {
                let (wi, wj) = (i.rem_euclid(cells), j.rem_euclid(cells));
                if hash(wi, wj, seed) > chance {
                    continue;
                }
                let cx = i as f32 + hash(wi, wj, seed + 1);
                let cy = j as f32 + hash(wi, wj, seed + 2);
                let r = rmin + (rmax - rmin) * hash(wi, wj, seed + 3).powi(2);
                // slightly elongated, randomly turned
                let a = hash(wi, wj, seed + 4) * std::f32::consts::TAU;
                let (dx, dy) = (x - cx, y - cy);
                let (px, py) = (dx * a.cos() + dy * a.sin(), (-dx * a.sin() + dy * a.cos()) * 1.35);
                let d2 = (px * px + py * py) / (r * r);
                if d2 < 1.0 {
                    let b = (1.0 - d2).sqrt();
                    h += b * r / cells as f32 * 12.0;
                    albedo += b * 0.12;
                }
            }
        }
    }
    (h, albedo)
}

/// RGBA8 mip chain (level 0 first).
pub fn generate() -> Vec<Vec<u8>> {
    let n = SIZE;
    let field: Vec<(f32, f32)> = (0..n * n).into_par_iter().map(|k| height((k % n) as f32 / n as f32, (k / n) as f32 / n as f32)).collect();
    let at = |x: i32, y: i32| field[(y.rem_euclid(n as i32) as usize) * n + x.rem_euclid(n as i32) as usize];
    // gradient scale: the heights are in tile units; steepen for visible relief
    let k = n as f32 * 0.12;
    let mut level: Vec<[f32; 4]> = (0..n * n)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % n) as i32, (i / n) as i32);
            let dx = (at(x + 1, y).0 - at(x - 1, y).0) * k;
            let dy = (at(x, y + 1).0 - at(x, y - 1).0) * k;
            let nrm = glam::Vec3::new(-dx, -dy, 1.0).normalize();
            let blur = (at(x + 3, y).0 + at(x - 3, y).0 + at(x, y + 3).0 + at(x, y - 3).0) * 0.25;
            let cavity = (1.0 + (at(x, y).0 - blur) * n as f32 * 0.3).clamp(0.7, 1.0);
            [nrm.x, nrm.y, at(x, y).1, cavity]
        })
        .collect();
    let encode = |l: &[[f32; 4]]| -> Vec<u8> {
        l.iter()
            .flat_map(|p| {
                [
                    ((p[0] * 0.5 + 0.5) * 255.0).round() as u8,
                    ((p[1] * 0.5 + 0.5) * 255.0).round() as u8,
                    ((p[2] * 0.5).clamp(0.0, 1.0) * 255.0).round() as u8,
                    (p[3].clamp(0.0, 1.0) * 255.0).round() as u8,
                ]
            })
            .collect()
    };
    let mut mips = vec![encode(&level)];
    let mut size = n;
    while size > 1 {
        let half = size / 2;
        level = (0..half * half)
            .into_par_iter()
            .map(|i| {
                let (x, y) = (i % half, i / half);
                let mut s = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(y * 2 + dy) * size + x * 2 + dx];
                    for c in 0..4 {
                        s[c] += p[c] * 0.25;
                    }
                }
                s
            })
            .collect();
        size = half;
        mips.push(encode(&level));
    }
    mips
}

pub fn texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let mips = generate();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("regolith detail"),
        size: wgpu::Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 },
        mip_level_count: mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (m, data) in mips.iter().enumerate() {
        let s = (SIZE >> m).max(1) as u32;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: m as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
            wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 1 },
        );
    }
    tex.create_view(&Default::default())
}
