//! Simple tool that uses the generation utility on the command line.

use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use embedded_font_generator::{BitFlow, FontMode, GenerationError};

xflags::xflags! {
    /// Tool to convert png images to a simple bitmap font format readable in embedded software.
    cmd app {
        /// Path to write output to
        optional -o, --output output: PathBuf
        /// The mode in which the font should be generated
        ///
        /// row: Each row is read and written directly to the font file, there is no alignment
        /// column-byte: 8 Pixel Columns are read from left to right and then top to bottom,
        ///              the data is byte aligned in multiples of 8.
        optional -m, --mode mode: FontMode
        /// The flow in which the bits inside a byte flow
        ///
        /// big: The first read pixel is the most significant bit
        /// small: The first read pixel is the least significant bit
        optional -f, --flow flow: BitFlow
        /// Generate a complete directory
        cmd generate-dir {
            /// Path to the directory
            required dir_path: PathBuf
        }
        /// Generate a single file as font
        cmd generate-file {
            /// Path to the file
            required file_path: PathBuf
        }
        /// Dump a file
        cmd dump {
            /// The format to dump to
            required format: DumpFormat
            /// The file to dump
            required file_path: PathBuf
        }
    }
}

fn main() {
    let args = App::from_env_or_exit();

    if let Err(e) = run(args) {
        eprintln!("Error while generating font: {}", e);
    }
}

/// Run the command with the given arguments
fn run(args: App) -> embedded_font_generator::Result<()> {
    let font_mode = args.mode.unwrap_or_default();
    let bit_flow = args.flow.unwrap_or_default();
    match args.subcommand {
        AppCmd::GenerateFile(GenerateFile { file_path }) => match args.output {
            Some(out_path) => {
                let f = File::create(&out_path).map_err(GenerationError::IoError)?;
                generate_file(&file_path, font_mode, bit_flow, &mut BufWriter::new(f))
            }
            None => generate_file(&file_path, font_mode, bit_flow, &mut io::stdout().lock()),
        },
        AppCmd::GenerateDir(GenerateDir { dir_path }) => match args.output {
            Some(out_path) => {
                let f = File::create(&out_path).map_err(GenerationError::IoError)?;
                generate_dir(&dir_path, font_mode, bit_flow, &mut BufWriter::new(f))
            }
            None => generate_dir(&dir_path, font_mode, bit_flow, &mut io::stdout().lock()),
        },
        AppCmd::Dump(Dump { format, file_path }) => {
            let bytes = fs::read(file_path)?;
            match format {
                DumpFormat::Binary => {
                    for (byte, i) in bytes.into_iter().zip(1..) {
                        print!("{:08b} ", byte);
                        if i % 8 == 0 {
                            println!("");
                        }
                    }
                    println!("");
                }
                DumpFormat::Hex => {
                    for (byte, i) in bytes.into_iter().zip(1..) {
                        print!("{:#04x} ", byte);
                        if i % 8 == 0 {
                            println!("");
                        }
                    }
                    println!("");
                }
            }
            Ok(())
        }
    }
}

/// generate single letter file
fn generate_file(
    file_path: &Path,
    font_mode: FontMode,
    bit_flow: BitFlow,
    mut out: &mut dyn Write,
) -> embedded_font_generator::Result<()> {
    let data = fs::read(file_path).map_err(GenerationError::IoError)?;

    embedded_font_generator::generate_monochromatic(&data, font_mode, bit_flow, &mut out)
}

/// Generate all images in a directory as font
fn generate_dir(
    dir_path: &Path,
    font_mode: FontMode,
    bit_flow: BitFlow,
    out: &mut dyn Write,
) -> embedded_font_generator::Result<()> {
    let dir_ents = fs::read_dir(dir_path)?;
    let mut entries: Vec<_> = dir_ents.filter_map(|res| res.ok()).collect();
    entries.sort_unstable_by_key(|ent| ent.file_name());
    for ent in entries {
        eprintln!("Generating for {}", ent.path().display());
        generate_file(&ent.path(), font_mode, bit_flow, out)?;
    }

    Ok(())
}

/// The format to show the dump in
#[derive(Debug, Clone, Copy)]
pub enum DumpFormat {
    /// Binary number
    Binary,
    /// Hexadecimal numbers
    Hex,
}

impl FromStr for DumpFormat {
    type Err = DumpFormatParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "binary" => Ok(Self::Binary),
            "hex" => Ok(Self::Hex),
            s => Err(DumpFormatParseError(s.to_owned())),
        }
    }
}

/// An error that occurs when trying to parse a dump format that doesn't exist
#[derive(Debug, Clone, thiserror::Error)]
#[error("Unsupported dump format: {0}")]
pub struct DumpFormatParseError(String);
