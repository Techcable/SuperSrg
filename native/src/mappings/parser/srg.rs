use std::fmt::{self, Display, Formatter};
use std::error::Error;
use std::io;

use string_cache::DefaultAtom;

use mappings::MappingsBuilder;
use types::{MethodSignature, MethodData, FieldData, JavaClassLookup, JavaClass, MethodDataLookup, FieldDataLookup, NameParseError, MethodDescriptorParseError};
use super::MappingsParser;

pub struct SrgMappingsParser {
    builder: MappingsBuilder,
    pub ignore_package_mappings: bool,
}
impl Default for SrgMappingsParser {
    #[inline]
    fn default() -> Self {
        SrgMappingsParser {
            builder: MappingsBuilder::new(),
            ignore_package_mappings: true, // Package mappings are technically part of the format
        }
    }
}
impl MappingsParser for SrgMappingsParser {
    type Error = SrgParseError;
    #[inline]
    fn finish(self) -> MappingsBuilder {
        self.builder
    }
    fn parse_line(&mut self, line: &str) -> Result<(), Self::Error> {
        if let Some(mapping_type) = line.get(..3) {
            let data = &line[3..];
            let mut words = data.split_whitespace();
            let mut num_words = 0;
            match mapping_type {
                "MD:" => {
                    if let Some(original_name) = words.next() {
                        num_words += 1;
                        if let Some(original_descriptor) = words.next() {
                            num_words += 1;
                            if let Some(revised_name) = words.next() {
                                num_words += 1;
                                if let Some(revised_descriptor) = words.next() {
                                    num_words += 1;
                                    num_words += words.count();
                                    if num_words == 4 {
                                        let original_signature = MethodSignature::new(original_descriptor);
                                        let revised_signature = MethodSignature::new(revised_descriptor);
                                        original_signature.parse()?;
                                        revised_signature.parse()?;
                                        let original_data = MethodData::parse_internal_name(original_name, original_signature)?;
                                        let revised_data = MethodData::parse_internal_name(revised_name, revised_signature)?;
                                        self.builder.insert_method(
                                            original_data.intern(),
                                            DefaultAtom::from(revised_data.name),
                                        );
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                    // Fallthrough to error
                    debug_assert_eq!(
                        data.split_whitespace().count(),
                        num_words,
                        "Miscounted words: {}",
                        line
                    );
                    return Err(SrgParseError::UnexpectedNumWords {
                        expected: 4,
                        actual: num_words,
                    });
                }
                "FD:" => {
                    if let Some(original_name) = words.next() {
                        num_words += 1;
                        if let Some(revised_name) = words.next() {
                            num_words += 1;
                            num_words += words.count();
                            if num_words == 2 {
                                let original_data = FieldData::parse_internal_name(original_name)?;
                                let revised_data = FieldData::parse_internal_name(revised_name)?;
                                self.builder.insert_field(
                                    original_data.intern(),
                                    DefaultAtom::from(revised_data.name),
                                );
                                return Ok(());
                            }
                        }
                    }
                    // Fallthrough to error
                    debug_assert_eq!(
                        data.split_whitespace().count(),
                        num_words,
                        "Miscounted words: {}",
                        line
                    );
                    return Err(SrgParseError::UnexpectedNumWords {
                        expected: 2,
                        actual: num_words,
                    });
                }
                "CL:" => {
                    if let Some(original_name) = words.next() {
                        num_words += 1;
                        if let Some(revised_name) = words.next() {
                            num_words += 1;
                            num_words += words.count();
                            if num_words == 2 {
                                let original_class = JavaClass::parse_internal_name(original_name)?;
                                let revised_class = JavaClass::parse_internal_name(revised_name)?;
                                self.builder.insert_class(
                                    original_class.intern(),
                                    revised_class.intern(),
                                );
                                return Ok(());
                            }
                        }
                    }
                    // Fallthrough to error
                    debug_assert_eq!(
                        data.split_whitespace().count(),
                        num_words,
                        "Miscounted words: {}",
                        line
                    );
                    return Err(SrgParseError::UnexpectedNumWords {
                        expected: 2,
                        actual: num_words,
                    });
                }
                "PK:" => {
                    if self.ignore_package_mappings {
                        return Ok(());
                    } else {
                        return Err(SrgParseError::UnexpectedMappingType("PK:".to_owned()));
                    }
                }
                _ => {}
            }
        }
        // Ignore any blank lines or ones starting with '#', otherwise fail fast
        if let Some(first_word) = line.split_whitespace().next() {
            if !first_word.starts_with('#') {
                return Err(SrgParseError::UnexpectedMappingType(first_word.to_owned()));
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum SrgParseError {
    InsufficentLength { expected: usize, actual: usize },
    UnexpectedMappingType(String),
    UnexpectedNumWords { expected: usize, actual: usize },
    InvalidMethodDescriptor(MethodDescriptorParseError),
    InvalidName(NameParseError),
    IOError(io::Error),
}
impl From<NameParseError> for SrgParseError {
    #[inline]
    fn from(cause: NameParseError) -> SrgParseError {
        SrgParseError::InvalidName(cause)
    }
}
impl From<MethodDescriptorParseError> for SrgParseError {
    #[inline]
    fn from(cause: MethodDescriptorParseError) -> SrgParseError {
        SrgParseError::InvalidMethodDescriptor(cause)
    }
}
impl From<io::Error> for SrgParseError {
    #[inline]
    fn from(cause: io::Error) -> SrgParseError {
        SrgParseError::IOError(cause)
    }
}
impl Display for SrgParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            SrgParseError::InsufficentLength { expected, .. } => write!(f, "Expected at least {} chars", expected),
            SrgParseError::UnexpectedMappingType(ref mapping_type) => write!(f, "Unexpected mapping type: {}", mapping_type),
            SrgParseError::UnexpectedNumWords { expected, actual } => write!(f, "Expected {} words of data, but got {}", expected, actual),
            SrgParseError::InvalidMethodDescriptor(ref cause) => write!(f, "Invalid method descriptor: {}", cause),
            SrgParseError::InvalidName(ref cause) => write!(f, "Invalid name: {}", cause),
            SrgParseError::IOError(ref cause) => write!(f, "IOError: {}", cause),
        }
    }
}
impl Error for SrgParseError {
    fn description(&self) -> &'static str {
        match *self {
            SrgParseError::InsufficentLength { .. } => "Insufficient length",
            SrgParseError::UnexpectedMappingType(_) => "Unexpected mapping type",
            SrgParseError::UnexpectedNumWords { .. } => "Unexpected number of data words",
            SrgParseError::InvalidMethodDescriptor(_) => "Invalid method descriptor",
            SrgParseError::InvalidName(_) => "Invalid name",
            SrgParseError::IOError(_) => "IOError",
        }
    }
    fn cause(&self) -> Option<&Error> {
        match *self {
            SrgParseError::InvalidMethodDescriptor(ref cause) => Some(cause),
            SrgParseError::InvalidName(ref cause) => Some(cause),
            SrgParseError::IOError(ref cause) => Some(cause),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use types::{MethodSignature, MethodData, FieldData, JavaClass};
    use super::*;
    static TEST_DATA: &str = r#"PK: packages/should be/ignored
CL: java/lang/String com/example/NotString
CL: com/google/guava/base/Preconditions short/Preconditions
CL: Unpackaged com/example/Unpackaged
CL: com/example/Packaged NoLongerPackaged
FD: com/example/Packaged/exists NoLongerPackaged/living
MD: com/google/guava/base/Preconditions/checkArgument (ZLjava/lang/String;I)V short/Preconditions/requireArgument (ZLcom/example/NotString;I)V
"#;
    #[test]
    fn parse_test() {
        let mut parser = SrgMappingsParser::default();
        parser.parse_text(TEST_DATA).expect(
            "Failed to parse test data",
        );
        let mut builder = parser.finish();
        let result = builder.build();
        assert_eq!(
            result.get_class(&JavaClass::new("java/lang/String")),
            JavaClass::new("com/example/NotString"),
            "Mappings: {:#?}",
            result
        );
        assert_eq!(
            result.get_class(&JavaClass::new("com/google/guava/base/Preconditions")),
            JavaClass::new("short/Preconditions")
        );
        assert_eq!(
            result.get_class(&JavaClass::new("Unpackaged")),
            JavaClass::new("com/example/Unpackaged")
        );
        assert_eq!(
            result.get_class(&JavaClass::new("com/example/Packaged")),
            JavaClass::new("NoLongerPackaged")
        );
        assert_eq!(
            result.get_field(&FieldData::parse_internal_name(
                "com/example/Packaged/exists",
            ).unwrap()),
            FieldData::parse_internal_name("NoLongerPackaged/living").unwrap()
        );
        assert_eq!(
            result.get_field(&FieldData::parse_internal_name(
                "com/example/Packaged/implicit",
            ).unwrap()),
            FieldData::parse_internal_name("NoLongerPackaged/implicit").unwrap()
        );
        assert_eq!(
            result.get_method(&MethodData::parse_internal_name(
                "com/google/guava/base/Preconditions/checkArgument",
                MethodSignature::new("(ZLjava/lang/String;I)V"),
            ).unwrap()),
            MethodData::parse_internal_name(
                "short/Preconditions/requireArgument",
                MethodSignature::new("(ZLcom/example/NotString;I)V"),
            ).unwrap()
        );
        // Check to make sure implicit remapping works too!
        let implicit_method = MethodData::parse_internal_name(
            "com/google/guava/base/Preconditions/checkState",
            MethodSignature::new("(ZLjava/lang/String;I)V"),
        ).unwrap();
        assert_eq!(
            result.try_get_method(&implicit_method),
            None,
            "Method was explicitly remaped: {:?}",
            implicit_method
        );
        assert_eq!(
            result.get_method(&implicit_method),
            MethodData::parse_internal_name(
                "short/Preconditions/checkState",
                MethodSignature::new("(ZLcom/example/NotString;I)V"),
            ).unwrap()
        );
    }
}
