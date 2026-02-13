use gif::{Encoder, Frame, Repeat};
use image::{DynamicImage, Rgba, RgbaImage};
use std::fs::File;
use std::path::Path;

const HSTRETCH_DURATION: f32 = 0.5;
const VSTRETCH_DURATION: f32 = 1.0 - HSTRETCH_DURATION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamMode {
    WarmUp,
    CoolDown,
    Fade,
    ScaleDown,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub mode: BeamMode,
    pub frames: usize,
    pub frame_delay_cs: u16,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            mode: BeamMode::CoolDown,
            frames: 48,
            frame_delay_cs: 2,
        }
    }
}

pub fn create_gif_from_image<P: AsRef<Path>>(
    image_path: P,
    output_gif_path: P,
    options: &RenderOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let src = image::open(image_path)?;
    let mut out = File::create(output_gif_path)?;
    create_gif(&src, &mut out, options)
}

pub fn create_gif<W: std::io::Write>(
    source: &DynamicImage,
    writer: &mut W,
    options: &RenderOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if options.frames < 2 {
        return Err("frames must be >= 2".into());
    }

    let src = source.to_rgba8();
    let (width, height) = src.dimensions();
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err("image dimensions must fit into GIF limits (<= 65535)".into());
    }

    let mut encoder = Encoder::new(writer, width as u16, height as u16, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;

    for frame_idx in 0..options.frames {
        let level = frame_idx as f32 / (options.frames - 1) as f32;
        let mut frame_img = render_frame(&src, options.mode, level);
        let mut frame = Frame::from_rgba_speed(width as u16, height as u16, frame_img.as_mut(), 10);
        frame.delay = options.frame_delay_cs;
        encoder.write_frame(&frame)?;
    }

    Ok(())
}

pub fn render_frame(src: &RgbaImage, mode: BeamMode, level: f32) -> RgbaImage {
    let clamped = level.clamp(0.0, 1.0);
    let (width, height) = src.dimensions();

    match mode {
        BeamMode::Fade => render_fade(src, clamped),
        BeamMode::ScaleDown => render_scale_down(src, clamped),
        BeamMode::WarmUp | BeamMode::CoolDown => {
            // In Android this class receives opposite level ramps for warm-up vs cool-down.
            // Mirror cool-down progress so the CLI behavior matches the expected visual direction.
            let progress = match mode {
                BeamMode::CoolDown => 1.0 - clamped,
                _ => clamped,
            };

            if progress < HSTRETCH_DURATION {
                let stretch = 1.0 - (progress / HSTRETCH_DURATION);
                render_hstretch(width, height, stretch)
            } else {
                let stretch = 1.0 - ((progress - HSTRETCH_DURATION) / VSTRETCH_DURATION);
                render_vstretch(src, mode, stretch)
            }
        }
    }
}

fn render_fade(src: &RgbaImage, level: f32) -> RgbaImage {
    let mut out = src.clone();
    let alpha = (1.0 - level).clamp(0.0, 1.0);
    for px in out.pixels_mut() {
        px[0] = ((px[0] as f32) * alpha) as u8;
        px[1] = ((px[1] as f32) * alpha) as u8;
        px[2] = ((px[2] as f32) * alpha) as u8;
    }
    out
}

fn render_scale_down(src: &RgbaImage, scale: f32) -> RgbaImage {
    let (width, height) = src.dimensions();
    let curved = scurve(scale, 8.0);

    let w = (width as f32) * curved;
    let h = (height as f32) * curved;
    let x = (width as f32 - w) * 0.5;
    let y = (height as f32 - h) * 0.5;

    let mut out = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    draw_textured_quad(src, &mut out, Quad { x, y, w, h });

    let dim_alpha = (1.0 - curved).clamp(0.0, 1.0);
    for py in 0..height {
        for px in 0..width {
            if point_in_quad(px as f32 + 0.5, py as f32 + 0.5, Quad { x, y, w, h }) {
                let p = out.get_pixel_mut(px, py);
                p[0] = ((p[0] as f32) * (1.0 - dim_alpha)) as u8;
                p[1] = ((p[1] as f32) * (1.0 - dim_alpha)) as u8;
                p[2] = ((p[2] as f32) * (1.0 - dim_alpha)) as u8;
            }
        }
    }

    out
}

fn render_hstretch(width: u32, height: u32, stretch: f32) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    if stretch >= 1.0 {
        return out;
    }

    let ag = scurve(stretch, 8.0);
    let quad = hstretch_quad(width as f32, height as f32, ag);
    let line = ((1.0 - ag * 0.75) * 255.0).clamp(0.0, 255.0) as u8;

    for y in 0..height {
        for x in 0..width {
            if point_in_quad(x as f32 + 0.5, y as f32 + 0.5, quad) {
                out.put_pixel(x, y, Rgba([line, line, line, 255]));
            }
        }
    }

    out
}

fn render_vstretch(src: &RgbaImage, mode: BeamMode, stretch: f32) -> RgbaImage {
    let (width, height) = src.dimensions();
    let mut out = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    let ar = scurve(stretch, 7.5);
    let ag = scurve(stretch, 8.0);
    let ab = scurve(stretch, 8.5);

    draw_single_channel_plane(
        src,
        &mut out,
        vstretch_quad(width as f32, height as f32, ar),
        0,
    );
    draw_single_channel_plane(
        src,
        &mut out,
        vstretch_quad(width as f32, height as f32, ag),
        1,
    );
    let blue_quad = vstretch_quad(width as f32, height as f32, ab);
    draw_single_channel_plane(src, &mut out, blue_quad, 2);

    if mode == BeamMode::CoolDown {
        let add = (ag * 255.0).clamp(0.0, 255.0) as u8;
        add_white_quad(&mut out, blue_quad, add);
    }

    out
}

