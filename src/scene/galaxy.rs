//! A large spiral galaxy in the likeness of Andromeda, seen nearly edge-on:
//! a warm core, a salmon-to-violet inner disk wrapped in tight dust lanes,
//! and a clumpy blue outer disk, with M32 hanging beside it.
//!
//! The disk is a volume: stars and gas fill a slab that flares outward, with
//! a thinner dust layer inside it, so near-side dust lanes silhouette
//! against the core. Rays march through the slab front to back.
//!
//! The disk spins differentially. Rotation never brings the texture back to
//! where it started, so each frame cross-fades the disk one loop ahead with
//! the disk one loop behind; the per-loop rotation is small enough that the
//! two copies stay nearly aligned and the fade is invisible.

use std::f32::consts::{PI, TAU};

use rayon::prelude::*;

use crate::camera;
use crate::color::{Blackbody, aces_hue, luminance, smoothstep};
use crate::noise::{fbm, hash01};
use crate::post::{LightMode, Style};
use crate::scene::Scene;
use crate::vec3::Vec3;

/// Camera elevation above the galactic plane; Andromeda sits about 13° off
/// edge-on.
const ELEVATION_DEG: f32 = 12.5;
const AZIMUTH_DEG: f32 = 30.0;
/// Tilts the major axis so it rises to the right.
const ROLL_DEG: f32 = 11.0;
/// Orthographic half-height of the frame, in disk radii.
const HALF_HEIGHT: f32 = 0.6;

/// Radius where the spiral arms begin.
const ARM_START: f32 = 0.2;
/// Logarithmic spiral winding, `1 / tan(pitch)`; Andromeda's arms are wound
/// tightly enough to look like nested rings.
const WINDING: f32 = 6.0;
/// Phase offset of dust lanes onto the inner edge of each arm.
const LANE_SHIFT: f32 = 0.35;
/// The star-forming ring about two thirds of the way out.
const RING_RADIUS: f32 = 0.64;
/// Rotation per loop at radius `r` is `SPIN / (r + SPIN_CORE)` radians.
const SPIN: f32 = 0.12;
const SPIN_CORE: f32 = 0.3;

/// Half-thickness of the marched slab, the number of layers through it, and
/// the radius beyond which the disk is empty.
const SLAB: f32 = 0.09;
/// Even, so the midplane falls on a layer boundary.
const LAYERS: usize = 16;
const DISK_EXTENT: f32 = 1.25;
/// Vertical-to-radial axis ratio of the core.
const CORE_FLATTENING: f32 = 0.62;

/// Star lattice spacing in the plane and the on-screen star radius.
const STAR_CELL: f32 = 0.009;
const STAR_RADIUS: f32 = 0.0016;

/// M32: screen position, size, and brightness (screen units are half-heights).
const SATELLITE: (f32, f32) = (-0.52, 0.36);
const SATELLITE_SIZE: f32 = 0.018;
const SATELLITE_BRIGHTNESS: f32 = 2.6;

/// Disk colour from core to rim, linear sRGB: cream, salmon, violet, blue.
const PALETTE: [(f32, Vec3); 5] = [
    (0.0, Vec3::new(1.0, 0.78, 0.55)),
    (0.22, Vec3::new(1.0, 0.52, 0.45)),
    (0.5, Vec3::new(0.78, 0.4, 0.92)),
    (0.78, Vec3::new(0.3, 0.45, 1.1)),
    (1.2, Vec3::new(0.16, 0.42, 1.2)),
];
const CORE: Vec3 = Vec3::new(1.0, 0.8, 0.6);
/// Tint of the brightest field stars, mixed with blackbody colours.
const STAR_VIOLET: Vec3 = Vec3::new(0.85, 0.6, 1.0);
/// Dust extinction per channel: blue light is absorbed most, so dust lanes
/// read red-brown against the pink disk.
const EXTINCTION: Vec3 = Vec3::new(0.55, 1.0, 1.55);

static STYLE: Style = Style {
    exposure: 1.05,
    bloom: &[(2, 0.10), (8, 0.08), (30, 0.08)],
    tonemap: aces_hue,
    // A galaxy is defined by its glow against the dark, which ink on white
    // can only invert.
    light: LightMode::Backdrop { feather: 16 },
};

/// Wraps an angle into (-π, π].
fn wrap(a: f32) -> f32 {
    a - TAU * (a / TAU).round()
}

