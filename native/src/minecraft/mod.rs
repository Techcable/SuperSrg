use std::str::Utf8Error;
use std::path::PathBuf;
use std::io::{self, Read, BufReader, BufWriter, Cursor};
use std::fs::{File, create_dir_all};
use std::error::Error;
use std::num::ParseIntError;

use git2::{Repository, Oid, Error as GitError};
use zip::ZipArchive;
use zip::result::ZipError;
use rmp_serde::{Serializer as RmpSerializer, Deserializer as RmpDeserializer};
use serde_json::Deserializer as JsonDeserializer;
use serde::{Serialize, Deserialize};
use regex::Regex;
use chrono::Utc;

pub mod mcp;
pub mod spigot;
pub mod targets;

use self::spigot::{BuildData, SpigotError};
use self::mcp::{McpMetadata, McpMappings};
use self::targets::TargetModifier;
use utils::{PooledString, SeaHashSerializableOrderMap, download_buffer, download_text, DownloadError};
use types::JavaClassLookup;
use mappings::{MappingsBuilder, Mappings};
use mappings::utils::PackageTransformer;
use mappings::parser::{CompactSrgParseError, MappingsParser, SrgMappingsParser, SrgParseError};
use mappings::binary::{MappingsDecoder, MappingsEncoder, BinaryMappingError};

