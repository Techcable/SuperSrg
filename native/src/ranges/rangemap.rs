use std::cmp::{PartialOrd, Ordering, max};
use std::io::{self, Cursor, Read, BufRead, BufWriter, Write};
use std::fmt;
use std::path::PathBuf;
use std::fs::File;

use serde::de::{Deserialize, Deserializer, Error as SerdeDeError};

use types::{PooledFieldData, PooledMethodData, FieldDataLookup, MethodDataLookup, FieldData, MethodData, MethodSignature};
use utils::{SimpleDecoder, SeaHashOrderMap, SeaHashOrderSet, SeaHashSerializableOrderMap};
use std::env;

// NOTE: No encapsulation because I'm lazy

#[derive(Clone, Debug)]
pub struct RangeMap {
    pub files: SeaHashOrderMap<String, FileRanges>,
}
impl RangeMap {
    pub fn debug_dump(&self) {
        if !cfg!(debug_assertions) { return };
        let debug_dir = PathBuf::from("testing/debug");
        if debug_dir.is_dir() && env::var("DUMP_RANGEMAP").is_ok() {
            let rangemap_file = debug_dir.join("rangeMap.txt");
            let mut writer = BufWriter::new(File::create(&rangemap_file).unwrap());
            write!(writer, "{:#?}", self).unwrap();
            eprintln!("Dumped rangemap to {}", rangemap_file.display());
        }
    }
}
#[derive(Clone, Debug)]
pub struct FileRanges {
    pub hash: Option<Vec<u8>>,
    pub field_references: Vec<FieldReference>,
    pub method_references: Vec<MethodReference>,
}
#[derive(Clone, Debug)]
pub enum MemberReference<'a> {
    Field(&'a FieldReference),
    Method(&'a MethodReference),
}
impl<'a> MemberReference<'a> {
    #[inline]
    pub fn name(&self) -> &str {
        match *self {
            MemberReference::Field(field) => &field.referenced_field.name,
            MemberReference::Method(method) => &method.referenced_method.name,
        }
    }
    #[inline]
    pub fn location(&self) -> FileLocation {
        match *self {
            MemberReference::Field(field) => field.location,
            MemberReference::Method(method) => method.location,
        }
    }
}
impl FileRanges {
    pub fn sorted(&self) -> Vec<MemberReference> {
        let mut result = Vec::with_capacity(self.field_references.len() + self.method_references.len());
        for fieldref in &self.field_references {
            result.push(MemberReference::Field(fieldref));
        }
        for methodref in &self.method_references {
            result.push(MemberReference::Method(methodref));
        }
        result.sort_by(|first, second| {
            let first_location = first.location();
            let second_location = second.location();
            assert!(
                !first_location.has_overlap(second_location),
                "Members overlap: {:?} and {:?}",
                first,
                second
            );
            first_location.start.cmp(&second_location.start)
        });
        result
    }
}
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RangeMapDeserializer {
    file_hashes: SeaHashSerializableOrderMap<String, ::serde_bytes::ByteBuf>,
    field_references: SeaHashSerializableOrderMap<String, Vec<FieldReference>>,
    method_references: SeaHashSerializableOrderMap<String, Vec<MethodReference>>,
}
impl RangeMapDeserializer {
    pub fn read<R: Read>(input: &mut R) -> Result<RangeMapDeserializer, ::rmp_serde::decode::Error> {
        use rmp_serde::decode::Deserializer;
        let mut de = Deserializer::new(input);
        Self::deserialize(&mut de)
    }
    pub fn build(mut self) -> RangeMap {
        let expected_size = max(
            max(self.field_references.len(), self.method_references.len()),
            self.file_hashes.len(),
        );
        let mut file_names = SeaHashOrderSet::with_capacity_and_hasher(expected_size, Default::default());
        for file_name in self.file_hashes.keys() {
            file_names.insert(file_name.clone(), ());
        }
        for file_name in self.field_references.keys() {
            file_names.insert(file_name.clone(), ());
        }
        for file_name in self.method_references.keys() {
            file_names.insert(file_name.clone(), ());
        }
        let mut files = SeaHashOrderMap::with_capacity_and_hasher(file_names.len(), Default::default());
        for (name, _) in file_names {
            let hash = self.file_hashes.remove(&name).map(
                ::serde_bytes::ByteBuf::into,
            );
            let field_references = self.field_references.remove(&name).unwrap_or_else(Vec::new);
            let method_references = self.method_references.remove(&name).unwrap_or_else(
                Vec::new,
            );
            let file_range = FileRanges {
                hash,
                field_references,
                method_references,
            };
            files.insert(name, file_range);
        }
        RangeMap { files }
    }
}
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct FieldReference {
    pub location: FileLocation,
    pub referenced_field: PooledFieldData,
}
impl FieldReference {
    fn decode<R: BufRead + fmt::Debug>(decoder: &mut SimpleDecoder<R>) -> Result<FieldReference, io::Error> {
        let location = FileLocation::decode(decoder)?;
        let field_name = decoder.read_string()?.to_owned();
        let field = FieldData::parse_internal_name(&field_name)
            .map(|d| d.intern())
            .unwrap_or_else(|e| {
                panic!("Invalid name ({}): {:?} for {:?}", e, field_name, &decoder)
            });
        Ok(FieldReference {
            location,
            referenced_field: field,
        })
    }
}
impl<'de> Deserialize<'de> for FieldReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_bytes::ByteBuf;
        let data = ByteBuf::deserialize(deserializer)?;
        let cursor = Cursor::new(&data);
        let mut decoder = SimpleDecoder::new(cursor);
        let fieldref = FieldReference::decode(&mut decoder).map_err(
            D::Error::custom,
        )?;
        assert_eq!(data.len() as u64, decoder.into_inner().position());
        Ok(fieldref)
    }
}
impl PartialOrd for FieldReference {
    #[inline]
    fn partial_cmp(&self, other: &FieldReference) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FieldReference {
    #[inline]
    fn cmp(&self, other: &FieldReference) -> Ordering {
        self.location.cmp(&other.location)
    }
}
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct MethodReference {
    pub location: FileLocation,
    pub referenced_method: PooledMethodData,
}
impl MethodReference {
    fn decode<R: BufRead>(decoder: &mut SimpleDecoder<R>) -> Result<Self, io::Error> {
        let location = FileLocation::decode(decoder)?;
        let method_name = decoder.read_string()?.to_owned();
        let signature_descriptor = decoder.read_string()?;
        let signature = MethodSignature::new(signature_descriptor);
        signature.parse().unwrap_or_else(|e| {
            panic!("Invalid descriptor ({}): {:?}", e, signature_descriptor)
        });
        let method = MethodData::parse_internal_name(&method_name, signature)
            .unwrap()
            .intern();
        Ok(MethodReference {
            location,
            referenced_method: method,
        })
    }
}
impl<'de> Deserialize<'de> for MethodReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_bytes::ByteBuf;
        let data = ByteBuf::deserialize(deserializer)?;
        let cursor = Cursor::new(&data);
        let mut decoder = SimpleDecoder::new(cursor);
        let methodref = MethodReference::decode(&mut decoder).map_err(
            D::Error::custom,
        )?;
        assert_eq!(data.len() as u64, decoder.into_inner().position());
        Ok(methodref)
    }
}
impl PartialOrd for MethodReference {
    #[inline]
    fn partial_cmp(&self, other: &MethodReference) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MethodReference {
    #[inline]
    fn cmp(&self, other: &MethodReference) -> Ordering {
        self.location.cmp(&other.location)
    }
}
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Debug)]
pub struct FileLocation {
    pub start: u32,
    pub end: u32,
}
impl FileLocation {
    /// Deserialize a file location from its binary representation
    #[inline]
    fn decode<R: BufRead>(decoder: &mut SimpleDecoder<R>) -> Result<FileLocation, io::Error> {
        let start = decoder.read_u32()?;
        let end = decoder.read_u32()?;
        Ok(FileLocation { start, end })
    }
    #[inline]
    pub fn has_overlap(&self, other: FileLocation) -> bool {
        if self > &other {
            // other, self
            other.end > self.start
        } else {
            // self, other
            self.end > other.start
        }
    }
    #[inline]
    pub fn size(&self) -> u32 {
        debug_assert!(self.end >= self.start);
        self.end - self.start
    }
}
