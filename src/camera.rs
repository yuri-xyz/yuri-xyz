//! Pixel sampling shared by every scene.

/// Rotated-grid 4× supersampling offsets within a pixel.
pub const SUBSAMPLES: [(f32, f32); 4] = [
    (0.375, 0.125),
    (0.875, 0.375),
    (0.125, 0.625),
    (0.625, 0.875),
];

/// Screen coordinates of every subsample of pixel `index`: `v` spans
/// [-1, 1] bottom to top and `u` keeps square pixels.
pub fn subsamples(index: usize, width: usize, height: usize) -> [(f32, f32); 4] {
    let (px, py) = ((index % width) as f32, (index / width) as f32);
    let aspect = width as f32 / height as f32;
    SUBSAMPLES.map(|(sx, sy)| {
        let u = (2.0 * (px + sx) / width as f32 - 1.0) * aspect;
        let v = 1.0 - 2.0 * (py + sy) / height as f32;
        (u, v)
    })
}
