//! SVG to pixels, with no Bevy involved (so the Hub, CLI and tests can use it too).

use resvg::{tiny_skia, usvg};

/// An 8-bit sRGB image with straight (not premultiplied) alpha, row-major RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub data: Vec<u8>,
}

/// Why an icon could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    /// No icon with this name in its set.
    #[error("icon not found: {0}")]
    NotFound(String),
    /// The SVG could not be parsed.
    #[error("invalid SVG: {0}")]
    Parse(usvg::Error),
    /// The requested size was zero or too large.
    #[error("cannot render an icon at {0} px")]
    BadSize(u32),
}

/// Largest size we will render, to catch unit mix-ups early.
const MAX_SIZE: u32 = 4096;

/// Render an SVG so its longest side is `size_px` pixels, keeping its aspect ratio.
pub fn rasterize(svg: &str, size_px: u32) -> Result<RgbaImage, IconError> {
    if size_px == 0 || size_px > MAX_SIZE {
        return Err(IconError::BadSize(size_px));
    }
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(IconError::Parse)?;

    let source = tree.size();
    let scale = size_px as f32 / source.width().max(source.height());
    let width = ((source.width() * scale).round() as u32).max(1);
    let height = ((source.height() * scale).round() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(IconError::BadSize(size_px))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied alpha; GPUs sample straight alpha here.
    let data = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();

    Ok(RgbaImage {
        width,
        height,
        data,
    })
}
