//! Error type definition for this crate.

use std::io;

use crate::imagedecode::{ColorSpace, PngDecodeErrors};

/// An error that can occur during font generation
#[derive(thiserror::Error, Debug)]
pub enum GenerationError {
    /// The given image has an unsupported colorspace see:
    /// [SupportedColorSpace](crate::imagedecode::SupportedColorSpace)
    #[error("The given colorspace {0:?} is not supported")]
    UnsupportedColorspace(ColorSpace),
    /// An error occured while decoding a given png.
    #[error("Error while decoding png: {0:?}")]
    PngDecodingError(PngDecodeErrors),
    /// Error that occurs when writing to the given
    #[error("Error while writing to the output writer: {0}")]
    OutputWriterError(io::Error),
    /// Another generic io Error
    #[error("An unexpected io error occured: {0}")]
    IoError(#[from] io::Error),
}

impl From<PngDecodeErrors> for GenerationError {
    fn from(e: PngDecodeErrors) -> Self {
        Self::PngDecodingError(e)
    }
}
