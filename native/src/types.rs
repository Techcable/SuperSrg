use string_cache::DefaultAtom;
use std::fmt::{self, Display, Formatter};
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::borrow::Cow;

use ordermap::Equivalent;

use mappings::MappingsTransformer;

/*
 * NOTE: ToOwned and Deref can't be used since they require us to borrow the resulting struct.
 * This is a unresolved issue in rust iself: https://github.com/rust-lang/rust/issues/39125
 */
pub trait JavaClassLookup: Clone + Equivalent<PooledJavaClass> + Hash {
    #[inline]
    fn borrowed(&self) -> JavaClass {
        JavaClass::new(self.internal_name())
    }
    #[inline]
    fn intern(&self) -> PooledJavaClass {
        PooledJavaClass(self.pooled_name().into_owned())
    }
    fn internal_name(&self) -> &str;
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Owned(DefaultAtom::from(self.internal_name()))
    }
}
#[derive(Hash, Eq, Copy, Clone, Debug)]
pub struct JavaClass<'a>(&'a str);
impl<'a> JavaClass<'a> {
    #[inline]
    pub fn parse_internal_name(name: &'a str) -> Result<Self, NameParseError> {
        if name.is_empty() {
            return Err(NameParseError::EmptyClassName);
        }
        if cfg!(debug_assertions) {
            // When debugging, check for dots, as those would indicate a non-internal name
            if let Some(dot_index) = name.find('.') {
                return Err(NameParseError::UnexpectedDot(dot_index));
            }
        }
        Ok(JavaClass(name))
    }
    #[inline]
    pub fn new(name: &'a str) -> JavaClass<'a> {
        JavaClass(name)
    }
    #[inline]
    fn not_internal_name(name: &str) -> ! {
        panic!("Not an internal name: {}", name);
    }
    #[inline]
    fn to_owned(&self) -> JavaClassBuf {
        JavaClassBuf(self.0.to_owned())
    }
}
impl<'a, T: JavaClassLookup> PartialEq<T> for JavaClass<'a> {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.internal_name() == other.internal_name()
    }
}
impl<'a> JavaClassLookup for JavaClass<'a> {
    #[inline]
    fn borrowed(&self) -> JavaClass {
        *self
    }
    #[inline]
    fn internal_name(&self) -> &str {
        self.0
    }
}
#[derive(Hash, PartialEq, Eq, Clone)]
pub struct JavaClassBuf(pub String);
#[derive(Eq, Clone)]
pub struct PooledJavaClass(DefaultAtom);
// NOTE: Must manually implement to avoid unessicarrily debug output of DefaultAtom
impl fmt::Debug for PooledJavaClass {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("PooledJavaClass")
            .field(&self.internal_name())
            .finish()
    }
}
// NOTE: Must manually implement to ensure we always have the same hash as JavaClass
impl Hash for PooledJavaClass {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.borrowed().hash(hasher)
    }
}
impl JavaClassLookup for PooledJavaClass {
    #[inline]
    fn intern(&self) -> PooledJavaClass {
        self.clone()
    }
    #[inline]
    fn internal_name(&self) -> &str {
        self.0.as_ref()
    }
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Borrowed(&self.0)
    }
}
impl<'a> Equivalent<PooledJavaClass> for JavaClass<'a> {
    #[inline]
    fn equivalent(&self, other: &PooledJavaClass) -> bool {
        other.0 == *self.0
    }
}
#[derive(Hash, PartialEq, Eq, Copy, Clone)]
pub enum PrimitiveType {
    Byte,
    Short,
    Int,
    Long,
    Double,
    Float,
    Char,
    Boolean,
    Void,
}
impl PrimitiveType {
    fn descriptor(&self) -> char {
        match *self {
            PrimitiveType::Byte => 'B',
            PrimitiveType::Short => 'S',
            PrimitiveType::Int => 'I',
            PrimitiveType::Long => 'J',
            PrimitiveType::Double => 'D',
            PrimitiveType::Float => 'F',
            PrimitiveType::Char => 'C',
            PrimitiveType::Boolean => 'Z',
            PrimitiveType::Void => 'V',
        }
    }
    fn from_descriptor(c: char) -> Option<PrimitiveType> {
        match c {
            'B' => Some(PrimitiveType::Byte),
            'S' => Some(PrimitiveType::Short),
            'I' => Some(PrimitiveType::Int),
            'J' => Some(PrimitiveType::Long),
            'D' => Some(PrimitiveType::Double),
            'F' => Some(PrimitiveType::Float),
            'C' => Some(PrimitiveType::Char),
            'Z' => Some(PrimitiveType::Boolean),
            'V' => Some(PrimitiveType::Void),
            _ => None,
        }
    }
}
#[derive(Hash, PartialEq, Eq, Clone)]
pub enum JavaType<C: JavaClassLookup> {
    Primitive(PrimitiveType),
    Array {
        dimensions: u32,
        element_type: Box<JavaType<C>>,
    },
    Class(C),
}
impl<C: JavaClassLookup> JavaType<C> {
    #[inline]
    pub fn descriptor(&self) -> String {
        let mut result = String::new();
        self.write_descriptor(&mut result);
        result
    }
    pub fn write_descriptor(&self, buf: &mut String) {
        match *self {
            JavaType::Class(ref class) => {
                buf.push('L');
                buf.push_str(class.internal_name());
                buf.push(';');
            }
            JavaType::Array {
                dimensions,
                ref element_type,
            } => {
                debug_assert!(dimensions > 0, "Invalid dimensions: {}", dimensions);
                for _ in 0..dimensions {
                    buf.push('[');
                }
                if cfg!(debug_assert) {
                    if let JavaType::Array { .. } = **element_type {
                        panic!("Nested array: {}", self);
                    }
                }
                element_type.write_descriptor(buf);
            }
            // Primitives
            JavaType::Primitive(primitive) => buf.push(primitive.descriptor()),
        }
    }
    #[inline]
    pub fn remap_class<N: JavaClassLookup, F>(&self, transformer: F) -> JavaType<N>
    where
        F: Fn(&C) -> N,
    {
        match *self {
            JavaType::Class(ref class) => JavaType::Class(transformer(class)),
            JavaType::Array {
                ref element_type,
                dimensions,
            } => {
                // NOTE: Nested arrays are forbidden, so no need to recurse
                let new_element_type = match **element_type {
                    JavaType::Class(ref class) => JavaType::Class(transformer(class)),
                    JavaType::Array { .. } => panic!("Nested array: {}", self),
                    JavaType::Primitive(primitive) => JavaType::Primitive(primitive),
                };
                JavaType::Array {
                    dimensions,
                    element_type: Box::new(new_element_type),
                }
            }
            JavaType::Primitive(primitive) => JavaType::Primitive(primitive),
        }
    }
}
impl<'a> JavaType<JavaClass<'a>> {
    pub fn parse_descriptor(descriptor: &'a str) -> Result<JavaType<JavaClass<'a>>, TypeDescriptorParseError> {
        let (size, result) = Self::partially_parse_descriptor(descriptor)?;
        debug_assert!(size <= descriptor.len());
        if descriptor.len() > size {
            return Err(TypeDescriptorParseError::UnexpectedlyLong {
                expected: size,
                actual: descriptor.len(),
            });
        }
        Ok(result)
    }
    pub fn partially_parse_descriptor(descriptor: &'a str) -> Result<(usize, JavaType<JavaClass<'a>>), TypeDescriptorParseError> {
        if let Some(start) = descriptor.chars().next() {
            match start {
                'L' => {
                    if let Some(end) = descriptor.find(';') {
                        let class = JavaClass::new(&descriptor[1..end]);
                        Ok((end + 1, JavaType::Class(class)))
                    } else {
                        Err(TypeDescriptorParseError::UnclosedClassDescriptor)
                    }
                }
                '[' => {
                    if let Some(dimensions) = descriptor.find(|c| c != '[') {
                        match Self::partially_parse_descriptor(&descriptor[dimensions..]) {
                            Ok((size, element_type)) => Ok((
                                size + dimensions,
                                JavaType::Array {
                                    element_type: Box::new(element_type),
                                    dimensions: dimensions as u32,
                                },
                            )),
                            Err(cause) => Err(TypeDescriptorParseError::InvalidElementDescriptor {
                                dimensions,
                                cause: Box::new(cause),
                            }),
                        }
                    } else {
                        Err(TypeDescriptorParseError::EmptyArray {
                            dimensions: descriptor.len(),
                        })
                    }
                }
                _ => {
                    if let Some(primitive) = PrimitiveType::from_descriptor(start) {
                        Ok((1, JavaType::Primitive(primitive)))
                    } else {
                        Err(TypeDescriptorParseError::InvalidStart(start))
                    }
                }
            }
        } else {
            Err(TypeDescriptorParseError::EmptyDescriptor)
        }
    }
}
#[derive(Debug)]
pub enum TypeDescriptorParseError {
    EmptyDescriptor,
    UnexpectedlyLong { expected: usize, actual: usize },
    UnclosedClassDescriptor,
    InvalidStart(char),
    EmptyArray { dimensions: usize },
    InvalidElementDescriptor {
        dimensions: usize,
        cause: Box<TypeDescriptorParseError>,
    },
}
impl Display for TypeDescriptorParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            TypeDescriptorParseError::UnexpectedlyLong { expected, .. } => {
                write!(
                    f,
                    "Expected the descriptor to be only {} bytes long",
                    expected
                )
            }
            TypeDescriptorParseError::InvalidStart(start) => write!(f, "Invalid descriptor start: {}", start),
            TypeDescriptorParseError::InvalidElementDescriptor {
                ref cause,
                dimensions,
            } => {
                write!(
                    f,
                    "Invalid element descriptor for {} dimension array: {}",
                    dimensions,
                    *cause
                )
            }
            TypeDescriptorParseError::EmptyArray { dimensions } => {
                write!(
                    f,
                    "Empty element descriptor for {} dimension array",
                    dimensions
                )
            }
            _ => self.description().fmt(f),
        }
    }
}
impl Error for TypeDescriptorParseError {
    fn description(&self) -> &'static str {
        match *self {
            TypeDescriptorParseError::EmptyDescriptor => "Empty type descriptor",
            TypeDescriptorParseError::UnexpectedlyLong { .. } => "Unexpectedly long type descriptor",
            TypeDescriptorParseError::UnclosedClassDescriptor => "Unclosed type descriptor",
            TypeDescriptorParseError::InvalidStart(_) => "Invalid type descriptor start",
            TypeDescriptorParseError::EmptyArray { .. } => "Empty array",
            TypeDescriptorParseError::InvalidElementDescriptor { .. } => "Invalid element descriptor",
        }
    }
}
fn parse_internal_name(name: &str) -> Result<(JavaClass, &str), NameParseError> {
    if let Some(seperator) = name.rfind('/') {
        let class = JavaClass::parse_internal_name(&name[..seperator])?;
        let member_name = &name[seperator + 1..];
        if member_name.is_empty() {
            Err(NameParseError::EmptyMemberName)
        } else {
            Ok((class, member_name))
        }
    } else if name.is_empty() {
        Err(NameParseError::EmptyName)
    } else {
        Err(NameParseError::MissingSeperator)
    }
}
impl<C: JavaClassLookup> Display for JavaType<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.descriptor().fmt(f)
    }
}
pub trait FieldDataLookup: Clone + Equivalent<PooledFieldData> + Hash {
    type Class: JavaClassLookup;
    #[inline]
    fn intern(&self) -> PooledFieldData {
        PooledFieldData {
            class: self.class().intern(),
            name: DefaultAtom::from(self.name()),
        }
    }
    #[inline]
    fn borrowed(&self) -> FieldData {
        FieldData {
            class: self.class().borrowed(),
            name: self.name(),
        }
    }
    fn class(&self) -> &Self::Class;
    fn name(&self) -> &str;
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Owned(DefaultAtom::from(self.name()))
    }
}
#[derive(Hash, Eq, Clone, Copy, Debug)]
pub struct FieldData<'a> {
    pub class: JavaClass<'a>,
    pub name: &'a str,
}
impl<'a> FieldData<'a> {
    #[inline]
    pub fn parse_internal_name(name: &'a str) -> Result<Self, NameParseError> {
        let (class, name) = parse_internal_name(name)?;
        Ok(FieldData { class, name })
    }
}
impl<'a, T: FieldDataLookup> PartialEq<T> for FieldData<'a> {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.class == *other.class() && self.name() == other.name()
    }
}
impl<'a> FieldDataLookup for FieldData<'a> {
    type Class = JavaClass<'a>;
    #[inline]
    fn borrowed(&self) -> FieldData {
        *self
    }
    #[inline]
    fn class(&self) -> &JavaClass<'a> {
        &self.class
    }
    #[inline]
    fn name(&self) -> &str {
        self.name
    }
}
#[derive(Clone, Eq)]
pub struct PooledFieldData {
    pub class: PooledJavaClass,
    pub name: DefaultAtom,
}
// NOTE: Must manually implement to avoid unessicarrily debug output of DefaultAtom
impl fmt::Debug for PooledFieldData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("PooledFieldData")
            .field("class", &self.class)
            .field("name", &self.name())
            .finish()
    }
}
// NOTE: Must manually implement to ensure we always have the same hash as FieldData
impl Hash for PooledFieldData {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.borrowed().hash(hasher)
    }
}
impl<T: FieldDataLookup> PartialEq<T> for PooledFieldData {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.class == *other.class() && self.name() == other.name()
    }
}
impl FieldDataLookup for PooledFieldData {
    type Class = PooledJavaClass;
    #[inline]
    fn intern(&self) -> PooledFieldData {
        self.clone()
    }
    #[inline]
    fn class(&self) -> &PooledJavaClass {
        &self.class
    }
    #[inline]
    fn name(&self) -> &str {
        &self.name
    }
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Borrowed(&self.name)
    }
}
impl<'a> From<FieldData<'a>> for PooledFieldData {
    #[inline]
    fn from(borrowed: FieldData<'a>) -> PooledFieldData {
        borrowed.intern()
    }
}
impl<T: JavaClassLookup> PartialEq<T> for PooledJavaClass {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.internal_name() == other.internal_name()
    }
}
impl<'a> Equivalent<PooledFieldData> for FieldData<'a> {
    #[inline]
    fn equivalent(&self, other: &PooledFieldData) -> bool {
        other.class == self.class && *self.name == other.name
    }
}
#[derive(Clone)]
pub struct FieldDataBuf {
    pub class: JavaClassBuf,
    pub name: String,
}
pub trait MethodDataLookup: Clone + Equivalent<PooledMethodData> + Hash {
    type Class: JavaClassLookup;
    #[inline]
    fn intern(&self) -> PooledMethodData {
        PooledMethodData {
            class: self.class().intern(),
            name: DefaultAtom::from(self.name()),
            signature: DefaultAtom::from(self.signature()),
        }
    }
    #[inline]
    fn borrowed(&self) -> MethodData {
        MethodData {
            class: self.class().borrowed(),
            name: self.name(),
            signature: MethodSignature { descriptor: self.signature() },
        }
    }
    fn class(&self) -> &Self::Class;
    fn name(&self) -> &str;
    fn signature(&self) -> &str;
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Owned(DefaultAtom::from(self.name()))
    }
    #[inline]
    fn pooled_signature(&self) -> Cow<DefaultAtom> {
        Cow::Owned(DefaultAtom::from(self.signature()))
    }
}
#[derive(Hash, Eq, Clone, Copy, Debug)]
pub struct MethodData<'a> {
    pub class: JavaClass<'a>,
    pub name: &'a str,
    pub signature: MethodSignature<'a>,
}
impl<'a> MethodDataLookup for MethodData<'a> {
    type Class = JavaClass<'a>;
    #[inline]
    fn borrowed(&self) -> MethodData {
        *self
    }
    #[inline]
    fn class(&self) -> &JavaClass<'a> {
        &self.class
    }
    #[inline]
    fn name(&self) -> &str {
        self.name
    }
    #[inline]
    fn signature(&self) -> &str {
        self.signature.descriptor
    }
}
impl<'a, T: MethodDataLookup> PartialEq<T> for MethodData<'a> {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.class == *other.class() && self.name() == other.name() && self.signature.descriptor == other.signature()
    }
}
impl<'a> MethodData<'a> {
    #[inline]
    pub fn parse_internal_name(name: &'a str, signature: MethodSignature<'a>) -> Result<Self, NameParseError> {
        let (class, name) = parse_internal_name(name)?;
        Ok(MethodData {
            class,
            name,
            signature,
        })
    }
}
#[derive(Eq, Clone)]
pub struct PooledMethodData {
    pub class: PooledJavaClass,
    pub name: DefaultAtom,
    pub signature: DefaultAtom,
}
// NOTE: Must manually implement to avoid unessicarrily debug output of DefaultAtom
impl fmt::Debug for PooledMethodData {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("PooledMethodData")
            .field("class", &self.class)
            .field("name", &self.name())
            .field("signature", &self.signature())
            .finish()
    }
}
// NOTE: Must manually implement to ensure we always have the same hash as MethodData
impl Hash for PooledMethodData {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.borrowed().hash(hasher)
    }
}
impl<T: MethodDataLookup> PartialEq<T> for PooledMethodData {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        self.class == *other.class() && self.name() == other.name() && self.signature.as_ref() == other.signature()
    }
}
impl MethodDataLookup for PooledMethodData {
    type Class = PooledJavaClass;
    #[inline]
    fn intern(&self) -> PooledMethodData {
        self.clone()
    }
    #[inline]
    fn class(&self) -> &PooledJavaClass {
        &self.class
    }
    #[inline]
    fn name(&self) -> &str {
        &self.name
    }
    #[inline]
    fn signature(&self) -> &str {
        &self.signature
    }
    #[inline]
    fn pooled_name(&self) -> Cow<DefaultAtom> {
        Cow::Borrowed(&self.name)
    }
    #[inline]
    fn pooled_signature(&self) -> Cow<DefaultAtom> {
        Cow::Borrowed(&self.signature)
    }
}
impl<'a> From<MethodData<'a>> for PooledMethodData {
    #[inline]
    fn from(borrowed: MethodData<'a>) -> PooledMethodData {
        borrowed.intern()
    }
}
impl<'a> Equivalent<PooledMethodData> for MethodData<'a> {
    #[inline]
    fn equivalent(&self, other: &PooledMethodData) -> bool {
        other.class == self.class && *self.name == other.name && *self.signature.descriptor == other.signature
    }
}
#[derive(Hash, PartialEq, Eq, Clone)]
pub struct MethodDataBuf {
    pub class: JavaClassBuf,
    pub name: String,
    pub signature: String,
}
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct MethodSignature<'a> {
    descriptor: &'a str,
}
pub struct ParsedMethodSignature<C: JavaClassLookup> {
    pub parameter_types: Vec<JavaType<C>>,
    pub return_type: JavaType<C>,
}
impl<'a> MethodSignature<'a> {
    #[inline]
    pub fn descriptor(&self) -> &'a str {
        self.descriptor
    }
    #[inline]
    pub fn new(descriptor: &'a str) -> Self {
        MethodSignature { descriptor }
    }
    pub fn parse(&self) -> Result<ParsedMethodSignature<JavaClass<'a>>, MethodDescriptorParseError> {
        let descriptor = self.descriptor;
        if let Some(first) = descriptor.chars().next() {
            if first != '(' {
                return Err(MethodDescriptorParseError::UnopenedDescriptor);
            }
            if let Some(end) = self.descriptor.find(')') {
                let mut parameter_types = Vec::with_capacity(32); // Methods larger than this are rare
                let mut index = 1;
                while index < end {
                    match JavaType::partially_parse_descriptor(&descriptor[index..end]) {
                        Ok((size, result)) => {
                            index += size;
                            parameter_types.push(result);
                        }
                        Err(cause) => {
                            let current_parameter = parameter_types.len();
                            return Err(MethodDescriptorParseError::InvalidParameterType {
                                start_index: index,
                                parameter: current_parameter,
                                cause,
                            });
                        }
                    }
                }
                assert_eq!(index, end, "Index overran end: {}", descriptor);
                index += 1;
                let return_type = match JavaType::parse_descriptor(&descriptor[index..]) {
                    Ok(result) => result,
                    Err(cause) => {
                        return Err(MethodDescriptorParseError::InvalidReturnType {
                            cause,
                            start_index: index,
                        })
                    }
                };
                Ok(ParsedMethodSignature {
                    parameter_types: parameter_types,
                    return_type: return_type,
                })
            } else {
                Err(MethodDescriptorParseError::UnclosedDescriptor)
            }
        } else {
            Err(MethodDescriptorParseError::EmptyDescriptor)
        }
    }
}
impl<C: JavaClassLookup> ParsedMethodSignature<C> {
    pub fn descriptor(&self) -> String {
        let mut result = String::new();
        self.write_descriptor(&mut result);
        result
    }
    pub fn write_descriptor(&self, buf: &mut String) {
        buf.push('(');
        for parameter_type in &self.parameter_types {
            parameter_type.write_descriptor(buf);
        }
        buf.push(')');
        self.return_type.write_descriptor(buf);
    }
    #[inline]
    pub fn remap_class<F, N>(&self, transformer: F) -> ParsedMethodSignature<N>
    where
        N: JavaClassLookup,
        F: Fn(&C) -> N,
    {
        self.remap(|original_type| original_type.remap_class(&transformer))
    }
    #[inline]
    pub fn remap<F, N>(&self, transformer: F) -> ParsedMethodSignature<N>
    where
        N: JavaClassLookup,
        F: Fn(&JavaType<C>) -> JavaType<N>,
    {
        let mut new_parameter_types = Vec::with_capacity(self.parameter_types.len());
        for parameter_type in &self.parameter_types {
            new_parameter_types.push(transformer(parameter_type));
        }
        let new_return_type = transformer(&self.return_type);
        ParsedMethodSignature {
            parameter_types: new_parameter_types,
            return_type: new_return_type,
        }
    }
    #[inline]
    pub fn transform<T: MappingsTransformer>(self, transformer: &T) -> ParsedMethodSignature<PooledJavaClass> {
        self.remap_class(|original_class| match transformer.transform_class(
            original_class,
        ) {
            Some(c) => c.into_owned(),
            None => original_class.intern(),
        })
    }
}
#[derive(Debug)]
pub enum NameParseError {
    EmptyName,
    EmptyMemberName,
    EmptyClassName,
    MissingSeperator,
    UnexpectedDot(usize),
}
impl Display for NameParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            NameParseError::UnexpectedDot(index) => write!(f, "Unexpected dot at {}", index),
            _ => self.description().fmt(f),
        }
    }
}
impl Error for NameParseError {
    fn description(&self) -> &'static str {
        match *self {
            NameParseError::EmptyName => "Empty name",
            NameParseError::EmptyMemberName => "Empty member name",
            NameParseError::EmptyClassName => "Empty class name",
            NameParseError::MissingSeperator => "Missing seperator",
            NameParseError::UnexpectedDot(_) => "Unexpected dot",
        }
    }
}
#[derive(Debug)]
pub enum MethodDescriptorParseError {
    EmptyDescriptor,
    UnopenedDescriptor,
    UnclosedDescriptor,
    InvalidReturnType {
        start_index: usize,
        cause: TypeDescriptorParseError,
    },
    InvalidParameterType {
        parameter: usize,
        start_index: usize,
        cause: TypeDescriptorParseError,
    },
}
impl Display for MethodDescriptorParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            MethodDescriptorParseError::InvalidReturnType { ref cause, .. } => write!(f, "Invalid return type: {}", cause),
            MethodDescriptorParseError::InvalidParameterType {
                parameter,
                ref cause,
                ..
            } => write!(f, "Invalid {} parameter type: {}", parameter, cause),
            _ => self.description().fmt(f),
        }
    }
}
impl Error for MethodDescriptorParseError {
    fn description(&self) -> &'static str {
        match *self {
            MethodDescriptorParseError::EmptyDescriptor => "Empty method descriptor",
            MethodDescriptorParseError::UnopenedDescriptor => "Unopened method descriptor",
            MethodDescriptorParseError::UnclosedDescriptor => "Unclosed method descriptor",
            MethodDescriptorParseError::InvalidReturnType { .. } => "Invalid return type",
            MethodDescriptorParseError::InvalidParameterType { .. } => "Invalid parameter type",
        }
    }
    fn cause(&self) -> Option<&Error> {
        match *self {
            MethodDescriptorParseError::InvalidReturnType { ref cause, .. } |
            MethodDescriptorParseError::InvalidParameterType { ref cause, .. } => Some(cause),
            _ => None,
        }
    }
}
impl<'a> Display for MethodData<'a> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}/{}{}",
            self.class.internal_name(),
            self.name(),
            self.signature.descriptor
        )
    }
}
impl<'a> Display for FieldData<'a> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.class.internal_name(), self.name())
    }
}
