use std::error::Error;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::fs::File;
use super::MappingsBuilder;

pub mod srg;
pub mod csrg;

pub use self::srg::{SrgParseError, SrgMappingsParser};
pub use self::csrg::{CompactSrgParser, CompactSrgParseError};

pub trait MappingsParser: Default {
    type Error: Error + From<io::Error>;
    #[inline]
    fn parse_text(&mut self, text: &str) -> Result<(), Self::Error> {
        for line in text.lines() {
            self.parse_line(line)?
        }
        Ok(())
    }
    #[inline]
    fn read_path(&mut self, path: &Path) -> Result<(), Self::Error> {
        let mut buffered = BufReader::new(File::open(path)?);
        self.read(&mut buffered)
    }
    fn read<R: BufRead>(&mut self, input: &mut R) -> Result<(), Self::Error> {
        let mut line = String::new();
        loop {
            line.clear();
            let num_read = input.read_line(&mut line)?;
            if num_read > 0 {
                self.parse_line(&line)?;
            } else {
                break;
            }
        }
        Ok(())
    }
    fn finish(self) -> MappingsBuilder;
    fn parse_line(&mut self, &str) -> Result<(), Self::Error>;
}
