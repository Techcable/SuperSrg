use std::str::{self, Utf8Error};
use std::fmt::{self, Formatter};
use std::hash::{Hash, BuildHasher, BuildHasherDefault};
use std::io::{self, Write, BufRead, Cursor};
use std::collections::hash_map::RandomState;
use std::path::Path;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::convert::TryFrom;

use byteorder::{ByteOrder, BigEndian};
use serde::ser::SerializeMap;
use serde::de::{self, MapAccess};
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use string_cache::DefaultAtom;
use ordermap::{self, OrderMap};
use curl::easy::Easy;
use git2::{Repository, Commit, Error as GitError};
use seahash::SeaHasher;

#[inline]
pub fn full_extension(path: &Path) -> Option<&str> {
    let text = path.to_str().unwrap();
    if let Some(dot) = text.find('.') {
        text.get(dot + 1..)
    } else {
        None
    }
}
pub fn load_from_commit(repo: &Repository, commit: &Commit, relative_path: &Path, buffer: &mut String) -> Result<(), CommitLoadError> {
    let tree = commit.tree()?;
    let object = tree.get_path(relative_path)?.to_object(repo)?;
    // TODO: Don't panic
    let blob = object.into_blob().unwrap_or_else(|e| {
        panic!(
            "Expected {} to be a blob, not a {:?}",
            relative_path.display(),
            e.kind()
        )
    });
    buffer.push_str(str::from_utf8(blob.content())?);
    Ok(())
}
pub enum CommitLoadError {
    Git(GitError),
    InvalidUtf8(Utf8Error),
}
impl From<GitError> for CommitLoadError {
    #[inline]
    fn from(cause: GitError) -> CommitLoadError {
        CommitLoadError::Git(cause)
    }
}
impl From<Utf8Error> for CommitLoadError {
    #[inline]
    fn from(cause: Utf8Error) -> CommitLoadError {
        CommitLoadError::InvalidUtf8(cause)
    }
}
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct SerializableOrderMap<K: Hash + Eq, V, S: BuildHasher = RandomState>(pub OrderMap<K, V, S>);
pub type SeaHashSerializableOrderMap<K, V> = SerializableOrderMap<K, V, SeaHashBuildHasher>;
impl<K: Hash + Eq, V, S: BuildHasher> Deref for SerializableOrderMap<K, V, S> {
    type Target = OrderMap<K, V, S>;
    #[inline(always)]
    fn deref(&self) -> &OrderMap<K, V, S> {
        &self.0
    }
}
impl<K: Hash + Eq, V, S: BuildHasher> DerefMut for SerializableOrderMap<K, V, S> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut OrderMap<K, V, S> {
        &mut self.0
    }
}
impl<K: Hash + Eq + Serialize, V: Serialize, H: BuildHasher> Serialize for SerializableOrderMap<K, V, H> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map_serializer = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self.iter() {
            map_serializer.serialize_entry(key, value)?;
        }
        map_serializer.end()
    }
}
struct OrderMapDeserializeVisitor<K: Hash + Eq, V, S: BuildHasher> {
    marker: PhantomData<SerializableOrderMap<K, V, S>>,
}
impl<'de, K: Hash + Eq, V, S: BuildHasher> de::Visitor<'de> for OrderMapDeserializeVisitor<K, V, S>
where
    K: Deserialize<'de>,
    V: Deserialize<'de>,
    S: Default,
{
    type Value = SerializableOrderMap<K, V, S>;
    #[inline]
    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("a map")
    }
    #[inline]
    fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
        let mut map = SerializableOrderMap(OrderMap::with_capacity_and_hasher(
            access.size_hint().unwrap_or(0),
            Default::default(),
        ));
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }
        Ok(map)
    }
}
impl<'de, K: Hash + Eq, V, S: BuildHasher> Deserialize<'de> for SerializableOrderMap<K, V, S>
where
    K: Deserialize<'de>,
    V: Deserialize<'de>,
    S: Default,
{
    #[inline]
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(OrderMapDeserializeVisitor { marker: PhantomData })
    }
}
#[inline]
pub fn download_buffer(url: &str) -> Result<Vec<u8>, DownloadError> {
    let mut buffer = Vec::with_capacity(2048);
    {
        let mut cursor = Cursor::new(buffer);
        download(url, &mut cursor)?;
        buffer = cursor.into_inner();
    }
    Ok(buffer)
}
#[inline]
pub fn download_text(url: &str) -> Result<String, DownloadError> {
    let buffer = download_buffer(url)?;
    String::from_utf8(buffer).map_err(|e| DownloadError::InvalidUtf8(e.utf8_error()))
}