fn palette(r: f32) -> Vec3 {
    for pair in PALETTE.windows(2) {
        let ((r0, c0), (r1, c1)) = (pair[0], pair[1]);
        if r <= r1 {
            let t = ((r - r0) / (r1 - r0)).clamp(0.0, 1.0);
            return c0 * (1.0 - t) + c1 * t;
        }
    }
    PALETTE[PALETTE.len() - 1].1
}

/// One oblate Gaussian component of the core, reduced to a 1D Gaussian
/// along a particular ray: `peak * exp(-k (s - center)²)`.
#[derive(Clone, Copy, Default)]
struct CoreAlongRay {
    peak: f32,
    center: f32,
    sqrt_k: f32,
}

impl CoreAlongRay {
    /// Light emitted between ray parameters `s0 < s1`, normalised so the
    /// whole ray carries `peak`.
    fn between(&self, s0: f32, s1: f32) -> f32 {
        let cdf = |s: f32| erf(self.sqrt_k * (s - self.center));
        self.peak * 0.5 * (cdf(s1) - cdf(s0))
    }
}

/// Abramowitz & Stegun 7.1.26, accurate to about 1e-7.
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_7 + t * (1.421_413_7 + t * (-1.453_152 + t * 1.061_405_4))));
    (1.0 - poly * (-x * x).exp()).copysign(x)
}

/// Share of a `sech²(y / h)` vertical profile (unit integral) lying between
/// heights `y0 < y1`. Integrating exactly keeps layers thinner than a march
/// step from turning grainy.
fn sech2_between(y0: f32, y1: f32, h: f32) -> f32 {
    0.5 * ((y1 / h).tanh() - (y0 / h).tanh())
}

/// Everything about a subsample ray that the spin does not change.
#[derive(Clone, Copy, Default)]
struct Ray {
    origin: Vec3,
    /// Offset of this subsample's slab samples within a layer, in [0, 1):
    /// spreads the layers apart between subsamples so they never band.
    jitter: f32,
    core: [CoreAlongRay; 3],
    /// Static light that is not part of the disk (M32).
    satellite: f32,
}

pub struct Galaxy {
    rays: Vec<[Ray; 4]>,
    forward: Vec3,
    /// Screen axes in world space, for projecting in-plane star offsets.
    right: Vec3,
    up: Vec3,
    blackbody: Blackbody,
}

impl Galaxy {
    pub fn new(width: usize, height: usize) -> Self {
        let (elevation, azimuth) = (ELEVATION_DEG.to_radians(), AZIMUTH_DEG.to_radians());
        let to_camera = Vec3::new(
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            -elevation.cos() * azimuth.cos(),
        );
        let forward = -to_camera;
        let right0 = Vec3::new(0.0, 1.0, 0.0).cross(forward).normalize();
        let up0 = forward.cross(right0);
        let (sin_roll, cos_roll) = ROLL_DEG.to_radians().sin_cos();
        let right = right0 * cos_roll + up0 * sin_roll;
        let up = up0 * cos_roll - right0 * sin_roll;

        let rays = (0..width * height)
            .into_par_iter()
            .map(|i| {
                let mut k = 0;
                camera::subsamples(i, width, height).map(|(u, v)| {
                    let origin = (right * u + up * v) * HALF_HEIGHT + to_camera * 4.0;
                    let (du, dv) = (u - SATELLITE.0, v - SATELLITE.1);
                    let satellite =
                        (-(du * du + dv * dv) / (SATELLITE_SIZE * SATELLITE_SIZE)).exp();
                    let jitter = (k as f32 + hash01(i as i32, k, 9)) / 4.0;
                    k += 1;
                    Ray {
                        origin,
                        jitter,
                        core: Self::core_along(origin, forward),
                        satellite: SATELLITE_BRIGHTNESS * satellite,
                    }
                })
            })
            .collect();
        Self {
            rays,
            forward,
            right,
            up,
            blackbody: Blackbody::new(),
        }
    }

    /// Reduces each oblate core component to a Gaussian along the ray.
    fn core_along(origin: Vec3, dir: Vec3) -> [CoreAlongRay; 3] {
        [(0.04, 3.0), (0.11, 1.4), (0.22, 0.3)].map(|(a, weight)| {
            let scale = Vec3::new(
                1.0 / (a * a),
                1.0 / (CORE_FLATTENING * a).powi(2),
                1.0 / (a * a),
            );
            let k = dir.dot(dir * scale);
            let b = dir.dot(origin * scale);
            CoreAlongRay {
                peak: weight * (-(origin.dot(origin * scale) - b * b / k)).exp(),
                center: -b / k,
                sqrt_k: k.sqrt(),
            }
        })
    }

