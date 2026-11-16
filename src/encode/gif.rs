//! Streaming GIF export.
//!
//! Each frame is quantized and handed to [`gif::Encoder`] as soon as it
//! arrives, so the animation is never collected in memory. Colors come from
//! `quantette`'s Wu quantizer with dithering disabled; the same frames and
//! options always produce byte-identical output.
//!
//! GIF limits: a frame palette holds at most 256 colors, alpha is 1-bit
//! (`alpha < 128` becomes transparent) and frame delays are stored in
//! hundredths of a second, so very high frame rates collapse to a 10 ms delay.

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use gif::{DisposalMethod, Frame as GifFrame, Repeat};
use image::RgbaImage;
use quantette::deps::palette::Srgb;
use quantette::{ImageBuf, PaletteSize, Pipeline, QuantizeMethod};
use thiserror::Error;

/// Palette sizes accepted by the exporter.
const MIN_PALETTE_SIZE: u16 = 2;
const MAX_PALETTE_SIZE: u16 = 256;

/// Alpha below this value is exported as transparent; GIF has 1-bit alpha.
const TRANSPARENT_ALPHA_THRESHOLD: u8 = 128;

/// Palette slot reserved for transparency when the option is enabled.
const TRANSPARENT_INDEX: u8 = 0;

/// One hundredth of a second, the unit of `gif::Frame::delay`.
const DELAY_UNITS_PER_SECOND: u32 = 100;

/// GIF encoder options shared by the CLI and the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifOptions {
    /// Playback rate in frames per second. Must be greater than zero.
    pub fps: u16,
    /// Repeat the animation forever.
    ///
    /// `false` maps to [`Repeat::Finite(0)`], the `gif` crate's encoding of
    /// "no repeat extension", which players interpret as a single play.
    pub loop_: bool,
    /// Maximum number of colors per frame palette, `2..=256`.
    pub palette_size: u16,
    /// Export pixels with `alpha < 128` as transparent.
    pub transparent: bool,
}

impl Default for GifOptions {
    fn default() -> Self {
        Self {
            fps: 30,
            loop_: true,
            palette_size: 256,
            transparent: false,
        }
    }
}

/// Errors produced by the GIF exporter.
#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("gif error: {0}")]
    Gif(#[from] gif::EncodingError),
    #[error("invalid GIF options: {reason}")]
    InvalidOptions { reason: String },
    #[error(
        "frame is {actual_width}x{actual_height} but the GIF canvas is \
         {expected_width}x{expected_height}"
    )]
    Dimensions {
        expected_width: u16,
        expected_height: u16,
        actual_width: u32,
        actual_height: u32,
    },
}

/// Streaming GIF encoder.
///
/// Frames are written to the underlying writer one at a time; callers feed
/// them as they are rendered and call [`GifEncoder::finish`] at the end.
pub struct GifEncoder<W: Write> {
    encoder: gif::Encoder<W>,
    width: u16,
    height: u16,
    delay: u16,
    palette_size: u16,
    transparent: bool,
}

impl<W: Write> GifEncoder<W> {
    /// Create an encoder and write the GIF header immediately.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::InvalidOptions`] when a dimension is zero,
    /// `fps` is zero or `palette_size` is outside `2..=256`.
    pub fn new(
        writer: W,
        width: u16,
        height: u16,
        options: GifOptions,
    ) -> Result<Self, EncodeError> {
        validate_options(width, height, &options)?;

        let mut encoder = gif::Encoder::new(writer, width, height, &[])?;
        let repeat = if options.loop_ {
            Repeat::Infinite
        } else {
            Repeat::Finite(0)
        };
        encoder.set_repeat(repeat)?;

        Ok(Self {
            encoder,
            width,
            height,
            delay: frame_delay(options.fps),
            palette_size: options.palette_size,
            transparent: options.transparent,
        })
    }