#[derive(Clone, Copy)]
struct Quad {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn vstretch_quad(dw: f32, dh: f32, a: f32) -> Quad {
    let w = dw + (dw * a);
    let h = dh - (dh * a);
    let x = (dw - w) * 0.5;
    let y = (dh - h) * 0.5;
    Quad { x, y, w, h }
}

fn hstretch_quad(dw: f32, dh: f32, a: f32) -> Quad {
    let w = 2.0 * dw * (1.0 - a);
    let h = 1.0;
    let x = (dw - w) * 0.5;
    let y = (dh - h) * 0.5;
    Quad { x, y, w, h }
}

fn draw_textured_quad(src: &RgbaImage, out: &mut RgbaImage, quad: Quad) {
    let (width, height) = out.dimensions();
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if let Some((u, v)) = quad_uv(px, py, quad) {
                let c = bilinear_sample(src, u, v);
                out.put_pixel(x, y, c);
            }
        }
    }
}

fn draw_single_channel_plane(src: &RgbaImage, out: &mut RgbaImage, quad: Quad, channel: usize) {
    let (width, height) = out.dimensions();
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if let Some((u, v)) = quad_uv(px, py, quad) {
                let c = bilinear_sample(src, u, v);
                let p = out.get_pixel_mut(x, y);
                p[channel] = c[channel];
            }
        }
    }
}

fn add_white_quad(out: &mut RgbaImage, quad: Quad, add: u8) {
    let (width, height) = out.dimensions();
    for y in 0..height {
        for x in 0..width {
            if point_in_quad(x as f32 + 0.5, y as f32 + 0.5, quad) {
                let p = out.get_pixel_mut(x, y);
                p[0] = p[0].saturating_add(add);
                p[1] = p[1].saturating_add(add);
                p[2] = p[2].saturating_add(add);
            }
        }
    }
}

fn point_in_quad(px: f32, py: f32, quad: Quad) -> bool {
    px >= quad.x && px <= quad.x + quad.w && py >= quad.y && py <= quad.y + quad.h
}

fn quad_uv(px: f32, py: f32, quad: Quad) -> Option<(f32, f32)> {
    if !point_in_quad(px, py, quad) || quad.w <= 0.0 || quad.h <= 0.0 {
        return None;
    }
    let u = ((px - quad.x) / quad.w).clamp(0.0, 1.0);
    let v = ((py - quad.y) / quad.h).clamp(0.0, 1.0);
    Some((u, v))
}

fn bilinear_sample(img: &RgbaImage, u: f32, v: f32) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    let fx = u * (w as f32 - 1.0);
    let fy = v * (h as f32 - 1.0);

    let x0 = fx.floor().max(0.0) as u32;
    let y0 = fy.floor().max(0.0) as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);

    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let c00 = img.get_pixel(x0, y0);
    let c10 = img.get_pixel(x1, y0);
    let c01 = img.get_pixel(x0, y1);
    let c11 = img.get_pixel(x1, y1);

    let mut out = [0u8; 4];
    for i in 0..4 {
        let a = lerp(c00[i] as f32, c10[i] as f32, tx);
        let b = lerp(c01[i] as f32, c11[i] as f32, tx);
        out[i] = lerp(a, b, ty).round().clamp(0.0, 255.0) as u8;
    }

    Rgba(out)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn scurve(value: f32, s: f32) -> f32 {
    let x = value - 0.5;
    let y = sigmoid(x, s) - 0.5;
    let v = sigmoid(0.5, s) - 0.5;
    y / v * 0.5 + 0.5
}

fn sigmoid(x: f32, s: f32) -> f32 {
    1.0 / (1.0 + (-x * s).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient_image(width: u32, height: u32) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        ((x as f32 / width as f32) * 255.0) as u8,
                        ((y as f32 / height as f32) * 255.0) as u8,
                        180,
                        255,
                    ]),
                );
            }
        }
        img
    }

    #[test]
    fn render_frame_preserves_dimensions() {
        let src = gradient_image(64, 36);
        let out = render_frame(&src, BeamMode::CoolDown, 0.7);
        assert_eq!(out.dimensions(), src.dimensions());
    }

    #[test]
    fn create_gif_writes_data() {
        let src = DynamicImage::ImageRgba8(gradient_image(48, 48));
        let options = RenderOptions {
            mode: BeamMode::ScaleDown,
            frames: 12,
            frame_delay_cs: 2,
        };
        let mut bytes = Vec::new();
        create_gif(&src, &mut bytes, &options).expect("GIF generation should succeed");
        assert!(bytes.len() > 100, "GIF output should not be empty");
        assert_eq!(&bytes[0..6], b"GIF89a");
    }

    #[test]
    fn cooldown_moves_toward_black() {
        let src = gradient_image(80, 40);
        let first = render_frame(&src, BeamMode::CoolDown, 0.0);
        let last = render_frame(&src, BeamMode::CoolDown, 1.0);

        let sum_luma = |img: &RgbaImage| -> u64 {
            img.pixels()
                .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
                .sum()
        };

        assert!(
            sum_luma(&first) > sum_luma(&last),
            "cool-down should end darker than it starts"
        );
    }
}