    /// Field stars at material angle `a`, drawn round in screen space.
    fn stars(&self, r: f32, theta: f32, a: f32, density: f32) -> Vec3 {
        let (sin, cos) = a.sin_cos();
        let (mx, mz) = (r * cos / STAR_CELL, r * sin / STAR_CELL);
        let (cx, cz) = (mx.floor() as i32, mz.floor() as i32);
        if hash01(cx, cz, 1) > density {
            return Vec3::default();
        }
        // Keep the star well inside its cell so neighbours never overlap it.
        let sx = cx as f32 + 0.3 + 0.4 * hash01(cx, cz, 2);
        let sz = cz as f32 + 0.3 + 0.4 * hash01(cx, cz, 3);
        let (dx, dz) = ((mx - sx) * STAR_CELL, (mz - sz) * STAR_CELL);
        // Rotate the offset back from the material frame into the world.
        let (dsin, dcos) = (a - theta).sin_cos();
        let offset = Vec3::new(dx * dcos + dz * dsin, 0.0, -dx * dsin + dz * dcos);
        let (su, sv) = (offset.dot(self.right), offset.dot(self.up));
        let d2 = (su * su + sv * sv) / (STAR_RADIUS * STAR_RADIUS);
        if d2 > 9.0 {
            return Vec3::default();
        }
        let brightness = hash01(cx, cz, 4).powi(10) * 14.0 + 0.3;
        let temperature = 5500.0 + 20_000.0 * hash01(cx, cz, 5).powi(2);
        let violet = hash01(cx, cz, 6);
        let color = self.blackbody.chroma(temperature) * (1.0 - violet) + STAR_VIOLET * violet;
        color * (brightness * (-d2).exp())
    }

    /// Face-on disk light and dust column at `(r, theta)` with the disk
    /// turned `cycles` loops; the slab march spreads both vertically.
    fn disk(&self, r: f32, theta: f32, cycles: f32) -> (Vec3, f32, f32) {
        let a = theta - SPIN / (r + SPIN_CORE) * cycles;
        let (sin, cos) = a.sin_cos();
        let (mx, mz) = (r * cos, r * sin);
        let noise = |freq: f32, seed: f32, octaves: u32| {
            fbm(Vec3::new(mx * freq, mz * freq, seed), octaves)
        };
        let (clumps, filaments, haze) =
            (noise(10.0, 3.1, 3), noise(18.0, 1.3, 3), noise(4.0, 8.2, 2));

        let edge = 1.0 - smoothstep(0.7, 1.2, r);
        let wobble = 0.9 * noise(2.2, 11.0, 2);
        let phase = a + WINDING * (r / ARM_START).max(1e-4).ln() + wobble;
        let width = 0.32 + 0.1 * r;
        let (mut arms, mut lanes) = (0.0, 0.0);
        for offset in [0.0, PI] {
            let d = wrap(phase - offset) / width;
            let lane = wrap(phase - offset + LANE_SHIFT) / (0.6 * width);
            arms += (-0.5 * d * d).exp();
            lanes += (-0.5 * lane * lane).exp();
        }
        let ring = (-((r - RING_RADIUS) / 0.07).powi(2)).exp();
        let envelope = smoothstep(0.12, 0.3, r) * edge;
        let arms = (arms + ring) * envelope;
        let lanes = (lanes + 0.8 * (-((r - RING_RADIUS + 0.04) / 0.03).powi(2)).exp()) * envelope;

        let smooth = 1.1 * (-r / 0.2).exp() + 0.25 * (-r / 0.4).exp() * edge;
        let outer_haze = smoothstep(0.5, 0.85, r) * edge * 0.16 * smoothstep(-0.2, 0.5, haze);
        let young = arms
            * (0.08 + 0.5 * smoothstep(-0.1, 0.5, clumps).powi(2))
            * (1.0 - 0.5 * smoothstep(0.7, 1.1, r));
        let brightness = smooth * (0.75 + 0.35 * clumps) + young + outer_haze;

        let dust = 9.0 * lanes * smoothstep(-0.2, 0.4, filaments)
            + 0.5 * smoothstep(0.15, 0.6, filaments) * smoothstep(0.12, 0.3, r) * edge;
        let star_density = (0.05 + 0.3 * arms + 0.3 * smooth).min(0.4) * edge;
        (palette(r) * brightness, dust, star_density)
    }

