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
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "blackhole" => Some(Self::BlackHole),
            "galaxy" => Some(Self::Galaxy),
            _ => None,
        }
    }

    pub fn build(self, width: usize, height: usize) -> Box<dyn Scene> {
        match self {
            Self::BlackHole => Box::new(blackhole::BlackHole::new(width, height)),
            Self::Galaxy => Box::new(galaxy::Galaxy::new(width, height)),
        }
    }
}