    /// Quantize `image` and stream it as the next frame.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::Dimensions`] when the image size differs from the
    /// canvas passed to [`GifEncoder::new`], and surfaces encoder and writer
    /// failures as [`EncodeError::Gif`] or [`EncodeError::Io`].
    pub fn write_frame(&mut self, image: &RgbaImage) -> Result<(), EncodeError> {
        if image.width() != u32::from(self.width) || image.height() != u32::from(self.height) {
            return Err(EncodeError::Dimensions {
                expected_width: self.width,
                expected_height: self.height,
                actual_width: image.width(),
                actual_height: image.height(),
            });
        }

        let quantized = quantize(image, self.palette_size, self.transparent);
        let frame = GifFrame {
            width: self.width,
            height: self.height,
            buffer: Cow::Owned(quantized.indices),
            palette: Some(quantized.palette),
            delay: self.delay,
            transparent: quantized.transparent,
            // Every frame is full canvas; clearing transparent frames avoids
            // ghosting when a later frame is transparent where an earlier one
            // was opaque.
            dispose: if quantized.transparent.is_some() {
                DisposalMethod::Background
            } else {
                DisposalMethod::Keep
            },
            ..GifFrame::default()
        };

        self.encoder.write_frame(&frame)?;
        Ok(())
    }

    /// Write the GIF trailer and flush the underlying writer.
    pub fn finish(self) -> Result<(), EncodeError> {
        let mut writer = self.encoder.into_inner()?;
        writer.flush()?;
        Ok(())
    }
}

/// Write an RGBA frame iterator to `path` as a streaming GIF.
///
/// Frames are consumed lazily: at most one frame is quantized at a time.
///
/// # Errors
///
/// Propagates [`EncodeError`] from option validation, the encoder and the
/// filesystem.
pub fn write_gif<I>(
    path: &Path,
    width: u16,
    height: u16,
    options: &GifOptions,
    frames: I,
) -> Result<(), EncodeError>
where
    I: IntoIterator<Item = RgbaImage>,
{
    // Validate before touching the filesystem so bad options never leave an
    // empty output file behind.
    validate_options(width, height, options)?;

    let file = File::create(path)?;
    let mut encoder = GifEncoder::new(BufWriter::new(file), width, height, *options)?;
    for frame in frames {
        encoder.write_frame(&frame)?;
    }
    encoder.finish()
}

fn validate_options(width: u16, height: u16, options: &GifOptions) -> Result<(), EncodeError> {
    if width == 0 || height == 0 {
        return Err(EncodeError::InvalidOptions {
            reason: format!("canvas must be non-zero, got {width}x{height}"),
        });
    }
    if options.fps == 0 {
        return Err(EncodeError::InvalidOptions {
            reason: "fps must be greater than zero".to_owned(),
        });
    }
    if !(MIN_PALETTE_SIZE..=MAX_PALETTE_SIZE).contains(&options.palette_size) {
        return Err(EncodeError::InvalidOptions {
            reason: format!(
                "palette_size {} out of range {MIN_PALETTE_SIZE}..={MAX_PALETTE_SIZE}",
                options.palette_size
            ),
        });
    }
    Ok(())
}

/// Per-frame delay in hundredths of a second: `max(1, round(100 / fps))`.
///
/// Integer half-up rounding keeps this exact and free of floating point error.
fn frame_delay(fps: u16) -> u16 {
    let fps = u32::from(fps);
    let hundredths = (DELAY_UNITS_PER_SECOND + fps / 2) / fps;
    u16::try_from(hundredths.clamp(1, u32::from(u16::MAX))).unwrap_or(1)
}

/// One quantized frame ready to become a `gif::Frame`.
struct QuantizedFrame {
    palette: Vec<u8>,
    indices: Vec<u8>,
    transparent: Option<u8>,
}

fn quantize(image: &RgbaImage, palette_size: u16, transparent: bool) -> QuantizedFrame {
    let has_transparency = transparent
        && image
            .pixels()
            .any(|pixel| pixel.0[3] < TRANSPARENT_ALPHA_THRESHOLD);
    if has_transparency {
        quantize_with_transparency(image, palette_size)
    } else {
        quantize_opaque(image, palette_size)
    }
}

