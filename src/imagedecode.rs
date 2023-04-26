//! Decode png images as source files for the generated

pub use zune_png::{error::PngDecodeErrors, zune_core::colorspace::ColorSpace};

use zune_png::{zune_core::result::DecodingResult, PngDecoder};

use crate::FontMode;

/// An iterator
pub struct MonochromaticColorIter(RgbaColorIter);

impl MonochromaticColorIter {
    /// Create a new iterator yielding monochromatic pixel values from the given png data.
    ///
    /// The font mode describes how the
    pub fn new(data: &[u8], font_mode: FontMode) -> crate::Result<Self> {
        let rgba_iter = RgbaColorIter::new(data, font_mode)?;
        Ok(Self(rgba_iter))
    }
}

impl Iterator for MonochromaticColorIter {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        /// The middle of the u8 range
        const U8_HALF: u8 = u8::MAX / 2;

        let rgba = self.0.next()?;
        let is_on = if self.0.color_space.suppports_alpha() {
            rgba.a > U8_HALF
        } else {
            rgba.a > U8_HALF && (rgba.r < U8_HALF || rgba.g < U8_HALF || rgba.b < U8_HALF)
        };
        Some(is_on)
    }
}

/// An iterator over RGBA pixels of a PNG
pub struct RgbaColorIter {
    /// The inner storage of decoded values
    inner: RgbaColorIterInner,
    /// The used color space
    color_space: SupportedColorSpace,
    /// The mode in which the font should be generated
    font_mode: FontMode,
    /// The images width
    width: usize,
    /// The images height
    height: usize,
    /// The size of the character, calculated from width and height respecting alignment
    char_size: usize,
    /// The current iteration index
    idx: usize,
}

impl RgbaColorIter {
    /// Create a new iterator over rgba pixels from png data
    pub fn new(data: &[u8], font_mode: FontMode) -> crate::Result<Self> {
        let mut decoder = PngDecoder::new(data);
        decoder.decode_headers()?;
        let color_space = decoder.get_colorspace().unwrap_or(ColorSpace::Unknown);
        let color_space = SupportedColorSpace::new(color_space)
            .ok_or(crate::GenerationError::UnsupportedColorspace(color_space))?;

        let info = decoder.get_info().ok_or_else(|| {
            crate::GenerationError::PngDecodingError(PngDecodeErrors::GenericStatic(
                "Unable to get image width/height",
            ))
        })?;

        let width = info.width;
        let height = info.height;
        let char_size = calc_char_size(font_mode, width, height);

        let decoded = decoder.decode()?;
        let me = Self {
            inner: match decoded {
                DecodingResult::U8(v) => RgbaColorIterInner::U8(v),
                DecodingResult::U16(v) => RgbaColorIterInner::U16(v),
                DecodingResult::F32(v) => RgbaColorIterInner::F32(v),
                _ => unimplemented!("Unsupported color depth"),
            },
            color_space,
            font_mode,
            width,
            height,
            char_size,
            idx: 0usize,
        };

        Ok(me)
    }
}

/// Calculate the size the complete char has in theory, this might be larger than width * height
/// because of aligment
fn calc_char_size(font_mode: FontMode, width: usize, height: usize) -> usize {
    match font_mode {
        FontMode::Row => width.wrapping_mul(height),
        FontMode::ByteColumn => {
            if height % 8 == 0 {
                width.wrapping_mul(height)
            } else {
                // 8 - (height % 8) + height to calculate next multiple of 8 as height
                let height = 8usize.wrapping_sub(height % 8).wrapping_add(height);
                width.wrapping_mul(height)
            }
        }
    }
}

impl Iterator for RgbaColorIter {
    type Item = Rgba;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == usize::MAX || self.idx >= self.char_size {
            return None;
        }

        let n = match self.font_mode {
            FontMode::Row => self.idx,
            FontMode::ByteColumn => {
                let idx = self.idx;
                let width = self.width;
                // Calculate the start pixel of the current width * 8 block
                let block_px = width.saturating_mul(8);
                let block_idx = idx.checked_div(block_px)?;
                let block_start = block_idx.wrapping_mul(block_px);
                // Calculate the column index
                let column = self.idx.wrapping_sub(block_start) / 8;
                // Calculate row index
                let row = self.idx.wrapping_sub(block_start) % 8;

                block_start
                    .wrapping_add(column)
                    .wrapping_add(row.wrapping_mul(width))
            }
        };

        self.idx = self.idx.saturating_add(1);

        if n >= self.width.wrapping_mul(self.height) {
            return Some(Rgba::ZERO);
        }

        let rgba = self.inner.get_nth_rgba(n, self.color_space)?;
        Some(rgba)
    }
}

/// Storage of various decoded color depth values
enum RgbaColorIterInner {
    /// 8 bit color depth
    U8(Vec<u8>),
    /// 16 bit color depth
    U16(Vec<u16>),
    /// 32 bit float color depth
    F32(Vec<f32>),
}

impl RgbaColorIterInner {
    /// Get the nth rgba pixel in the image, counting starts in the top left corner and goes from
    /// left to right, top to bottom.
    fn get_nth_rgba(&mut self, n: usize, color_space: SupportedColorSpace) -> Option<Rgba> {
        let mut bytes = [0u8; 4];
        let byte_offset = n.saturating_mul(color_space.num_components());
        let bytes_filled = match *self {
            Self::U8(ref mut v) => fill_bytes(
                v.iter().skip(byte_offset).copied(),
                color_space.num_components(),
                &mut bytes,
            ),
            Self::U16(ref mut v) => fill_bytes(
                v.iter().skip(byte_offset).copied().map(|val| {
                    u8::try_from(val / 256)
                        .unwrap_or_else(|_| unreachable!("Every u16 / 256 should be a valid u8"))
                }),
                color_space.num_components(),
                &mut bytes,
            ),
            Self::F32(ref mut v) => fill_bytes(
                v.iter().skip(byte_offset).copied().map(f32_to_u8),
                color_space.num_components(),
                &mut bytes,
            ),
        };

        if bytes_filled {
            Some(parse_rgba(bytes, color_space))
        } else {
            None
        }
    }
}