    /// Marches the ray through the disk slab front to back, returning light
    /// and the dust transmittance left for whatever lies behind.
    fn march(&self, ray: &Ray, cycles: f32) -> (Vec3, Vec3) {
        let (o, d) = (ray.origin, self.forward);
        let s_at = |y: f32| (y - o.y) / d.y;
        let core_between =
            |s0: f32, s1: f32| -> f32 { ray.core.iter().map(|c| c.between(s0, s1)).sum() };

        // The camera looks down, so the slab's top face is nearest.
        let mut light = CORE * core_between(f32::NEG_INFINITY, s_at(SLAB));
        let mut through = Vec3::new(1.0, 1.0, 1.0);
        let step = 2.0 * SLAB / LAYERS as f32;
        for layer in 0..LAYERS {
            // Stars live in the midplane, behind the upper half's dust.
            if layer == LAYERS / 2 {
                let p0 = o + d * s_at(0.0);
                let (r0, theta0) = ((p0.x * p0.x + p0.z * p0.z).sqrt(), p0.z.atan2(p0.x));
                if r0 <= DISK_EXTENT {
                    let (_, _, density) = self.disk(r0, theta0, cycles);
                    let a0 = theta0 - SPIN / (r0 + SPIN_CORE) * cycles;
                    light = light + self.stars(r0, theta0, a0, density) * through * 0.4;
                }
            }
            let top = SLAB - layer as f32 * step;
            let y = top - ray.jitter * step;
            let p = o + d * s_at(y);
            let r = (p.x * p.x + p.z * p.z).sqrt();

            let core = CORE * core_between(s_at(top), s_at(top - step));
            if r > DISK_EXTENT {
                light = light + core * through;
                continue;
            }
            let theta = p.z.atan2(p.x);
            let (emission, dust, _) = self.disk(r, theta, cycles);
            let stars_h = 0.014 + 0.022 * r;
            let dust_h = 0.006 + 0.007 * r;
            let tau = dust * sech2_between(top - step, top, dust_h);
            let layer_through = EXTINCTION.map(|k| (-tau * k).exp());
            // Light emitted inside the layer is, on average, half attenuated
            // by the layer's own dust.
            let half = (layer_through + Vec3::new(1.0, 1.0, 1.0)) * 0.5;
            let own = (emission * sech2_between(top - step, top, stars_h) + core) * half;
            light = light + own * through;
            through = through * layer_through;
        }
        light = light + CORE * core_between(s_at(-SLAB), f32::INFINITY) * through;
        (light, through)
    }

    fn shade_ray(&self, ray: &Ray, phase: f32) -> (Vec3, f32) {
        // Cross-fade one loop ahead with one loop behind: at phase 1 the
        // second term is exactly the first term at phase 0.
        let (ahead, through_ahead) = self.march(ray, phase);
        let (behind, through_behind) = self.march(ray, phase - 1.0);
        let light = ahead * (1.0 - phase) + behind * phase;
        let through = through_ahead * (1.0 - phase) + through_behind * phase;

        let satellite = self.blackbody.chroma(5200.0) * ray.satellite;
        // Dust only partly hides the page in light mode; full opacity reads
        // as holes in the galaxy rather than lanes.
        let occlusion = 0.5 * (1.0 - luminance(through)) * smoothstep(0.0, 0.05, luminance(light));
        (light + satellite, 1.0 - occlusion)
    }
}

impl Scene for Galaxy {
    fn style(&self) -> &'static Style {
        &STYLE
    }

    fn shade(&self, phase: f32) -> (Vec<Vec3>, Vec<f32>) {
        self.rays
            .par_iter()
            .map(|subsamples| {
                let mut emission = Vec3::default();
                let mut transmittance = 0.0;
                for ray in subsamples {
                    let (e, t) = self.shade_ray(ray, phase);
                    emission = emission + e;
                    transmittance += t;
                }
                let n = subsamples.len() as f32;
                (emission * (1.0 / n), transmittance / n)
            })
            .unzip()
    }
}