fn quantize_opaque(image: &RgbaImage, palette_size: u16) -> QuantizedFrame {
    let buffer = image_buffer(image);
    let indexed = wu_pipeline(PaletteSize::from_u16_clamped(palette_size))
        .input_image(buffer.as_ref())
        .output_srgb8_indexed_image();

    QuantizedFrame {
        palette: palette_bytes(indexed.palette()),
        indices: indexed.indices().to_vec(),
        transparent: None,
    }
}

/// Reserve palette index 0 for transparency and quantize only opaque pixels.
fn quantize_with_transparency(image: &RgbaImage, palette_size: u16) -> QuantizedFrame {
    let mut opaque_pixels = Vec::new();
    let mut transparent_mask = Vec::with_capacity(image.pixels().len());
    for pixel in image.pixels() {
        let [red, green, blue, alpha] = pixel.0;
        let is_transparent = alpha < TRANSPARENT_ALPHA_THRESHOLD;
        transparent_mask.push(is_transparent);
        if !is_transparent {
            opaque_pixels.push(Srgb::new(red, green, blue));
        }
    }

    if opaque_pixels.is_empty() {
        return QuantizedFrame {
            palette: vec![0, 0, 0],
            indices: vec![TRANSPARENT_INDEX; transparent_mask.len()],
            transparent: Some(TRANSPARENT_INDEX),
        };
    }

    // One palette slot belongs to transparency, so opaque pixels get the rest.
    let opaque_palette = wu_pipeline(PaletteSize::from_u16_clamped(palette_size - 1))
        .input_slice(&opaque_pixels)
        .expect("opaque pixels are non-empty and below the quantizer pixel limit")
        .output_srgb8_palette();

    let buffer = image_buffer(image);
    let indexed = Pipeline::new()
        .ditherer(None)
        .quantize_method(opaque_palette)
        .input_image(buffer.as_ref())
        .output_srgb8_indexed_image();

    let mut palette = Vec::with_capacity((indexed.palette().len() + 1) * 3);
    palette.extend_from_slice(&[0, 0, 0]);
    palette.extend(palette_bytes(indexed.palette()));

    let indices = indexed
        .indices()
        .iter()
        .zip(&transparent_mask)
        .map(|(&index, &is_transparent)| {
            if is_transparent {
                TRANSPARENT_INDEX
            } else {
                // The opaque palette holds at most `palette_size - 1` colors,
                // so shifting by one can never overflow a u8.
                index + 1
            }
        })
        .collect();

    QuantizedFrame {
        palette,
        indices,
        transparent: Some(TRANSPARENT_INDEX),
    }
}

/// Wu's quantizer, no dithering, sequential by default.
///
/// Not enabling the `threads` feature keeps this deterministic and avoids
/// requiring a thread pool.
fn wu_pipeline(palette_size: PaletteSize) -> Pipeline {
    Pipeline::new()
        .palette_size(palette_size)
        .ditherer(None)
        .quantize_method(QuantizeMethod::Wu)
}

/// Copy the RGBA frame into the sRGB image type expected by `quantette`.
fn image_buffer(image: &RgbaImage) -> ImageBuf<Srgb<u8>> {
    let (width, height) = image.dimensions();
    let pixels = image
        .pixels()
        .map(|pixel| {
            let [red, green, blue, _] = pixel.0;
            Srgb::new(red, green, blue)
        })
        .collect();

    ImageBuf::new(width, height, pixels)
        .expect("GIF canvases are u16-sized, so the pixel buffer always fits the quantizer limits")
}

fn palette_bytes(colors: &[Srgb<u8>]) -> Vec<u8> {
    colors
        .iter()
        .flat_map(|color| [color.red, color.green, color.blue])
        .collect()
}