/// Fill the bytes with `num_components` bytes, returns wether enough bytes could be pulled from src.
///
/// if `num_components` is greater than 4 then false is returned.
fn fill_bytes<I: Iterator<Item = u8>>(
    mut src: I,
    num_components: usize,
    out: &mut [u8; 4],
) -> bool {
    for i in 0..num_components {
        if let Some(b) = out.get_mut(i) {
            match src.next() {
                Some(val) => *b = val,
                None => return false,
            }
        }
    }

    true
}

/// Best effor conversion of a f32 from 0.0 to 1.0 to u8
fn f32_to_u8(val: f32) -> u8 {
    // This is the only real way to convert a f32 between 0 and 1 to a u8
    #![allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    (val * 255f32) as u8
}

/// Parse a single rgba value from 4 bytes and a color space
fn parse_rgba(bytes: [u8; 4], color_space: SupportedColorSpace) -> Rgba {
    match color_space {
        SupportedColorSpace::Rgb => Rgba {
            r: bytes[0],
            g: bytes[1],
            b: bytes[2],
            a: u8::MAX,
        },
        SupportedColorSpace::Rgba => Rgba {
            r: bytes[0],
            g: bytes[1],
            b: bytes[2],
            a: bytes[3],
        },
        SupportedColorSpace::Luma => {
            let val = bytes[0];
            Rgba {
                r: val,
                b: val,
                g: val,
                a: u8::MAX,
            }
        }
        SupportedColorSpace::LumaA => {
            let val = bytes[0];
            Rgba {
                r: val,
                b: val,
                g: val,
                a: bytes[1],
            }
        }
        SupportedColorSpace::Bgr => Rgba {
            b: bytes[0],
            g: bytes[1],
            r: bytes[2],
            a: u8::MAX,
        },
        SupportedColorSpace::Bgra => Rgba {
            b: bytes[0],
            g: bytes[1],
            r: bytes[2],
            a: bytes[3],
        },
    }
}

/// A single Rgba Pixel
#[derive(Clone, Copy, Debug)]
pub struct Rgba {
    /// Red value
    r: u8,
    /// Green value
    g: u8,
    /// Blue value
    b: u8,
    /// Alpha value
    a: u8,
}

impl Rgba {
    /// ZERO value of an Rgba pixel with all components = 0 (completely transparent black)
    const ZERO: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
}

/// Enumeration of all supported color spaces
#[derive(Debug, Clone, Copy)]
pub enum SupportedColorSpace {
    /// RGB color space, alpha will be set to u8::MAX
    Rgb,
    /// RGBA color space, can be used 1:1
    Rgba,
    /// Grayscale, All color components will be set to the grayscale value, alpha will be u8::MAX
    Luma,
    /// Grayscale with transparency, same as Luma but uses the given alpha
    LumaA,
    /// Same as RGB but blue and red are swapped alpha is set to u8::MAX
    Bgr,
    /// Same as RGBA but blue and red are swapped
    Bgra,
}

impl SupportedColorSpace {
    /// Create a new supported color space from a color space supported by zune
    fn new(space: ColorSpace) -> Option<Self> {
        match space {
            ColorSpace::RGB => Some(Self::Rgb),
            ColorSpace::RGBA => Some(Self::Rgba),
            ColorSpace::Luma => Some(Self::Luma),
            ColorSpace::LumaA => Some(Self::LumaA),
            ColorSpace::BGR => Some(Self::Bgr),
            ColorSpace::BGRA => Some(Self::Bgra),
            _ => None,
        }
    }

    /// Get the number of components (values per pixel) of a color space
    fn num_components(self) -> usize {
        match self {
            Self::Rgb => ColorSpace::RGB.num_components(),
            Self::Rgba => ColorSpace::RGBA.num_components(),
            Self::Luma => ColorSpace::Luma.num_components(),
            Self::LumaA => ColorSpace::LumaA.num_components(),
            Self::Bgr => ColorSpace::BGR.num_components(),
            Self::Bgra => ColorSpace::BGRA.num_components(),
        }
    }

    /// Wether this color space has alpha
    fn suppports_alpha(self) -> bool {
        match self {
            Self::Rgb => ColorSpace::RGB.has_alpha(),
            Self::Rgba => ColorSpace::RGBA.has_alpha(),
            Self::Luma => ColorSpace::Luma.has_alpha(),
            Self::LumaA => ColorSpace::LumaA.has_alpha(),
            Self::Bgr => ColorSpace::BGR.has_alpha(),
            Self::Bgra => ColorSpace::BGRA.has_alpha(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Prevent regression of wrong size calculations
    #[test]
    fn calc_char_size_test() {
        assert_eq!(calc_char_size(FontMode::Row, 10, 16), 10 * 16);
        assert_eq!(calc_char_size(FontMode::Row, 10, 20), 10 * 20);
        assert_eq!(calc_char_size(FontMode::ByteColumn, 10, 16), 10 * 16);
        assert_eq!(calc_char_size(FontMode::ByteColumn, 10, 20), 10 * 24);
    }
}
