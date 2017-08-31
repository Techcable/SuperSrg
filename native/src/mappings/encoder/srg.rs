use std::io::{self, Write};

use super::MappingsEncoder;
use mappings::{MappingsSnapshot, MappingsIterator};
use types::{JavaClassLookup, FieldDataLookup, MethodDataLookup};

pub struct SrgEncoder<'a> {
    mappings: &'a MappingsSnapshot,
}
impl<'a> MappingsEncoder<'a> for SrgEncoder<'a> {
    #[inline]
    fn new(mappings: &'a MappingsSnapshot) -> Self {
        SrgEncoder { mappings }
    }
    #[inline]
    fn write<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for (original, renamed) in self.mappings.classes() {
            out.write_all(b"CL: ")?;
            out.write_all(original.internal_name().as_bytes())?;
            out.write_all(b" ")?;
            out.write_all(renamed.internal_name().as_bytes())?;
            out.write_all(b"\n")?;
        }
        for (original, renamed) in self.mappings.fields() {
            out.write_all(b"FD: ")?;
            out.write_all(original.class().internal_name().as_bytes())?;
            out.write_all(b"/")?;
            out.write_all(original.name().as_bytes())?;
            out.write_all(b" ")?;
            out.write_all(renamed.class().internal_name().as_bytes())?;
            out.write_all(b"/")?;
            out.write_all(renamed.name().as_bytes())?;
            out.write_all(b"\n")?;
        }
        for (original, renamed) in self.mappings.methods() {
            out.write_all(b"MD: ")?;
            out.write_all(original.class().internal_name().as_bytes())?;
            out.write_all(b"/")?;
            out.write_all(original.name().as_bytes())?;
            out.write_all(b" ")?;
            out.write_all(original.signature().as_bytes())?;
            out.write_all(b" ")?;
            out.write_all(renamed.class().internal_name().as_bytes())?;
            out.write_all(b"/")?;
            out.write_all(renamed.name().as_bytes())?;
            out.write_all(b" ")?;
            out.write_all(renamed.signature().as_bytes())?;
            out.write_all(b"\n")?;
        }
        Ok(())
    }
}
