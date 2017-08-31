use std::io::{self, Write};

use mappings::MappingsSnapshot;

pub mod srg;
pub use self::srg::SrgEncoder;

pub trait MappingsEncoder<'a> {
    fn new(mappings: &'a MappingsSnapshot) -> Self;
    /// Encode the mappings to the specified output
    #[inline]
    fn write<W: Write>(&self, out: &mut W) -> io::Result<()>;
}
