use std::io::Read;
use std::cell::RefCell;
use cesu8::{from_java_cesu8, to_java_cesu8, Cesu8DecodingError};



pub struct StringData<'a> {
    reader: ConstantPoolReader<'a>,
    
}

pub enum ConstantPoolType {
    
}

#[derive(Clone)]
pub enum ConstantPoolEntry<'a> {
    StringData(Cow<'a, str>),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class {
        name: Cow<'a, str>
    },
    String(Cow<'a, str>),
    Field {
        class: Cow<'a, str>,
        name: Cow<'a, str>,
        descriptor: Cow<'a, str>
    },
    Method {
        
    },
    InterfaceMethod {
        class: Cow<'a, str>,
        name: Cow<'a, str>,
        descriptor: Cow<'a, str>
    },
    NameAndType {
        name: Cow<'a, str>,
        type_descriptor: Cow<'a, str>
    },
    MethodHandle
}

pub struct ConstantPoolReader<'a> {
    strings: Vec<Option<Cow<'a, str>>>,
    buffer: &'a [u8]
}

enum ConstantPoolParseError {
    InvalidUtf8(Cesu8DecodingError),
    InvalidEntryType(u8),
    UnexpectedEntryType { expected: &'static str, actual: u8 },
}
