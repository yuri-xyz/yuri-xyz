//! Blackbody colors, tone mapping, and sRGB encoding.

use crate::vec3::Vec3;

const TABLE_SIZE: usize = 512;
const T_MIN: f32 = 800.0;
const T_MAX: f32 = 40_000.0;

/// Asymmetric Gaussian lobe used by the CIE 1931 fit.
fn lobe(lambda: f32, mu: f32, s1: f32, s2: f32) -> f32 {
  let s = if lambda < mu { s1 } else { s2 };
  let t = (lambda - mu) / s;
  (-0.5 * t * t).exp()
}

/// CIE 1931 2° observer, multi-lobe fit by Wyman, Sloan & Shirley (2013).
fn cie_xyz(lambda: f32) -> Vec3 {
  Vec3::new(
    1.056 * lobe(lambda, 599.8, 37.9, 31.0) + 0.362 * lobe(lambda, 442.0, 16.0, 26.7)
      - 0.065 * lobe(lambda, 501.1, 20.4, 26.2),
    0.821 * lobe(lambda, 568.8, 46.9, 40.5) + 0.286 * lobe(lambda, 530.9, 16.3, 31.1),
    1.217 * lobe(lambda, 437.0, 11.8, 36.0) + 0.681 * lobe(lambda, 459.0, 26.0, 13.8),
  )
}

fn xyz_to_linear_srgb(c: Vec3) -> Vec3 {
  Vec3::new(
    3.2406 * c.x - 1.5372 * c.y - 0.4986 * c.z,
    -0.9689 * c.x + 1.8758 * c.y + 0.0415 * c.z,
    0.0557 * c.x - 0.2040 * c.y + 1.0570 * c.z,
  )
}

/// Linear sRGB chromaticity of a blackbody, normalised to unit luminance.
fn blackbody_chroma(temperature: f32) -> Vec3 {
  const C2: f32 = 1.4388e7; // second radiation constant, nm·K
  let mut xyz = Vec3::default();
  let mut lambda: f32 = 380.0;
  while lambda <= 780.0 {
    let planck = lambda.powi(-5) / ((C2 / (lambda * temperature)).exp() - 1.0);
    xyz = xyz + cie_xyz(lambda) * planck;
    lambda += 5.0;
  }
  let rgb = xyz_to_linear_srgb(xyz * (1.0 / xyz.y)).map(|c| c.max(0.0));
  rgb * (1.0 / luminance(rgb))
}

pub fn luminance(c: Vec3) -> f32 {
  0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

/// Precomputed blackbody chromaticities on a logarithmic temperature axis.
pub struct Blackbody {
  table: Vec<Vec3>,
}

impl Blackbody {
  pub fn new() -> Self {
    let table = (0..TABLE_SIZE)
      .map(|i| blackbody_chroma(Self::temperature_at(i as f32)))
      .collect();
    Self { table }
  }

  fn temperature_at(index: f32) -> f32 {
    let t = index / (TABLE_SIZE - 1) as f32;
    T_MIN * (T_MAX / T_MIN).powf(t)
  }

  pub fn chroma(&self, temperature: f32) -> Vec3 {
    let t = (temperature.clamp(T_MIN, T_MAX) / T_MIN).ln() / (T_MAX / T_MIN).ln();
    let f = t * (TABLE_SIZE - 1) as f32;
    let i = (f as usize).min(TABLE_SIZE - 2);
    let w = f - i as f32;
    self.table[i] * (1.0 - w) + self.table[i + 1] * w
  }
}

/// ACES filmic curve fit by Krzysztof Narkowicz.
fn aces_curve(x: f32) -> f32 {
  ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// Per-channel ACES: bright colours burn out to white, like hot gas.
pub fn aces(c: Vec3) -> Vec3 {
  c.map(aces_curve)
}

/// ACES on the brightest channel, keeping hue so saturated colours survive;
/// only the very brightest light drifts towards white.
pub fn aces_hue(c: Vec3) -> Vec3 {
  let peak = c.max_element();
  if peak <= 0.0 {
    return Vec3::default();
  }
  let mapped = aces_curve(peak);
  let toward_white = 0.6 * smoothstep(0.8, 3.0, peak);
  c * (mapped / peak * (1.0 - toward_white)) + Vec3::new(mapped, mapped, mapped) * toward_white
}

pub fn linear_to_srgb(x: f32) -> f32 {
  if x <= 0.003_130_8 {
    12.92 * x
  } else {
    1.055 * x.powf(1.0 / 2.4) - 0.055
  }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
  let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
  t * t * (3.0 - 2.0 * t)
}
