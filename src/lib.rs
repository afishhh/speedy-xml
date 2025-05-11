#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![warn(clippy::missing_errors_doc)]

pub mod escape;
mod lut;
pub mod reader;
pub mod writer;

pub use reader::Reader;
pub use writer::Writer;
