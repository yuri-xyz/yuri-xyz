//! Hash-based gradient noise and seamless looping.

use crate::vec3::Vec3;

pub fn hash(x: i32, y: i32, z: i32) -> u32 {
  let mut h = (x as u32).wrapping_mul(0x8da6_b343)
    ^ (y as u32).wrapping_mul(0xd816_3841)
    ^ (z as u32).wrapping_mul(0xcb1a_b31f);
  h ^= h >> 15;
  h = h.wrapping_mul(0x2c1b_3c6d);
  h ^= h >> 12;
  h = h.wrapping_mul(0x297a_2d39);
  h ^ (h >> 15)
}

fn gradient(h: u32, x: f32, y: f32, z: f32) -> f32 {
  // Perlin's twelve cube-edge gradients.
  match h % 12 {
    0 => x + y,
    1 => -x + y,
    2 => x - y,
    3 => -x - y,
    4 => x + z,
    5 => -x + z,
    6 => x - z,
    7 => -x - z,
    8 => y + z,
    9 => -y + z,
    10 => y - z,
    _ => -y - z,
  }
}

fn fade(t: f32) -> f32 {
  t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
  a + (b - a) * t
}

/// Signed 3D gradient noise in roughly [-1, 1].
fn perlin(p: Vec3) -> f32 {
  let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
  let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
  let (x, y, z) = (p.x - fx, p.y - fy, p.z - fz);
  let (u, v, w) = (fade(x), fade(y), fade(z));
  let g = |dx: i32, dy: i32, dz: i32| {
    gradient(
      hash(ix + dx, iy + dy, iz + dz),
      x - dx as f32,
      y - dy as f32,
      z - dz as f32,
    )
  };
  lerp(
    lerp(
      lerp(g(0, 0, 0), g(1, 0, 0), u),
      lerp(g(0, 1, 0), g(1, 1, 0), u),
      v,
    ),
    lerp(
      lerp(g(0, 0, 1), g(1, 0, 1), u),
      lerp(g(0, 1, 1), g(1, 1, 1), u),
      v,
    ),
    w,
  )
}

pub fn fbm(mut p: Vec3, octaves: u32) -> f32 {
  let mut sum = 0.0;
  let mut amp = 0.5;
  for _ in 0..octaves {
    sum += amp * perlin(p);
    p = p * 2.03 + Vec3::new(17.3, -9.1, 4.7);
    amp *= 0.5;
  }
  sum
}

/// Uniform value in [0, 1) derived from a lattice hash.
pub fn hash01(x: i32, y: i32, z: i32) -> f32 {
  (hash(x, y, z) >> 8) as f32 / (1u32 << 24) as f32
}

/// Makes a zero-mean field loop seamlessly over `phase` in [0, 1).
///
/// `field(cycle)` evaluates the field shifted by `cycle` whole loops. The
/// current cycle is cross-faded into the previous one, which equals cycle 0
/// again at phase 1, and the blend is renormalised so the field's variance,
/// and therefore its contrast, stays constant through the fade.
pub fn looping(phase: f32, field: impl Fn(f32) -> f32) -> f32 {
  let w = phase;
  let (a, b) = (field(0.0), field(-1.0));
  (a * (1.0 - w) + b * w) / ((1.0 - w).powi(2) + w * w).sqrt()
}
