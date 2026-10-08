//! A Schwarzschild black hole with a turbulent, Doppler-beamed thin disk.

mod disk;
mod geodesic;

use rayon::prelude::*;

use self::disk::Disk;
use self::geodesic::Path;
use crate::camera;
use crate::color::{aces, luminance, smoothstep};
use crate::post::{LightMode, Style};
use crate::scene::Scene;
use crate::vec3::Vec3;

const CAMERA_DISTANCE: f32 = 58.0;
/// Camera elevation above the disk plane.
const CAMERA_ELEVATION_DEG: f32 = 7.5;
/// Slight roll so the disk does not sit perfectly level.
const CAMERA_ROLL_DEG: f32 = -4.0;
const VERTICAL_FOV_DEG: f32 = 24.0;

/// Light-mode ink ramp from faint glow to the hottest gas, in sRGB. White-hot
/// light is invisible on a white page, so intensity maps to deeper, more
/// opaque ink instead.
const INK: [(f32, Vec3); 5] = [
  (0.0, Vec3::new(1.0, 0.92, 0.76)),
  (0.3, Vec3::new(0.95, 0.78, 0.50)),
  (0.6, Vec3::new(0.82, 0.60, 0.32)),
  (0.85, Vec3::new(0.62, 0.42, 0.20)),
  (1.0, Vec3::new(0.38, 0.25, 0.12)),
];

fn ink(light: Vec3) -> (Vec3, f32) {
  let intensity = luminance(light).powf(1.4).clamp(0.0, 1.0);
  let coverage = smoothstep(0.0, 0.35, intensity);
  for pair in INK.windows(2) {
    let ((x0, c0), (x1, c1)) = (pair[0], pair[1]);
    if intensity <= x1 {
      let t = (intensity - x0) / (x1 - x0);
      return (c0 * (1.0 - t) + c1 * t, coverage);
    }
  }
  (INK[INK.len() - 1].1, coverage)
}

static STYLE: Style = Style {
  exposure: 1.6,
  bloom: &[(3, 0.10), (10, 0.07), (32, 0.05)],
  tonemap: aces,
  light: LightMode::Ink {
    ink,
    // GitHub's light-theme foreground (`#1f2328`), softer than pure black.
    occluder: Vec3::new(0.122, 0.137, 0.157),
  },
};

pub struct BlackHole {
  /// Every subsample ray, traced once: the geometry is static, only the
  /// disk texture changes between frames.
  paths: Vec<[Path; 4]>,
  disk: Disk,
}

impl BlackHole {
  pub fn new(width: usize, height: usize) -> Self {
    let elevation = CAMERA_ELEVATION_DEG.to_radians();
    let origin = Vec3::new(0.0, elevation.sin(), -elevation.cos()) * CAMERA_DISTANCE;
    let forward = (-origin).normalize();
    let roll = CAMERA_ROLL_DEG.to_radians();
    let world_up = Vec3::new(roll.sin(), roll.cos(), 0.0);
    let right = world_up.cross(forward).normalize();
    let up = forward.cross(right);
    let half_h = (VERTICAL_FOV_DEG.to_radians() * 0.5).tan();

    let paths = (0..width * height)
      .into_par_iter()
      .map(|i| {
        camera::subsamples(i, width, height).map(|(u, v)| {
          let dir = forward + right * (u * half_h) + up * (v * half_h);
          geodesic::trace(origin, dir, disk::INNER, disk::OUTER)
        })
      })
      .collect();
    Self {
      paths,
      disk: Disk::new(),
    }
  }
}

impl Scene for BlackHole {
  fn style(&self) -> &'static Style {
    &STYLE
  }

  fn shade(&self, phase: f32) -> (Vec<Vec3>, Vec<f32>) {
    let t = disk::loop_period() * phase;
    self
      .paths
      .par_iter()
      .map(|subsamples| {
        let mut emission = Vec3::default();
        let mut transmittance = 0.0;
        for path in subsamples {
          let mut through = 1.0;
          for hit in path.hits() {
            let s = self.disk.sample(hit, t);
            emission = emission + s.emission * through;
            through *= 1.0 - s.alpha;
          }
          if !path.captured {
            transmittance += through;
          }
        }
        let n = subsamples.len() as f32;
        (emission * (1.0 / n), transmittance / n)
      })
      .unzip()
  }
}
