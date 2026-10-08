//! Null geodesics around a Schwarzschild black hole (G = c = M = 1).
//!
//! Uses the Cartesian form of the photon orbit equation, `x'' = -3 h² x / r⁵`
//! with `h = |x × x'|`, which reproduces `u'' + u = 3u²` exactly while
//! staying free of coordinate singularities away from the origin.

use crate::vec3::Vec3;

/// Event horizon radius.
const HORIZON: f32 = 2.0;
/// Rays farther than this and moving outward have escaped.
const ESCAPE_RADIUS: f32 = 90.0;
const MAX_STEPS: usize = 4000;

/// Disk crossings recorded per ray; higher-order images are sub-pixel.
pub const MAX_HITS: usize = 3;

/// A point where a ray pierces the equatorial plane inside the disk.
#[derive(Clone, Copy, Default)]
pub struct Hit {
    pub r: f32,
    pub phi: f32,
    /// Total frequency shift (Doppler × gravitational), observed / emitted.
    pub g: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Path {
    pub hits: [Hit; MAX_HITS],
    pub hit_count: u8,
    pub captured: bool,
}

impl Path {
    pub fn hits(&self) -> &[Hit] {
        &self.hits[..usize::from(self.hit_count)]
    }
}

fn accel(x: Vec3, h2: f32) -> Vec3 {
    let r2 = x.dot(x);
    x * (-3.0 * h2 / (r2 * r2 * r2.sqrt()))
}

/// Frequency shift for light leaving the disk at `x` towards the camera,
/// travelling opposite to the traced direction `dir`.
fn shift(x: Vec3, dir: Vec3, r: f32) -> f32 {
    // Orbital speed of a circular orbit as measured by a static observer.
    let beta = (1.0 / (r - 2.0)).sqrt().min(0.99);
    let gamma = 1.0 / (1.0 - beta * beta).sqrt();
    let tangent = Vec3::new(-x.z, 0.0, x.x).normalize();
    let cos_theta = tangent.dot(-dir.normalize());
    let doppler = 1.0 / (gamma * (1.0 - beta * cos_theta));
    doppler * (1.0 - 2.0 / r).sqrt()
}

/// Traces a ray backwards from the camera and records its disk crossings.
pub fn trace(origin: Vec3, dir: Vec3, disk_inner: f32, disk_outer: f32) -> Path {
    let mut path = Path::default();
    let mut x = origin;
    let mut v = dir.normalize();
    let h2 = x.cross(v).length_squared();

    for _ in 0..MAX_STEPS {
        let r = x.length();
        if r < HORIZON {
            path.captured = true;
            break;
        }
        if r > ESCAPE_RADIUS && x.dot(v) > 0.0 {
            break;
        }

        // Large steps far away, fine steps near the photon sphere.
        let dt = (0.06 * r).clamp(0.02, 4.0) * (1.0 - 1.6 / r).max(0.15);

        // Classic RK4 on (x, v).
        let k1x = v;
        let k1v = accel(x, h2);
        let k2x = v + k1v * (0.5 * dt);
        let k2v = accel(x + k1x * (0.5 * dt), h2);
        let k3x = v + k2v * (0.5 * dt);
        let k3v = accel(x + k2x * (0.5 * dt), h2);
        let k4x = v + k3v * dt;
        let k4v = accel(x + k3x * dt, h2);
        let nx = x + (k1x + k2x * 2.0 + k3x * 2.0 + k4x) * (dt / 6.0);
        let nv = v + (k1v + k2v * 2.0 + k3v * 2.0 + k4v) * (dt / 6.0);

        if (x.y > 0.0) != (nx.y > 0.0) && usize::from(path.hit_count) < MAX_HITS {
            let t = x.y / (x.y - nx.y);
            let p = x + (nx - x) * t;
            let pr = p.length();
            if (disk_inner..disk_outer).contains(&pr) {
                path.hits[usize::from(path.hit_count)] = Hit {
                    r: pr,
                    phi: p.z.atan2(p.x),
                    g: shift(p, nx - x, pr),
                };
                path.hit_count += 1;
            }
        }

        x = nx;
        v = nv;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rays parallel to the axis at impact parameter `b`, with the disk
    /// moved out of the way.
    fn captured(b: f32) -> bool {
        trace(
            Vec3::new(0.0, b, -80.0),
            Vec3::new(0.0, 0.0, 1.0),
            1e3,
            1e3 + 1.0,
        )
        .captured
    }

    /// The shadow edge sits at the critical impact parameter 3√3 M.
    #[test]
    fn shadow_has_schwarzschild_radius() {
        let critical = 3.0 * 3.0_f32.sqrt();
        assert!(captured(critical * 0.97));
        assert!(!captured(critical * 1.03));
    }
}
