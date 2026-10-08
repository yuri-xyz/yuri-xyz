//! Accretion disk appearance: turbulent density, emission, and opacity.

use std::f32::consts::TAU;

use super::geodesic::Hit;
use crate::color::{Blackbody, smoothstep};
use crate::noise::{fbm, looping};
use crate::vec3::Vec3;

/// Innermost stable circular orbit.
pub const INNER: f32 = 6.0;
pub const OUTER: f32 = 22.0;

/// Coordinate time covered by one loop: exactly one ISCO orbit, so the
/// inner rim's pattern also lines up on its own.
pub fn loop_period() -> f32 {
    TAU * INNER.powf(1.5)
}

/// Peak disk temperature in kelvin before Doppler shifting.
const PEAK_TEMPERATURE: f32 = 5200.0;
/// Exponent on the frequency shift applied to brightness (relativistic
/// beaming); the physical value is 4, softened for a readable image.
const BEAMING: f32 = 3.0;

/// Keplerian angular velocity.
fn omega(r: f32) -> f32 {
    r.powf(-1.5)
}

/// Turbulence sampled in the frame co-rotating with the gas at time `t`.
fn turbulence(r: f32, phi: f32, t: f32) -> f32 {
    let psi = phi - omega(r) * t;
    // Azimuthally stretched, radially fine: gas sheared into streaks.
    let ring = 2.2;
    let p = Vec3::new(psi.cos() * ring, psi.sin() * ring, r.ln() * 9.0);
    fbm(p, 5)
}

/// Radiated flux of a thin disk (Novikov–Thorne shape, Schwarzschild ISCO).
fn flux(r: f32) -> f32 {
    r.powi(-3) * (1.0 - (INNER / r).sqrt()).max(0.0)
}

/// Emitted radiance and opacity of one disk crossing.
pub struct Sample {
    pub emission: Vec3,
    pub alpha: f32,
}

pub struct Disk {
    blackbody: Blackbody,
    peak_flux: f32,
}

impl Disk {
    pub fn new() -> Self {
        // The thin-disk flux peaks at r = 49/6 for a Schwarzschild ISCO of 6.
        Self {
            blackbody: Blackbody::new(),
            peak_flux: flux(49.0 / 6.0),
        }
    }

    pub fn sample(&self, hit: &Hit, t: f32) -> Sample {
        let r = hit.r;
        let period = loop_period();
        let n = looping(t / period, |cycle| {
            turbulence(r, hit.phi, t + cycle * period)
        });

        let edges = smoothstep(INNER, INNER + 0.6, r) * (1.0 - smoothstep(OUTER - 9.0, OUTER, r));
        let density = edges * smoothstep(-0.35, 0.45, n);
        let alpha = (density * 0.92).clamp(0.0, 1.0);

        let f = flux(r) / self.peak_flux;
        let temperature = PEAK_TEMPERATURE * f.powf(0.25) * hit.g;
        let brightness = f.powf(0.55) * hit.g.powf(BEAMING) * (0.55 + 0.9 * density);
        Sample {
            emission: self.blackbody.chroma(temperature) * (brightness * alpha),
            alpha,
        }
    }
}
