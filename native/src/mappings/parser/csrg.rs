use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;

use string_cache::DefaultAtom;

use mappings::MappingsBuilder;
use types::{MethodSignature, MethodData, FieldData, JavaClass, NameParseError, MethodDescriptorParseError, FieldDataLookup, MethodDataLookup, JavaClassLookup};
use super::MappingsParser;

pub struct CompactSrgParser {
    builder: MappingsBuilder,
}
impl Default for CompactSrgParser {
    #[inline]
    fn default() -> Self {
        CompactSrgParser { builder: MappingsBuilder::new() }
    }
}
impl MappingsParser for CompactSrgParser {
    type Error = CompactSrgParseError;
    #[inline]
    fn finish(self) -> MappingsBuilder {
        self.builder
    }
    fn parse_line(&mut self, line: &str) -> Result<(), Self::Error> {
        let mut word_iter = line.split_whitespace();
        if let Some(first_word) = word_iter.next() {
            if first_word.starts_with('#') {
                // Ignore comment lines
                return Ok(());
            }
            let mut words = Vec::with_capacity(4);
            words.push(first_word);
            words.extend(word_iter);
            match words.len() {
                2 => {
                    let original_class = JavaClass::parse_internal_name(words[0])?;
                    let revised_class = JavaClass::parse_internal_name(words[1])?;
                    self.builder.insert_class(
                        original_class.intern(),
                        revised_class.intern(),
                    );
                    Ok(())
                }
                3 => {
                    let original_class = JavaClass::parse_internal_name(words[0])?;
                    let original_name = words[1];
                    let revised_name = words[2];
                    let original_field = FieldData {
                        class: original_class,
                        name: original_name,
                    };
                    self.builder.insert_field(
                        original_field.intern(),
                        DefaultAtom::from(revised_name),
                    );
                    Ok(())
                }
                4 => {
                    let original_class = JavaClass::parse_internal_name(words[0])?;
                    let original_name = words[1];
                    let original_signature = MethodSignature::new(words[2]);
                    original_signature.parse()?;
                    let revised_name = words[3];
                    let original_method = MethodData {
                        class: original_class,
                        name: original_name,
                        signature: original_signature,
                    };
                    self.builder.insert_method(
                        original_method.intern(),
                        DefaultAtom::from(revised_name),
                    );
                    Ok(())
                }
                _ => Err(CompactSrgParseError::UnexpectedNumWords(words.len())),
            }
        } else {
            // Ignore blank lines
            Ok(())
        }
    }
}
#[derive(Debug)]
pub enum CompactSrgParseError {
    IOError(io::Error),
    UnexpectedNumWords(usize),
    InvalidName(NameParseError),
    InvalidDescriptor(MethodDescriptorParseError),
}
impl Display for CompactSrgParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            CompactSrgParseError::IOError(ref cause) => write!(f, "IOError: {}", cause),
            CompactSrgParseError::UnexpectedNumWords(amount) => write!(f, "Unexpected number of data words: {}", amount),
            CompactSrgParseError::InvalidName(ref cause) => write!(f, "Invalid name: {}", cause),
            CompactSrgParseError::InvalidDescriptor(ref cause) => write!(f, "Invalid descriptor: {}", cause),
        }
    }
}
impl Error for CompactSrgParseError {
    fn description(&self) -> &'static str {
        match *self {
            CompactSrgParseError::IOError(_) => "IOError",
            CompactSrgParseError::UnexpectedNumWords(_) => "Unexpected number of data words",
            CompactSrgParseError::InvalidName(_) => "Invalid name",
            CompactSrgParseError::InvalidDescriptor(_) => "Invalid method descriptor",
        }
    }
    fn cause(&self) -> Option<&Error> {
        match *self {
            CompactSrgParseError::IOError(ref cause) => Some(cause),
            CompactSrgParseError::InvalidName(ref cause) => Some(cause),
            CompactSrgParseError::InvalidDescriptor(ref cause) => Some(cause),
            _ => None,
        }
    }
}
impl From<io::Error> for CompactSrgParseError {
    #[inline]
    fn from(cause: io::Error) -> Self {
        CompactSrgParseError::IOError(cause)
    }
}
impl From<NameParseError> for CompactSrgParseError {
    #[inline]
    fn from(cause: NameParseError) -> Self {
        CompactSrgParseError::InvalidName(cause)
    }
}
impl From<MethodDescriptorParseError> for CompactSrgParseError {
    #[inline]
    fn from(cause: MethodDescriptorParseError) -> Self {
        CompactSrgParseError::InvalidDescriptor(cause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{MethodData, JavaClass, FieldData, MethodSignature};
    static TEST_DATA: &str = r#"java/lang/String com/example/NotString
com/google/guava/base/Preconditions short/Preconditions
Unpackaged com/example/Unpackaged
com/example/Packaged NoLongerPackaged
com/example/Packaged exists living
com/google/guava/base/Preconditions checkArgument (ZLjava/lang/String;I)V requireArgument"#;
    #[test]
    fn parse_test() {
        let mut parser = CompactSrgParser::default();
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
