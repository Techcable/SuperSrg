use std::io::{Read, Cursor};
use std::path::{Path, PathBuf};
use std::str::{self, Utf8Error};

use git2::{Repository, Oid, Commit, Error as GitError};
use utils::{load_from_commit, CommitLoadError, download_text, DownloadError};
use mappings::MappingsBuilder;
use mappings::parser::{MappingsParser, CompactSrgParser, CompactSrgParseError};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VersionInfoRefs {
    pub build_data: String,
    pub bukkit: String,
    pub craft_bukkit: String,
    pub spigot: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub name: String,
    pub refs: VersionInfoRefs,
}
impl VersionInfo {
    #[inline]
    pub fn fetch(version: &str) -> Result<VersionInfo, SpigotError> {
        let text = download_text(version)?;
        Ok(::serde_json::from_str(&text)?)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDataInfo {
    pub minecraft_version: String,
    pub server_url: String,
    pub minecraft_hash: String,
    pub access_transforms: String,
    pub class_mappings: String,
    pub member_mappings: String,
    pub package_mappings: String,
}
impl BuildDataInfo {
    #[inline]
    pub fn read<R: Read>(input: &mut R) -> Result<BuildDataInfo, SpigotError> {
        Ok(::serde_json::from_reader(input)?)
    }
}

pub struct BuildData(pub Repository);
impl BuildData {
    pub fn find_commit(&self, id: Oid) -> Result<BuildDataCommit, SpigotError> {
        let commit = self.0.find_commit(id)?;
        let mut build_data_buffer = String::new();
        load_from_commit(
            &self.0,
            &commit,
            Path::new("info.json"),
            &mut build_data_buffer,
        )?;
        let info = BuildDataInfo::read(&mut Cursor::new(build_data_buffer))?;
        Ok(BuildDataCommit {
            info,
            commit,
            data: self,
        })
    }
}
pub struct BuildDataCommit<'a> {
    info: BuildDataInfo,
    commit: Commit<'a>,
    data: &'a BuildData,
}
impl<'a> BuildDataCommit<'a> {
    #[inline]
    fn load(&self, path: &Path, buffer: &mut String) -> Result<(), SpigotError> {
        load_from_commit(&self.data.0, &self.commit, path, buffer)?;
        Ok(())
    }
    pub fn read_class_mappings(&self) -> Result<MappingsBuilder, SpigotError> {
        /// Approximate size of the build data class mappings
        let mut buffer = String::with_capacity(64 * 1024);
        self.load_class_mapping_data(&mut buffer)?;
        buffer.shrink_to_fit();
        let mut parser = CompactSrgParser::default();
        parser.parse_text(&buffer)?;
        Ok(parser.finish())
    }
    pub fn read_member_mappings(&self) -> Result<MappingsBuilder, SpigotError> {
        /// Approximate size of the build data member mappings
        let mut buffer = String::with_capacity(128 * 1024);
        self.load_member_mapping_data(&mut buffer)?;
        buffer.shrink_to_fit();
        let mut parser = CompactSrgParser::default();
        parser.parse_text(&buffer)?;
        Ok(parser.finish())
    }
    fn load_class_mapping_data(&self, buffer: &mut String) -> Result<(), SpigotError> {
        let mut path = PathBuf::from("mappings");
        path.push(&self.info.class_mappings);
        self.load(&path, buffer)?;
        Ok(())
    }
    fn load_member_mapping_data(&self, buffer: &mut String) -> Result<(), SpigotError> {
        let mut path = PathBuf::from("mappings");
        path.push(&self.info.member_mappings);
        self.load(&path, buffer)?;
        Ok(())
    }
}
pub enum SpigotError {
    Git(GitError),
    InvalidUtf8(Utf8Error),
    InvalidCompactSrg(CompactSrgParseError),
    InvalidJson(::serde_json::Error),
    Download(DownloadError),
}
impl From<DownloadError> for SpigotError {
    #[inline]
    fn from(cause: DownloadError) -> SpigotError {
        SpigotError::Download(cause)
    }
}
impl From<CompactSrgParseError> for SpigotError {
    #[inline]
    fn from(cause: CompactSrgParseError) -> SpigotError {
        SpigotError::InvalidCompactSrg(cause)
    }
}
impl From<GitError> for SpigotError {
    #[inline]
    fn from(cause: GitError) -> SpigotError {
        SpigotError::Git(cause)
    }
}
impl From<Utf8Error> for SpigotError {
    #[inline]
    fn from(cause: Utf8Error) -> SpigotError {
        SpigotError::InvalidUtf8(cause)
    }
}
impl From<::serde_json::Error> for SpigotError {
    #[inline]
    fn from(cause: ::serde_json::Error) -> SpigotError {
        SpigotError::InvalidJson(cause)
    }
}
impl From<CommitLoadError> for SpigotError {
    fn from(cause: CommitLoadError) -> SpigotError {
        match cause {
            CommitLoadError::Git(cause) => SpigotError::Git(cause),
            CommitLoadError::InvalidUtf8(cause) => SpigotError::InvalidUtf8(cause),
        }
    }
}
