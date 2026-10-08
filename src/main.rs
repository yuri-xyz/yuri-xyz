//! Renders seamlessly looping space scenes as PNG frames in two theme
//! variants (`dark/`, `light/`) composited onto GitHub's page backgrounds,
//! ready to be encoded into animated WebP for a profile README.

mod camera;
mod color;
mod noise;
mod post;
mod scene;
mod vec3;

use std::path::{Path as FsPath, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use crate::post::Theme;
use crate::scene::SceneKind;

struct Options {
    scene: SceneKind,
    width: usize,
    height: usize,
    frames: usize,
    only_frame: Option<usize>,
    out: PathBuf,
}

const USAGE: &str = "usage: profile-art <blackhole|galaxy> [--width N] [--height N] [--frames N] [--frame K] <out-dir>";

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        scene: SceneKind::BlackHole,
        width: 960,
        height: 440,
        frames: 150,
        only_frame: None,
        out: PathBuf::new(),
    };
    let mut scene = None;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| -> Result<usize, String> {
            args.next()
                .ok_or_else(|| format!("{name} needs a value"))?
                .parse()
                .map_err(|e| format!("{name}: {e}"))
        };
        match arg.as_str() {
            "--width" => opts.width = value("--width")?,
            "--height" => opts.height = value("--height")?,
            "--frames" => opts.frames = value("--frames")?,
            "--frame" => opts.only_frame = Some(value("--frame")?),
            "-h" | "--help" => return Err(USAGE.to_owned()),
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ if scene.is_none() => {
                scene = Some(
                    SceneKind::parse(&arg)
                        .ok_or_else(|| format!("unknown scene {arg}\n{USAGE}"))?,
                );
            }
            _ if out.is_none() => out = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg}\n{USAGE}")),
        }
    }
    opts.scene = scene.ok_or_else(|| USAGE.to_owned())?;
    opts.out = out.ok_or_else(|| USAGE.to_owned())?;
    if opts.width == 0 || opts.height == 0 || opts.frames == 0 {
        return Err("dimensions and frame count must be positive".to_owned());
    }
    if opts.only_frame.is_some_and(|k| k >= opts.frames) {
        return Err("--frame must be below --frames".to_owned());
    }
    Ok(opts)
}

fn save(
    dir: &FsPath,
    frame: usize,
    width: usize,
    height: usize,
    rgb: Vec<u8>,
) -> Result<(), String> {
    let file = dir.join(format!("{frame:04}.png"));
    image::RgbImage::from_raw(width as u32, height as u32, rgb)
        .ok_or("frame buffer size mismatch")?
        .save(&file)
        .map_err(|e| format!("{}: {e}", file.display()))
}

fn run(opts: &Options) -> Result<(), String> {
    let (w, h) = (opts.width, opts.height);
    for theme in Theme::ALL {
        std::fs::create_dir_all(opts.out.join(theme.name()))
            .map_err(|e| format!("{}: {e}", opts.out.display()))?;
    }

    let start = Instant::now();
    let scene = opts.scene.build(w, h);
    let style = scene.style();
    eprintln!("prepared {}×{} in {:.1?}", w, h, start.elapsed());

    let frames: Vec<usize> = match opts.only_frame {
        Some(k) => vec![k],
        None => (0..opts.frames).collect(),
    };
    for frame in frames {
        let (emission, transmittance) = scene.shade(frame as f32 / opts.frames as f32);
        let glow = post::bloom(style, &emission, w, h);
        for theme in Theme::ALL {
            let rgb = post::composite(style, theme, w, h, &emission, &glow, &transmittance);
            save(&opts.out.join(theme.name()), frame, w, h, rgb)?;
        }
        eprint!("\rframe {}/{}", frame + 1, opts.frames);
    }
    eprintln!("\ndone in {:.1?}", start.elapsed());
    Ok(())
}

fn main() -> ExitCode {
    match parse_args().and_then(|opts| run(&opts)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
