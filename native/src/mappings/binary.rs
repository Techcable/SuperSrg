///! The reference implementation of the supersrg binary mapping format.
///! All values are in big endian format, which is the 'network byte order'.
///! The format is versioned and starts with a magic number header.
///! Strings are prefixed with their length as a u16, and are UTF-8 encoded, unless specified otherwise.
///! By convention, the recomended file extension for these binary mappings should be `.srg.dat`,
///! though this may change in the future.
///! Duplicate mappings are forbidden, and it is recomended that tools treat them as an error.
///! Empty strings are forbidden unless explicity allowed.
///!
///! The format is as follows:
///! - `SuperSrg binary mappings\0` (UTF-8 encoded, null-termianted) Magic header identifying the file's format
///! - `version` (u32) The version of the mappings format, currently 1
///! - `compression` (UTF8) The compression algorithm of the following array, or empty for uncompressed.
///!   - Allowed compression algorithms are `lzma2`, `lz4-frame`, and `gzip`
///!   - Implementations are only required to support uncompressed data,
///!     though `lz4-frame` is encouraged and used in supersrg by default.
///! - `num_classes` (u64) The number of classes in the following list.
///!   - `original_name` (UTF8) The original name of the class, encoded as a java internal name
///!   - `revised_name` (UTF8) The revised name of the class, or an empty string if unchanged.
///!   - `num_methods` (u32) The number of entries in the following list.
///!     - `original_name` (UTF8) The original name of the method.
///!     - `revised_name` (UTF8) The revised name of the method.
///!       - May be empty to indicate no change if and only if the signature has changed.
///!     - `original_signature` (UTF8) The original signature of the method.
///!     - `revised_signature` (UTF8) The revised signature of the method, or empty to indicate it must be infered.
///!   - `num_fields` (u32) The number of entries in the following list.
///!     - `original_name` (UTF8) The original name of the field.
///!     - `revised_name` (UTF8) The revised name of the field.
///!
use std::io::{self, Write, BufRead, BufWriter, BufReader};
use std::convert::TryFrom;
use std::str::FromStr;
use std::fmt::{self, Display, Formatter};
use std::error::Error;
use std::fs::File;
use std::path::Path;

use lz4::{EncoderBuilder as Lz4EncoderBuilder, Decoder as Lz4Decoder};
use ordermap::OrderMap;
use string_cache::DefaultAtom;

use mappings::{MappingsBuilder, MappingsIterator, MappingsSnapshot};
use types::{PooledMethodData, PooledJavaClass, PooledFieldData, JavaClass, JavaClassLookup, NameParseError, MethodDescriptorParseError, MethodSignature, MethodDataLookup, FieldDataLookup};
use utils::{SimpleEncoder, SimpleDecoder, SeaHashOrderMap};

