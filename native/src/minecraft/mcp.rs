use std::borrow::Cow;

use string_cache::DefaultAtom;
use ordermap::OrderMap;

use super::MinecraftMappingError;
use utils::{SeaHashSerializableOrderMap, PooledString, SerializableOrderMap};
use mappings::MappingsTransformer;
use types::{FieldDataLookup, MethodDataLookup};

#[derive(Serialize, Deserialize, Default)]
pub struct McpMappings {
    pub fields: SeaHashSerializableOrderMap<PooledString, PooledString>,
    pub methods: SeaHashSerializableOrderMap<PooledString, PooledString>,
}
impl McpMappings {
    #[inline]
    pub fn with_capacity(capacity: usize) -> McpMappings {
        McpMappings {
            fields: SerializableOrderMap(OrderMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            )),
            methods: SerializableOrderMap(OrderMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            )),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct McpMetadata(pub SeaHashSerializableOrderMap<String, McpVersionInfo>);
#[derive(Default, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct McpVersionInfo {
    snapshot: Vec<u64>,
    stable: Vec<u64>,
}
impl McpVersionInfo {
    pub fn available_versions(&self, channel: &str, mcp_version: &str) -> Result<&[u64], MinecraftMappingError> {
        match channel {
            "stable" => Ok(&self.stable),
            "snapshot" => Ok(&self.snapshot),
            _ => {
                debug!("Unknown channel: {}", channel);
                Err(MinecraftMappingError::InvalidMcpVersion(
                    mcp_version.to_owned(),
                    "Unknown channel",
                    None,
                ))
            }
        }
    }
}
impl MappingsTransformer for McpMappings {
    #[inline]
    fn transform_method<T: MethodDataLookup>(&self, original: &T) -> Option<Cow<DefaultAtom>> {
        self.methods
            .get(original.pooled_name().as_ref())
            .map(|x| &x.0)
            .map(Cow::Borrowed)
    }
    #[inline]
    fn transform_field<T: FieldDataLookup>(&self, original: &T) -> Option<Cow<DefaultAtom>> {
        self.fields
            .get(original.pooled_name().as_ref())
            .map(|x| &x.0)
            .map(Cow::Borrowed)
    }
}
