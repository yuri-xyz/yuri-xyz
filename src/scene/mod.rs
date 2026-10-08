//! The scenes the renderer can draw.

mod blackhole;
mod galaxy;

use crate::post::Style;
use crate::vec3::Vec3;

/// A scene whose geometry is prepared once and then shaded per frame.
pub trait Scene: Sync {
    fn style(&self) -> &'static Style;

    /// Linear HDR emission and background transmittance for every pixel at
    /// `phase` in [0, 1) of the loop.
    fn shade(&self, phase: f32) -> (Vec<Vec3>, Vec<f32>);
}

#[derive(Clone, Copy)]
pub enum SceneKind {
    BlackHole,
    Galaxy,
}

impl SceneKind {
    pub const ALL: [Self; 2] = [Self::BlackHole, Self::Galaxy];

    pub fn name(self) -> &'static str {
        match self {
            Self::BlackHole => "blackhole",
            Self::Galaxy => "galaxy",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn build(self, width: usize, height: usize) -> Box<dyn Scene> {
        match self {
            Self::BlackHole => Box::new(blackhole::BlackHole::new(width, height)),
            Self::Galaxy => Box::new(galaxy::Galaxy::new(width, height)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn max_difference(a: &[Vec3], b: &[Vec3]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(&x, &y)| (x - y).map(f32::abs).max_element())
            .fold(0.0, f32::max)
    }

    /// The last instant of the loop must meet its first frame far more
    /// closely than two ordinary neighbouring frames do.
    #[test]
    fn loops_are_seamless() {
        let frames = 150.0;
        for kind in SceneKind::ALL {
            let scene = kind.build(64, 32);
            let (start, _) = scene.shade(0.0);
            let (next, _) = scene.shade(1.0 / frames);
            let (end, _) = scene.shade(1.0 - 1e-4);
            let step = max_difference(&start, &next);
            let seam = max_difference(&start, &end);
            assert!(step > 0.0, "{} does not move", kind.name());
            assert!(
                seam < 0.1 * step,
                "{} seam {seam} vs frame step {step}",
                kind.name()
            );
        }
    }
}
