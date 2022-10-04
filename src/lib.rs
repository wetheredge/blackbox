#![warn(unsafe_code, clippy::std_instead_of_alloc, clippy::std_instead_of_core)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[macro_use]
pub mod common;

pub mod betaflight;
pub mod inav;

pub mod log;
pub mod parser;

use alloc::vec::Vec;

use memchr::memmem;
use tracing::instrument;

pub use self::log::Log;
use self::parser::ParseResult;

#[derive(Debug)]
pub struct File<'data> {
    offsets: Vec<usize>,
    data: &'data [u8],
}

impl<'data> File<'data> {
    pub fn new(data: &'data [u8]) -> Self {
        let offsets = memmem::find_iter(data, parser::MARKER).collect();
        Self { offsets, data }
    }

    pub fn log_count(&self) -> usize {
        self.offsets.len()
    }

    /// # Panics
    ///
    /// This panics if given an `index` greater than or equal to the number of
    /// logs in the file.
    #[instrument(level = "trace", skip(self, config), fields(offset))]
    pub fn parse_by_index<'config>(
        &self,
        config: &'config parser::Config,
        index: usize,
    ) -> ParseResult<Log<'data>> {
        tracing::trace!(?config);

        let start = self.offsets[index];
        tracing::Span::current().record("offset", start);

        Log::parse(config, &self.data[start..])
    }
}