pub fn download<W: Write>(url: &str, output: &mut W) -> Result<(), DownloadError> {
    let mut easy = Easy::new();
    easy.url(url)?;
    let mut error: Option<io::Error> = None;
    let result = {
        let mut transfer = easy.transfer();
        transfer.write_function(
            |data| if let Err(e) = output.write_all(data) {
                error = Some(e);
                Ok(0)
            } else {
                Ok(data.len())
            },
        )?;
        transfer.perform()
    };
    match result {
        Err(e) => {
            if let Some(actual_error) = error.take() {
                Err(DownloadError::IOError(actual_error))
            } else {
                Err(DownloadError::Curl(e))
            }
        }
        Ok(_) => {
            assert!(error.is_none());
            Ok(())
        }
    }
}
pub enum DownloadError {
    Curl(::curl::Error),
    IOError(io::Error),
    InvalidUtf8(Utf8Error),
}
impl From<::curl::Error> for DownloadError {
    #[inline]
    fn from(cause: ::curl::Error) -> DownloadError {
        DownloadError::Curl(cause)
    }
}
impl From<io::Error> for DownloadError {
    #[inline]
    fn from(cause: io::Error) -> DownloadError {
        DownloadError::IOError(cause)
    }
}

pub type SeaHashBuildHasher = BuildHasherDefault<SeaHasher>;
pub type SeaHashOrderMap<K, V> = OrderMap<K, V, SeaHashBuildHasher>;
pub type SeaHashOrderSet<T> = SeaHashOrderMap<T, ()>;
#[derive(Debug)]
pub struct SimpleDecoder<R: BufRead> {
    pub reader: R,
    buffer: Vec<u8>,
}
/// A wrapper for `DefaultAtom` that implements `ordermap::Equivelant`
#[derive(Default, Clone, PartialEq, Hash, Eq, Serialize, Deserialize)]
pub struct PooledString(pub DefaultAtom);
impl ordermap::Equivalent<PooledString> for str {
    #[inline]
    fn equivalent(&self, other: &PooledString) -> bool {
        *other.0 == *self
    }
}
impl ordermap::Equivalent<PooledString> for DefaultAtom {
    #[inline]
    fn equivalent(&self, other: &PooledString) -> bool {
        *other.0 == *self
    }
}
impl<'a> From<&'a str> for PooledString {
    #[inline]
    fn from(value: &str) -> PooledString {
        PooledString(DefaultAtom::from(value))
    }
}
impl AsRef<str> for PooledString {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<R: BufRead> SimpleDecoder<R> {
    #[inline]
    pub fn new(reader: R) -> Self {
        SimpleDecoder {
            reader,
            buffer: Vec::new(),
        }
    }
    #[inline]
    pub fn read_bytes(&mut self, amount: usize) -> Result<&[u8], io::Error> {
        while self.buffer.len() < amount {
            self.buffer.push(0);
        }
        let data = &mut self.buffer[..amount];
        self.reader.read_exact(data)?;
        Ok(data)
    }
    #[inline]
    pub fn read_u64(&mut self) -> Result<u64, io::Error> {
        let mut data = [0; 8];
        self.reader.read_exact(&mut data)?;
        Ok(BigEndian::read_u64(&data))
    }
    #[inline]
    pub fn read_u32(&mut self) -> Result<u32, io::Error> {
        let mut data = [0; 4];
        self.reader.read_exact(&mut data)?;
        Ok(BigEndian::read_u32(&data))
    }
    #[inline]
    pub fn read_u16(&mut self) -> Result<u16, io::Error> {
        let mut data = [0; 2];
        self.reader.read_exact(&mut data)?;
        Ok(BigEndian::read_u16(&data))
    }
    #[inline]
    pub fn read_string(&mut self) -> Result<&str, io::Error> {
        let length = self.read_u16()? as usize;
        self.read_raw_string(length)
    }
    #[inline]
    pub fn read_raw_string(&mut self, byte_size: usize) -> Result<&str, io::Error> {
        let data = self.read_bytes(byte_size)?;
        match str::from_utf8(data) {
            Ok(result) => Ok(result),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)), 
        }
    }
    /// Read a null termianted string of bytes, including the null terminator itself
    #[inline]
    pub fn read_nullterm(&mut self) -> Result<&[u8], io::Error> {
        self.buffer.clear();
        self.reader.read_until(b'\0', &mut self.buffer)?;
        Ok(&self.buffer)
    }
    #[inline]
    pub fn into_inner(self) -> R {
        self.reader
    }
}
pub struct SimpleEncoder<W: Write>(pub W);
impl<W: Write> SimpleEncoder<W> {
    #[inline]
    pub fn write_u16(&mut self, value: u16) -> Result<(), io::Error> {
        let mut buffer = [0; 2];
        BigEndian::write_u16(&mut buffer, value);
        self.0.write_all(&buffer)
    }
    #[inline]
    pub fn write_u32(&mut self, value: u32) -> Result<(), io::Error> {
        let mut buffer = [0; 4];
        BigEndian::write_u32(&mut buffer, value);
        self.0.write_all(&buffer)
    }
    #[inline]
    pub fn write_u64(&mut self, value: u64) -> Result<(), io::Error> {
        let mut buffer = [0; 8];
        BigEndian::write_u64(&mut buffer, value);
        self.0.write_all(&buffer)
    }
    #[inline]
    pub fn write_string(&mut self, value: &str) -> Result<(), io::Error> {
        let length = u16::try_from(value.len()).expect("String too big");
        self.write_u16(length)?;
        self.0.write_all(value.as_bytes())
    }
}