#[derive(Default)]
pub struct MappingsEncoderBuilder {
    compressor: MappingsCompressor,
}
impl MappingsEncoderBuilder {
    #[inline]
    fn new() -> Self {
        Default::default()
    }
    #[inline]
    fn lz4_compression(self, level: u32) -> Self {
        let mut builder = Lz4EncoderBuilder::new();
        builder.level(level);
        self.compression(MappingsCompressor::Lz4(builder))
    }
    #[inline]
    fn compression(mut self, compressor: MappingsCompressor) -> Self {
        self.compressor = compressor;
        self
    }
    fn build<W: Write>(self, writer: W) -> MappingsEncoder<W> {
        MappingsEncoder {
            writer,
            compressor: self.compressor,
        }
    }
}
pub const MAGIC_HEADER: &[u8] = b"SuperSrg binary mappings\0";
pub const CURRENT_VERSION: u32 = 1;
pub struct MappingsEncoder<W: Write> {
    writer: W,
    compressor: MappingsCompressor,
}
impl MappingsEncoder<BufWriter<File>> {
    #[inline]
    pub fn create_path(path: &Path) -> io::Result<Self> {
        Ok(Self::new(BufWriter::new(File::create(path)?)))
    }
}
impl<W: Write> MappingsEncoder<W> {
    #[inline]
    pub fn new(writer: W) -> Self {
        MappingsEncoderBuilder::default().build(writer)
    }
    pub fn encode(self, mappings: &MappingsSnapshot) -> Result<W, io::Error> {
        let mut encoder = SimpleEncoder(self.writer);
        encoder.0.write_all(MAGIC_HEADER)?;
        encoder.write_u32(CURRENT_VERSION)?;
        match self.compressor {
            MappingsCompressor::Lz4(builder) => {
                encoder.write_string(MappingsCompressionFormat::Lz4.id())?;
                let encoder = builder.build(encoder.0)?;
                let (writer, result) = CompressedMappingsEncoder(encoder)
                    .encode(mappings)?
                    .finish();
                result?;
                Ok(writer)
            }
            MappingsCompressor::Uncompressed => {
                encoder.write_string(
                    MappingsCompressionFormat::Uncompressed.id(),
                )?;
                CompressedMappingsEncoder(encoder.0).encode(mappings)
            }
        }
    }
}
struct CompressedMappingsEncoder<W: Write>(W);
impl<W: Write> CompressedMappingsEncoder<W> {
    fn encode<'a>(self, mappings: &'a MappingsSnapshot) -> Result<W, io::Error> {
        #[derive(Default)]
        struct ClassData<'a> {
            renamed: Option<&'a PooledJavaClass>,
            fields: Vec<(&'a PooledFieldData, PooledFieldData)>,
            methods: Vec<(&'a PooledMethodData, PooledMethodData)>,
        }
        let mut encoder = SimpleEncoder(self.0);
        let classes_iter = mappings.classes();
        let hint = classes_iter.size_hint();
        let expected_size = hint.1.unwrap_or(hint.0);
        let mut known_classes: SeaHashOrderMap<&'a PooledJavaClass, ClassData<'a>> = OrderMap::with_capacity_and_hasher(expected_size, Default::default());
        for (original, renamed) in classes_iter {
            known_classes.insert(
                original,
                ClassData {
                    renamed: Some(renamed),
                    fields: Vec::new(),
                    methods: Vec::new(),
                },
            );
        }
        for (original, renamed) in mappings.fields() {
            let class_data = known_classes.entry(&original.class).or_insert_with(
                Default::default,
            );
            if original.name != renamed.name {
                class_data.fields.push((original, renamed.into_owned()));
            }
        }
        for (original, renamed) in mappings.methods() {
            let class_data = known_classes.entry(&original.class).or_insert_with(
                Default::default,
            );
            if original.name != renamed.name {
                class_data.methods.push((original, renamed.into_owned()));
            }
        }
        encoder.write_u64(known_classes.len() as u64)?;
        for (original, class_data) in known_classes.iter() {
            encoder.write_string(original.internal_name())?;
            encoder.write_string(
                class_data
                    .renamed
                    .map(|x| x.internal_name())
                    .unwrap_or(""),
            )?;
            encoder.write_u32(
                u32::try_from(class_data.methods.len()).expect(
                    "Too many methods",
                ),
            )?;
            for &(original, ref renamed) in &class_data.methods {
                encoder.write_string(&original.name)?;
                assert_ne!(original.name, renamed.name);
                encoder.write_string(&renamed.name)?;
                encoder.write_string(&original.signature)?;
                encoder.write_string("")?; // Renamed signature is mostly a waste of space
            }
            encoder.write_u32(
                u32::try_from(class_data.fields.len()).expect(
                    "Too many fields",
                ),
            )?;
            for &(original, ref renamed) in &class_data.fields {
                encoder.write_string(&original.name)?;
                encoder.write_string(&renamed.name)?;
                assert_ne!(original.name, renamed.name);
            }
        }
        Ok(encoder.0)
    }
}
pub struct MappingsDecoder<R: BufRead> {
    reader: R,
}
impl MappingsDecoder<BufReader<File>> {
    #[inline]
    pub fn from_path(path: &Path) -> io::Result<Self> {
        Ok(Self::new(BufReader::new(File::open(path)?)))
    }
}
impl<R: BufRead> MappingsDecoder<R> {
    #[inline]
    pub fn new(reader: R) -> Self {
        MappingsDecoder { reader }
    }
    pub fn decode(self, builder: &mut MappingsBuilder) -> Result<R, BinaryMappingError> {
        let mut decoder = SimpleDecoder::new(self.reader);
        {
            let actual_header = decoder.read_nullterm()?;
            if actual_header != MAGIC_HEADER {
                return Err(BinaryMappingError::UnexpectedHeader(
                    actual_header.to_owned(),
                ));
            }
        }
        let version = decoder.read_u32()?;
        if version != CURRENT_VERSION {
            return Err(BinaryMappingError::UnexpectedVersion(version));
        }
        let compression_format: MappingsCompressionFormat = decoder.read_string()?.parse()?;
        match compression_format {
            MappingsCompressionFormat::Lz4 => {
                let decoder = Lz4Decoder::new(decoder.into_inner())?;
                let buffered = BufReader::new(decoder);
                let (reader, result) = CompressedMappingsDecoder::new(buffered)
                    .decode(builder)?
                    .into_inner()
                    .finish();
                result?;
                Ok(reader)
            }
            MappingsCompressionFormat::Uncompressed => CompressedMappingsDecoder::new(decoder.into_inner()).decode(builder),
            _ => Err(BinaryMappingError::UnsupportedCompression(
                compression_format,
            )),
        }
    }
}
struct CompressedMappingsDecoder<R: BufRead> {
    reader: R,
    lenient: bool,
}
impl<R: BufRead> CompressedMappingsDecoder<R> {
    #[inline]
    fn new(reader: R) -> Self {
        CompressedMappingsDecoder {
            reader,
            lenient: false,
        }
    }
    fn decode(self, builder: &mut MappingsBuilder) -> Result<R, BinaryMappingError> {
        let mut decoder = SimpleDecoder::new(self.reader);
        let num_classes = decoder.read_u64()?;
        builder.classes.reserve(num_classes as usize);
        for _ in 0..num_classes {
            let original_class = JavaClass::parse_internal_name(decoder.read_string()?)?
                .intern();
            let revised_class = {
                let revised_name = decoder.read_string()?;
                if revised_name.is_empty() {
                    original_class.clone()
                } else {
                    JavaClass::parse_internal_name(revised_name)?.intern()
                }
            };
            if revised_class != original_class {
                builder.insert_class(original_class.clone(), revised_class.clone());
            }
            let num_methods = decoder.read_u32()?;
            builder.method_names.reserve(num_methods as usize);
            for _ in 0..num_methods {
                let original_name = DefaultAtom::from(decoder.read_string()?);
                if original_name.is_empty() {
                    return Err(BinaryMappingError::InvalidName(
                        NameParseError::EmptyMemberName,
                    ));
                }
                let revised_name = {
                    let raw_revised_name = decoder.read_string()?;
                    if raw_revised_name.is_empty() {
                        original_name.clone()
                    } else {
                        DefaultAtom::from(raw_revised_name)
                    }
                };
                let original_signature = DefaultAtom::from(decoder.read_string()?);
                MethodSignature::new(&original_signature).parse()?;
                let original_data = PooledMethodData {
                    class: original_class.clone(),
                    name: original_name.clone(),
                    signature: original_signature.clone(),
                };
                let revised_signature = {
                    let raw_revised_signature = decoder.read_string()?;
                    if !raw_revised_signature.is_empty() {
                        MethodSignature::new(raw_revised_signature).parse()?;
                        Some(DefaultAtom::from(raw_revised_signature))
                    } else {
                        None
                    }
                };
                if original_name != revised_name {
                    builder.insert_method(original_data, revised_name);
                } else {
                    let mut changed = false;
                    if let Some(revised_signature) = revised_signature {
                        changed |= revised_signature != original_signature;
                    }
                    if !changed && !self.lenient {
                        return Err(BinaryMappingError::UnchangedMethod(original_data));
                    }
                }
            }
            let num_fields = decoder.read_u32()?;
            builder.field_names.reserve(num_fields as usize);
            for _ in 0..num_fields {
                let original_name = DefaultAtom::from(decoder.read_string()?);
                if original_name.is_empty() {
                    return Err(BinaryMappingError::InvalidName(
                        NameParseError::EmptyMemberName,
                    ));
                }
                let revised_name = DefaultAtom::from(decoder.read_string()?);
                if revised_name.is_empty() {
                    return Err(BinaryMappingError::InvalidName(
                        NameParseError::EmptyMemberName,
                    ));
                }
                let original_data = PooledFieldData {
                    class: original_class.clone(),
                    name: original_name.clone(),
                };
                if original_name == revised_name {
                    return Err(BinaryMappingError::UnchangedField(original_data));
                }
                builder.insert_field(original_data, revised_name);
            }
        }
        // NOTE: lz4 demands we read the entire compressed stream, so insert a check here
        let mut trailing = Vec::new();
        decoder.reader.read_to_end(&mut trailing)?;
        if !trailing.is_empty() {
            return Err(BinaryMappingError::UnexpectedTrailing(trailing));
        }
        Ok(decoder.into_inner())
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MappingsCompressionFormat {
    Lz4,
    Lzma2,
    Gzip,
    Uncompressed,
}
impl MappingsCompressionFormat {
    #[inline]
    fn id(&self) -> &'static str {
        match *self {
            MappingsCompressionFormat::Lz4 => "lz4-frame",
            MappingsCompressionFormat::Lzma2 => "lzma2",
            MappingsCompressionFormat::Gzip => "gzip",
            MappingsCompressionFormat::Uncompressed => "",
        }
    }
}
impl FromStr for MappingsCompressionFormat {
    type Err = BinaryMappingError;
    #[inline]
    fn from_str(id: &str) -> Result<MappingsCompressionFormat, BinaryMappingError> {
        match id {
            "lz4-frame" => Ok(MappingsCompressionFormat::Lz4),
            "lzma2" => Ok(MappingsCompressionFormat::Lzma2),
            "gzip" => Ok(MappingsCompressionFormat::Gzip),
            "" => Ok(MappingsCompressionFormat::Uncompressed),
            _ => Err(BinaryMappingError::ForbiddenCompression(id.to_owned())),
        }
    }
}
pub enum MappingsCompressor {
    Lz4(Lz4EncoderBuilder),
    Uncompressed,
}
impl Default for MappingsCompressor {
    #[inline]
    fn default() -> Self {
        let mut builder = Lz4EncoderBuilder::new();
        builder.level(1);
        MappingsCompressor::Lz4(builder)
    }
}
#[derive(Debug)]
pub enum BinaryMappingError {
    IOError(io::Error),
    /// Indicates that the compression algorithm is not part of the standard
    ForbiddenCompression(String),
    /// Indicates that the compression algorithim isn't currently supported by the implementation
    UnsupportedCompression(MappingsCompressionFormat),
    /// Indicates that a name is invalid
    InvalidName(NameParseError),
    /// Indicates that a method is unchanged, and thus redundant
    UnchangedMethod(PooledMethodData),
    UnchangedField(PooledFieldData),
    InvalidMethodDescriptor(MethodDescriptorParseError),
    UnexpectedHeader(Vec<u8>),
    UnexpectedVersion(u32),
    UnexpectedTrailing(Vec<u8>),
}
impl From<io::Error> for BinaryMappingError {
    #[inline]
    fn from(cause: io::Error) -> BinaryMappingError {
        BinaryMappingError::IOError(cause)
    }
}
impl From<NameParseError> for BinaryMappingError {
    #[inline]
    fn from(cause: NameParseError) -> BinaryMappingError {
        BinaryMappingError::InvalidName(cause)
    }
}
impl From<MethodDescriptorParseError> for BinaryMappingError {
    #[inline]
    fn from(cause: MethodDescriptorParseError) -> BinaryMappingError {
        BinaryMappingError::InvalidMethodDescriptor(cause)
    }
}
impl Display for BinaryMappingError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            BinaryMappingError::IOError(ref cause) => write!(f, "IOError: {}", cause),
            BinaryMappingError::ForbiddenCompression(ref id) => write!(f, "Nonstandard compression: {}", id),
            BinaryMappingError::UnsupportedCompression(c) => write!(f, "Unsupported compression: {}", c.id()),
            BinaryMappingError::InvalidName(ref cause) => write!(f, "Invalid name: {}", cause),
            BinaryMappingError::UnchangedMethod(ref cause) => write!(f, "Unchanged, redundant method: {}", cause.borrowed()),
            BinaryMappingError::UnchangedField(ref cause) => write!(f, "Unchanged, redundant field: {}", cause.borrowed()),
            BinaryMappingError::InvalidMethodDescriptor(ref cause) => write!(f, "Invalid method descriptor: {}", cause),
            BinaryMappingError::UnexpectedHeader(ref cause) => write!(f, "Unexpected header: {:?}", cause),
            BinaryMappingError::UnexpectedVersion(version) => write!(f, "Unexpected version: {}", version),
            BinaryMappingError::UnexpectedTrailing(ref trailing) => {
                write!(f, "Unexpected trailing data: ")?;
                for b in trailing {
                    write!(f, "{:X}", b)?;
                }
                Ok(())
            }
        }
    }
}
impl Error for BinaryMappingError {
    fn description(&self) -> &'static str {
        match *self {
            BinaryMappingError::IOError(_) => "IOError",
            BinaryMappingError::ForbiddenCompression(_) => "Nonstandard compression",
            BinaryMappingError::UnsupportedCompression(_) => "Unsupported compression",
            BinaryMappingError::InvalidName(_) => "Invalid name",
            BinaryMappingError::UnchangedMethod(_) => "Unchanged, redundant method",
            BinaryMappingError::UnchangedField(_) => "Unchanged, redundant field",
            BinaryMappingError::InvalidMethodDescriptor(_) => "Invalid method descriptor",
            BinaryMappingError::UnexpectedHeader(_) => "Unexpected header",
            BinaryMappingError::UnexpectedVersion(_) => "Unexpected version",
            BinaryMappingError::UnexpectedTrailing(_) => "Unexpected trailing data",
        }
    }
    fn cause(&self) -> Option<&Error> {
        match *self {
            BinaryMappingError::IOError(ref cause) => Some(cause),
            BinaryMappingError::InvalidName(ref cause) => Some(cause),
            BinaryMappingError::InvalidMethodDescriptor(ref cause) => Some(cause),
            _ => None,
        }
    }
}
