//! Bloom, tone mapping, and compositing onto GitHub's page backgrounds.

use crate::color::{linear_to_srgb, smoothstep};
use crate::vec3::Vec3;

/// How a scene turns linear light into each theme's pixels.
pub struct Style {
    pub exposure: f32,
    /// Blur radii in pixels and their weights, from tight halo to wide haze.
    pub bloom: &'static [(usize, f32)],
    /// Maps exposed linear light into [0, 1].
    pub tonemap: fn(Vec3) -> Vec3,
    /// How the scene stays visible on the light theme's white page.
    pub light: LightMode,
}

/// Bright light is invisible on a white page, so the light theme needs a
/// different rendering.
pub enum LightMode {
    /// Restates tone-mapped sRGB light as ink colour and coverage, drawn
    /// over `occluder` wherever the scene blocks the page (shadow, dust).
    Ink {
        ink: fn(Vec3) -> (Vec3, f32),
        occluder: Vec3,
    },
    /// Keeps the dark-theme look over a soft patch of night sky that hugs
    /// the scene, feathered into the page over about `feather` pixels.
    Backdrop { feather: usize },
}

#[derive(Clone, Copy)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];

    /// GitHub's default page background (`#0d1117` dark, `#ffffff` light),
    /// in sRGB. Baked in rather than left transparent because lossless WebP
    /// alpha for the soft glow roughly quadruples the file size.
    fn background(self) -> Vec3 {
        match self {
            Self::Dark => Vec3::new(13.0, 17.0, 23.0) * (1.0 / 255.0),
            Self::Light => Vec3::new(1.0, 1.0, 1.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// One horizontal or vertical box-blur pass with clamped edges.
fn box_pass(src: &[Vec3], dst: &mut [Vec3], w: usize, h: usize, radius: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    let norm = 1.0 / (2 * radius + 1) as f32;
    for line in 0..lines {
        let mut acc = Vec3::default();
        for k in 0..=2 * radius {
            acc = acc + src[at(line, k.saturating_sub(radius).min(len - 1))];
        }
        for i in 0..len {
            dst[at(line, i)] = acc * norm;
            let add = (i + radius + 1).min(len - 1);
            let sub = i.saturating_sub(radius);
            acc = acc + src[at(line, add)] - src[at(line, sub)];
        }
    }
}

/// Approximate Gaussian blur from three box passes per axis.
fn blur(src: &[Vec3], w: usize, h: usize, radius: usize) -> Vec<Vec3> {
    let mut a = src.to_vec();
    let mut b = vec![Vec3::default(); src.len()];
    for _ in 0..3 {
        box_pass(&a, &mut b, w, h, radius, true);
        box_pass(&b, &mut a, w, h, radius, false);
    }
    a
}

pub fn bloom(style: &Style, emission: &[Vec3], w: usize, h: usize) -> Vec<Vec3> {
    let mut glow = vec![Vec3::default(); emission.len()];
    for &(radius, weight) in style.bloom {
        for (g, b) in glow.iter_mut().zip(blur(emission, w, h, radius)) {
            *g = *g + b * weight;
        }
    }
    glow
}

/// Opacity of the scene over the dark page: alpha is raised to at least the
/// brightest channel because additive light has no exact alpha
/// representation; over a near-black page this is indistinguishable from
/// adding the light.
fn over_dark(light: Vec3, tr: f32) -> (Vec3, f32) {
    let alpha = (1.0 - tr).max(light.max_element()).clamp(0.0, 1.0);
    (light * (1.0 / alpha.max(1e-6)), alpha)
}

/// Converts light plus occlusion into sRGB over the theme's background.
pub fn composite(
    style: &Style,
    theme: Theme,
    w: usize,
    h: usize,
    emission: &[Vec3],
    glow: &[Vec3],
    transmittance: &[f32],
) -> Vec<u8> {
    let light: Vec<Vec3> = emission
        .iter()
        .zip(glow)
        .map(|(&e, &g)| (style.tonemap)((e + g) * style.exposure).map(linear_to_srgb))
        .collect();
    let page = theme.background();
    let night = Theme::Dark.background();
    // Per-pixel night-sky weight under the scene (backdrop mode only).
    let backdrop = match (theme, &style.light) {
        (Theme::Light, &LightMode::Backdrop { feather }) => {
            let opacity: Vec<Vec3> = light
                .iter()
                .zip(transmittance)
                .map(|(&l, &tr)| Vec3::new(1.0, 1.0, 1.0) * over_dark(l, tr).1)
                .collect();
            blur(&opacity, w, h, feather)
                .iter()
                .map(|o| smoothstep(0.02, 0.35, o.x))
                .collect()
        }
        _ => Vec::new(),
    };
    let mut out = Vec::with_capacity(emission.len() * 3);
    for (i, (&l, &tr)) in light.iter().zip(transmittance).enumerate() {
        let (rgb, alpha, background) = match (theme, &style.light) {
            (Theme::Dark, _) => {
                let (rgb, alpha) = over_dark(l, tr);
                (rgb, alpha, page)
            }
            (Theme::Light, &LightMode::Backdrop { .. }) => {
                let (rgb, alpha) = over_dark(l, tr);
                let k = backdrop[i];
                (rgb, alpha, night * k + page * (1.0 - k))
            }
            (Theme::Light, &LightMode::Ink { ink, occluder }) => {
                let (ink, coverage) = ink(l);
                // Ink over whatever the scene occludes, then the page.
                let occluded = (1.0 - tr) * (1.0 - coverage);
                let alpha = coverage + occluded;
                let rgb = (ink * coverage + occluder * occluded) * (1.0 / alpha.max(1e-6));
                (rgb, alpha, page)
            }
        };
        // Blend in sRGB space, as browsers composite.
        let c = rgb * alpha + background * (1.0 - alpha);
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        out.extend_from_slice(&[q(c.x), q(c.y), q(c.z)]);
    }
    out
}
