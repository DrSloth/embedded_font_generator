//! Utility to create simple font files for embedded devices.

mod error;
mod imagedecode;

pub use error::GenerationError;

use std::{io::Write, str::FromStr};

/// Result type that uses this crates error by default
pub type Result<T, E = GenerationError> = std::result::Result<T, E>;

/// Generate a single monochromatic font
///
/// # Errors
/// An error is returned when the given image data can not be decoded as png or writing to the
/// `out` writer fails.
pub fn generate_monochromatic(
    data: &[u8],
    font_mode: FontMode,
    bit_flow: BitFlow,
    out: &mut impl Write,
) -> crate::Result<()> {
    let decoded = imagedecode::MonochromaticColorIter::new(data, font_mode)?;
    let mut cur_byte = 0;
    let mut i = 7u8;

    for pix in decoded {
        #[allow(clippy::arithmetic_side_effects)] // Wrongly flagged already fixed in 1.70
        {
            match bit_flow {
                BitFlow::Mtl => {
                    cur_byte <<= 1u32;
                    cur_byte |= u8::from(pix);
                }
                BitFlow::Ltm => {
                    cur_byte >>= 1u32;
                    cur_byte |= u8::from(pix) << 7i32;
                }
            }
        }
        if let Some(v) = i.checked_sub(1) {
            i = v;
        } else {
            out.write(&[cur_byte])
                .map_err(GenerationError::OutputWriterError)?;
            i = 7;
            cur_byte = 0;
        }
    }

    Ok(())
}

/// The mode in which the font should be generated
#[derive(Debug, Clone, Copy, Default)]
pub enum FontMode {
    /// The image is read line by line and each pixel is inserted into the resulting font.
    /// There is no alignment.
    #[default]
    Row,
    // Column,
    /// Works in Columns of 8, scans the columns left to right and then top to bottom, aligned by 8.
    ByteColumn,
}

impl FromStr for FontMode {
    type Err = FontModeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "row" => Ok(Self::Row),
            // "column" => Ok(Self::Column),
            "byte-column" | "column-byte" => Ok(Self::ByteColumn),
            _ => Err(FontModeParseError(s.to_owned())),
        }
    }
}

/// A font mode was tried to be parsed that doesn't exist
#[derive(Debug, thiserror::Error)]
#[error("Unsuported font mode: {0}")]
pub struct FontModeParseError(String);

/// In what direction the bits inside a byte flow in a monochromatic font.
#[derive(Clone, Copy, Debug, Default)]
pub enum BitFlow {
    /// Most significant bit to least significant bit
    #[default]
    Mtl,
    /// Least significant bit most significant bit
    Ltm,
}

impl FromStr for BitFlow {
    type Err = BitFlowParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mtl" | "MTL" | "Mtl" | "m2l" | "Most" | "big" | "Big" => Ok(Self::Mtl),
            "ltm" | "LTM" | "Ltm" | "l2m" | "Least" | "small" | "Small" => Ok(Self::Ltm),
            s => Err(BitFlowParseError(s.to_owned())),
        }
    }
}

/// A bit flow was tried to be parsed that doesn't exist
#[derive(Clone, Debug, thiserror::Error)]
#[error("Unsupported byte flow: {0}")]
pub struct BitFlowParseError(String);