pub struct MinecraftMappingsCache {
    location: PathBuf,
}
impl MinecraftMappingsCache {
    #[inline]
    pub fn new(location: PathBuf) -> Self {
        MinecraftMappingsCache { location }
    }
    fn fetch_mcp_mapping_metadata(&self, force_update: bool) -> Result<McpMetadata, MinecraftMappingError> {
        let mcp_metadata = self.location.join("mcp-metadata.dat");
        if !force_update && mcp_metadata.exists() {
            let file = BufReader::new(File::open(&mcp_metadata)?);
            let mut deserializer = RmpDeserializer::from_read(file);
            Ok(McpMetadata::deserialize(&mut deserializer)?)
        } else {
            println!("Fetching MCP mapping metadata");
            let text = download_text("http://export.mcpbot.bspk.rs/versions.json")?;
            let mut deserializer = JsonDeserializer::from_str(&text);
            let file = BufWriter::new(File::create(&mcp_metadata)?);
            let mut serializer = RmpSerializer::new(file);
            let metadata = McpMetadata::deserialize(&mut deserializer)?;
            metadata.serialize(&mut serializer)?;
            Ok(metadata)
        }
    }
    #[allow(unused_assignments)] // We need to maintain the 'refreshed' flag in case the code ever changes
    fn fetch_mcp_mappings(&self, mcp_version: &str, minecraft_version: &str) -> Result<McpMappings, MinecraftMappingError> {
        let version_dir = self.location.join(format!("version-{}", minecraft_version));
        let mcp_mappings_file = version_dir.join(format!("mcp-{}.dat", mcp_version));
        if mcp_mappings_file.exists() {
            let file = BufReader::new(File::open(&mcp_mappings_file)?);
            let mut deserializer = RmpDeserializer::new(file);
            Ok(McpMappings::deserialize(&mut deserializer)?)
        } else {
            let mut mappings_metadata = self.fetch_mcp_mapping_metadata(false)?;
            let mut refreshed = false;
            let mut mcp_version_info = match mappings_metadata.0.get(minecraft_version).cloned() {
                Some(result) => result,
                None => {
                    // Force refresh
                    assert!(!refreshed);
                    mappings_metadata = self.fetch_mcp_mapping_metadata(true)?;
                    refreshed = true;
                    mappings_metadata
                        .0
                        .get(minecraft_version)
                        .ok_or_else(|| {
                            MinecraftMappingError::UnknownMinecraftVersion(minecraft_version.to_owned())
                        })?
                        .clone()
                }
            };
            lazy_static! {
                static ref MAPPINGS_PATTERN: Regex = Regex::new(r#"(\w+?)(?:_(nodoc))?_(\d+)"#).unwrap();
            }
            let captures = MAPPINGS_PATTERN.captures(mcp_version).ok_or_else(|| {
                MinecraftMappingError::InvalidMcpVersion(mcp_version.to_owned(), "Malformed version", None)
            })?;
            let channel = &captures[1];
            let nodoc = captures.get(2).is_some();
            let version_num: u64 = captures[3].parse().map_err(|e: ParseIntError| {
                MinecraftMappingError::InvalidMcpVersion(
                    mcp_version.to_owned(),
                    "Invalid version number",
                    Some(Box::new(e)),
                )
            })?;
            let is_known_version = {
                let available_versions = mcp_version_info.available_versions(channel, mcp_version)?;
                available_versions.contains(&version_num)
            };
            if !is_known_version {
                let available_versions = if !refreshed {
                    // Refresh and try again
                    mappings_metadata = self.fetch_mcp_mapping_metadata(true)?;
                    mcp_version_info = mappings_metadata
                        .0
                        .get(minecraft_version)
                        .ok_or_else(|| {
                            MinecraftMappingError::UnknownMinecraftVersion(minecraft_version.to_owned())
                        })?
                        .clone();
                    refreshed = true;
                    mcp_version_info.available_versions(channel, mcp_version)?
                } else {
                    mcp_version_info.available_versions(channel, mcp_version)?
                };
                if !available_versions.contains(&version_num) {
                    return Err(MinecraftMappingError::InvalidMcpVersion(
                        mcp_version.to_owned(),
                        "Unknown version number",
                        None,
                    ));
                }
            }
            // http://export.mcpbot.bspk.rs/mcp_snapshot_nodoc/20170702-1.12/mcp_snapshot_nodoc-20170702-1.12.zip
            let channel_nodoc = if nodoc {
                format!("{}_nodoc", channel)
            } else {
                channel.to_owned()
            };
            let url = format!(
                "http://export.mcpbot.bspk.rs/mcp_{0}/{1}-{2}/mcp_{0}-{1}-{2}.zip",
                channel_nodoc,
                version_num,
                minecraft_version
            );
            let start = Utc::now();
            let buffer = download_buffer(&url)?;
            let mut archive = ZipArchive::new(Cursor::new(&buffer))?;
            #[inline]
            fn parse_mcp_csv<R: Read, F: FnMut(&str, &str) -> Result<(), MinecraftMappingError>>(input: R, mut func: F) -> Result<(), MinecraftMappingError> {
                let mut reader = ::csv::Reader::from_reader(input);
                {
                    let headers = reader.headers()?;
                    if headers.get(0) != Some("searge") {
                        return Err(MinecraftMappingError::UnexpectedCsv(
                            format!("Unexpected first header: {:?}", headers.get(0)),
                        ));
                    } else if headers.get(1) != Some("name") {
                        return Err(MinecraftMappingError::UnexpectedCsv(
                            format!("Unexpected second header: {:?}", headers.get(2)),
                        ));
                    }
                }
                for result in reader.into_records() {
                    let record = result?;
                    func(&record[0], &record[1])?;
                }
                Ok(())
            }
            // Rounded up size for the mcp mappings, giving plenty of room to spare
            let mut result = McpMappings::with_capacity(1024 * 8);
            {
                parse_mcp_csv(archive.by_name("fields.csv")?, |searge, mcp| {
                    result.fields.insert(
                        PooledString::from(searge),
                        PooledString::from(mcp),
                    );
                    Ok(())
                })?;
                parse_mcp_csv(archive.by_name("methods.csv")?, |searge, mcp| {
                    result.methods.insert(
                        PooledString::from(searge),
                        PooledString::from(mcp),
                    );
                    Ok(())
                })?;
            }
            let file = BufWriter::new(File::create(&mcp_mappings_file)?);
            let mut serializer = RmpSerializer::new(file);
            result.serialize(&mut serializer)?;
            let end = Utc::now();
            let duration = end.signed_duration_since(start);
            println!("Fetched MCP mappings {} for {}: {:.2} seconds", mcp_version, minecraft_version, duration.num_milliseconds() as f64 / 1000.0);
            Ok(result)
        }
    }
    fn load_srg_mappings(&self, version: &str) -> Result<MappingsBuilder, MinecraftMappingError> {
        let version_dir = self.location.join(format!("version-{}", version));
        let binary_srg_mappings = version_dir.join("joined-mcp.srg.dat");
        if binary_srg_mappings.exists() {
            let decoder = MappingsDecoder::from_path(&binary_srg_mappings)?;
            let mut builder = MappingsBuilder::new();
            decoder.decode(&mut builder)?;
            Ok(builder)
        } else {
            create_dir_all(version_dir)?;
            let traditional_srg_mappings = self.fetch_srg_mappings(version)?;
            let mut parser = SrgMappingsParser::default();
            parser.ignore_package_mappings = true;
            parser.read_path(&traditional_srg_mappings)?;
            let result = parser.finish();
            MappingsEncoder::create_path(&binary_srg_mappings)?.encode(
                &result.snapshot(),
            )?;
            Ok(result)
        }
    }
    fn fetch_srg_mappings(&self, version: &str) -> Result<PathBuf, MinecraftMappingError> {
        let version_dir = self.location.join(format!("version-{}", version));
        // TODO: Make this temp file, and use only the binary mappings
        let srg_mapings = version_dir.join("joined-mcp.srg");
        if !srg_mapings.exists() {
            println!("Fetching srg mappings for {}", version);
            create_dir_all(version_dir)?;
            let mappings_url = format!(
                "http://files.minecraftforge.net/maven/de/oceanlabs/mcp/mcp/{0}/mcp-{0}-srg.zip",
                version
            );
            let buffer = download_buffer(&mappings_url)?;
            let mut archive = ZipArchive::new(Cursor::new(&buffer))?;
            let mut entry = archive.by_name("joined.srg")?;
            let mut file = File::create(&srg_mapings)?;
            io::copy(&mut entry, &mut file)?;
        }
        Ok(srg_mapings)
    }
    /// Fetch spigot BuildData and ensure it contains the specified commit
    pub fn fetch_build_data(&self, commit: &str) -> Result<BuildData, MinecraftMappingError> {
        let repo_location = self.location.join("BuildData");
        create_dir_all(repo_location.parent().unwrap())?;
        let repo_url = "https://hub.spigotmc.org/stash/scm/spigot/builddata.git";
        let commit_id = Oid::from_str(commit)?;
        let repo = if !repo_location.exists() {
            println!("Fetching BuildData@{}", commit);
            Repository::clone(repo_url, repo_location)?
        } else {
            let repo = Repository::open(repo_location)?;
            if repo.find_commit(commit_id).is_err() {
                println!("Updating BuildData@{}", commit);
                // Update the repo if we don't have the commit we want
                let mut remote = repo.remote_anonymous(repo_url)?;
                remote.fetch(
                    &["master", format!(":{}", commit).as_ref()],
                    None,
                    None,
                )?;
            }
            repo
        };
        Ok(BuildData(repo))
    }
    /// Compute the spigot BuildData for the latest commit
    /// Note that to avoid the overhead of a web request, we cache the BuildData commit for each version,
    /// which must be explictly invaliated if you want the latest information
    pub fn compute_spigot(&self, version: &str) -> Result<MappingsBuilder, MinecraftMappingError> {
        let builddata_commit = self.builddata_commit(version, false)?;
        let version_dir = self.location.join(format!("version-{}", version));
        let spigot_mappings_file = version_dir.join(format!("spigot-{}.srg.dat", builddata_commit));
        if spigot_mappings_file.exists() {
            let mut builder = MappingsBuilder::new();
            let decoder = MappingsDecoder::from_path(&spigot_mappings_file)?;
            decoder.decode(&mut builder)?;
            Ok(builder)
        } else {
            println!("Computing spigot mappings for {} with BuildData@{}", version, builddata_commit);
            create_dir_all(version_dir)?;
            let build_data = self.fetch_build_data(&builddata_commit)?;
            let commit = build_data.find_commit(
                Oid::from_str(&builddata_commit).expect(
                    "Malformed commit",
                ),
            )?;
            let mut mappings = commit.read_class_mappings()?;
            self.debug_dump(&mappings, "spigot-cl");
            // Strip invalid classes
            mappings.classes.retain(|original, renamed| {
                !original.internal_name().contains('#') && !renamed.internal_name().contains('#')
            });
            let member_mappings = commit.read_member_mappings()?;
            self.debug_dump(&member_mappings, "spigot-raw-members");
            mappings.chain(&member_mappings);
            self.debug_dump(&mappings, "spigot-members");
            mappings.transform(&PackageTransformer::single(
                "".to_owned(),
                "net/minecraft/server".to_owned(),
            ));
            // Cache the result so we don't have to go through this again
            let encoder = MappingsEncoder::create_path(&spigot_mappings_file)?;
            encoder.encode(&mappings.snapshot())?;
            Ok(mappings)
        }
    }
    #[cfg(not(debug_assertions))]
    #[inline]
    pub fn debug_dump(&self, _: &MappingsBuilder, _: &str) {}
    #[cfg(debug_assertions)]
    pub fn debug_dump(&self, builder: &MappingsBuilder, id: &str) {
        use mappings::encoder::{MappingsEncoder, SrgEncoder};
        // TODO: Remove debugging dumps
        let debug_dumps = self.location.join("debug");
        create_dir_all(&debug_dumps).unwrap();
        let mut writer = BufWriter::new(
            File::create(debug_dumps.join(format!("{}.srg", id))).unwrap(),
        );
        let mappings = builder.snapshot();
        let encoder = SrgEncoder::new(&mappings);
        encoder.write(&mut writer).unwrap();
    }
    pub fn builddata_commit(&self, version: &str, force_refresh: bool) -> Result<String, MinecraftMappingError> {
        let metadata_file = self.location.join("builddata-commits.dat");
        // NOTE: We need to load the existing data regardless of whether we force refresh, so we can save it again
        let mut existing_commits: SeaHashSerializableOrderMap<String, String> = if metadata_file.exists() {
            let reader = BufReader::new(File::open(&metadata_file)?);
            let mut deserializer = RmpDeserializer::from_read(reader);
            SeaHashSerializableOrderMap::deserialize(&mut deserializer)?
        } else {
            SeaHashSerializableOrderMap::default()
        };
        if !force_refresh {
            // NOTE: Okay to remove this since if we succeed we won't be saving the map
            if let Some(commit) = existing_commits.remove(&version.to_owned()) {
                return Ok(commit);
            }
        }
        let commit = {
            let metadata_text = download_text(&format!(
                "https://hub.spigotmc.org/versions/{}.json",
                version
            ))?;
            let json_metadata: self::spigot::VersionInfo = ::serde_json::from_str(&metadata_text)?;
            json_metadata.refs.build_data
        };
        trace!("Fetched BuildData commit for {}: {}", version, commit);
        // Now cache it for future use
        existing_commits.insert(version.to_owned(), commit.clone());
        let writer = BufWriter::new(File::create(&metadata_file)?);
        let mut serializer = RmpSerializer::new(writer);
        existing_commits.serialize(&mut serializer)?;
        Ok(commit)
    }
}

#[derive(Debug)] // TODO: Implement display
pub enum MinecraftMappingError {
    IOError(io::Error),
    InvalidJson(::serde_json::Error),
    InvalidMsgpack(::rmp_serde::decode::Error),
    MsgpackEncodeFailure(::rmp_serde::encode::Error),
    InvalidMcpVersion(String, &'static str, Option<Box<Error>>),
    UnknownMinecraftVersion(String),
    InvalidUtf8(Utf8Error),
    InvalidCompactSrg(CompactSrgParseError),
    InvalidSrg(SrgParseError),
    InvalidBinaryMapping(BinaryMappingError),
    Git(GitError),
    Curl(::curl::Error),
    Zip(ZipError),
    Csv(::csv::Error),
    UnexpectedCsv(String),
    /// Indicates that a target isn't valid
    InvalidTarget(String),
    IncompatibleModifiers(String, (TargetModifier, TargetModifier)),
}
impl From<::csv::Error> for MinecraftMappingError {
    #[inline]
    fn from(cause: ::csv::Error) -> MinecraftMappingError {
        MinecraftMappingError::Csv(cause)
    }
}
impl From<io::Error> for MinecraftMappingError {
    #[inline]
    fn from(cause: io::Error) -> MinecraftMappingError {
        MinecraftMappingError::IOError(cause)
    }
}
impl From<ZipError> for MinecraftMappingError {
    #[inline]
    fn from(cause: ZipError) -> MinecraftMappingError {
        MinecraftMappingError::Zip(cause)
    }
}
impl From<::serde_json::Error> for MinecraftMappingError {
    #[inline]
    fn from(cause: ::serde_json::Error) -> MinecraftMappingError {
        MinecraftMappingError::InvalidJson(cause)
    }
}
impl From<::rmp_serde::decode::Error> for MinecraftMappingError {
    #[inline]
    fn from(cause: ::rmp_serde::decode::Error) -> MinecraftMappingError {
        MinecraftMappingError::InvalidMsgpack(cause)
    }
}
impl From<::rmp_serde::encode::Error> for MinecraftMappingError {
    #[inline]
    fn from(cause: ::rmp_serde::encode::Error) -> MinecraftMappingError {
        MinecraftMappingError::MsgpackEncodeFailure(cause)
    }
}
impl From<GitError> for MinecraftMappingError {
    #[inline]
    fn from(cause: GitError) -> MinecraftMappingError {
        MinecraftMappingError::Git(cause)
    }
}
impl From<SpigotError> for MinecraftMappingError {
    #[inline]
    fn from(cause: SpigotError) -> MinecraftMappingError {
        match cause {
            SpigotError::Git(cause) => MinecraftMappingError::Git(cause),
            SpigotError::InvalidUtf8(cause) => MinecraftMappingError::InvalidUtf8(cause),
            SpigotError::InvalidJson(cause) => MinecraftMappingError::InvalidJson(cause),
            SpigotError::InvalidCompactSrg(cause) => MinecraftMappingError::InvalidCompactSrg(cause),
            SpigotError::Download(cause) => MinecraftMappingError::from(cause),
        }
    }
}
impl From<DownloadError> for MinecraftMappingError {
    fn from(cause: DownloadError) -> MinecraftMappingError {
        match cause {
            DownloadError::Curl(cause) => MinecraftMappingError::Curl(cause),
            DownloadError::IOError(cause) => MinecraftMappingError::IOError(cause),
            DownloadError::InvalidUtf8(cause) => MinecraftMappingError::InvalidUtf8(cause),
        }
    }
}
impl From<BinaryMappingError> for MinecraftMappingError {
    #[inline]
    fn from(cause: BinaryMappingError) -> MinecraftMappingError {
        MinecraftMappingError::InvalidBinaryMapping(cause)
    }
}
impl From<SrgParseError> for MinecraftMappingError {
    #[inline]
    fn from(cause: SrgParseError) -> MinecraftMappingError {
        MinecraftMappingError::InvalidSrg(cause)
    }
}
