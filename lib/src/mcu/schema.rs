#![allow(clippy::significant_drop_tightening)]

use crate::mcu::{Api, ApiRequest, IApiClient};
use crate::mcu::{ApiClient, CommandApi};
use crate::util::join_all_results;

use futures::future::join_all;
use log::{debug, error, trace};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};

use std::fs;
use std::path::PathBuf;
use std::{collections, path::Path};
use strum::IntoEnumIterator;

use serde_json::Value;

#[derive(Deserialize, Serialize, Debug)]
pub struct RootEntry {
    list_endpoint: String,
    schema: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Root {
    name: String,
    base_schemas: collections::HashMap<String, RootEntry>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Endpoint {
    pub allowed_detail_http_methods: HashSet<Methods>,
    pub allowed_list_http_methods: HashSet<Methods>,
    pub default_limit: usize,
    pub fields: HashMap<String, Field>,
    #[serde(default, deserialize_with = "deserialize_filtering")]
    pub filtering: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub ordering: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[expect(clippy::struct_excessive_bools)]
pub struct Field {
    pub blank: bool,
    pub default: Option<Value>,
    pub help_text: String,
    pub nullable: bool,
    pub readonly: bool,
    #[serde(rename = "type")]
    pub data_type: Type,
    pub related_type: Option<RelationType>,
    pub unique: bool,
    pub valid_choices: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    String,
    Boolean,
    DateTime,
    Date,
    Time,
    Integer,
    Float,
    Related,
    List,
    File,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    ToOne,
    ToMany,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Methods {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

fn deserialize_filtering<'de, D>(deserializer: D) -> Result<HashMap<String, Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_map(FilteringVisitor {})
}

struct FilteringVisitor;

impl<'de> Visitor<'de> for FilteringVisitor {
    type Value = HashMap<String, Vec<String>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("filtering criteria")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut values = HashMap::new();
        while let Some((key, value)) = map.next_entry::<String, FilteringItem>()? {
            values.insert(key, value.0);
        }
        Ok(values)
    }
}

struct FilteringItem(Vec<String>);

impl<'de> Deserialize<'de> for FilteringItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(FilteringItemVisitor {})
    }
}

struct FilteringItemVisitor;

impl<'de> Visitor<'de> for FilteringItemVisitor {
    type Value = FilteringItem;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("1, 2, or a list of the allowed filters")
    }

    fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        const ALL_FILTERS: &[&str] = &[
            "exact",
            "iexact",
            "contains",
            "icontains",
            "startswith",
            "istartswith",
            "endswith",
            "iendswith",
            "regex",
            "iregex",
            "lt",
            "lte",
            "gt",
            "gte",
        ];
        Ok(FilteringItem(
            ALL_FILTERS.iter().copied().map(String::from).collect(),
        ))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(seq.size_hint().unwrap_or_default());
        while let Some(value) = seq.next_element::<String>()? {
            values.push(value);
        }
        Ok(FilteringItem(values))
    }
}

/// # Panics
/// Will panic if retrieving a valid cache directory from the operating system fails.
#[must_use]
pub fn cache_exists(cache_dir: &Path) -> bool {
    debug!("Checking for schema cache in {cache_dir:?}");
    if cache_dir.exists()
        && cache_dir
            .read_dir()
            .map(|mut i| i.next().is_some())
            .unwrap_or(false)
    {
        debug!("Detected existing schema cache");
        true
    } else {
        debug!("Schema cache not found");
        false
    }
}

#[must_use]
fn get_root_cache_path(cache_dir: &Path, api: Api) -> PathBuf {
    let api_part = match api {
        Api::Configuration => "configuration",
        Api::History => "history",
        Api::Status => "status",
        Api::Command(CommandApi::Conference) => "command/conference",
        Api::Command(CommandApi::Participant) => "command/participant",
        Api::Command(CommandApi::Platform) => "command/platform",
    };

    cache_dir.join(api_part)
}

#[must_use]
fn get_endpoint_cache_path(cache_dir: &Path, api: Api, endpoint: &str) -> PathBuf {
    let mut path = get_root_cache_path(cache_dir, api);
    path.push(endpoint);
    path.set_extension("json");
    path
}

pub async fn read_schema_from_cache(
    cache_dir: &Path,
    api: Api,
    endpoint: &str,
) -> std::io::Result<Endpoint> {
    let path = get_endpoint_cache_path(cache_dir, api, endpoint);
    trace!("Reading schema from cache file: {path:?}");
    let schema = tokio::fs::read_to_string(path).await?;
    let schema: Endpoint = serde_json::from_str(&schema)?;
    Ok(schema)
}

pub async fn read_all_schemas(
    cache_dir: &Path,
) -> std::io::Result<HashMap<Api, HashMap<String, Endpoint>>> {
    let mut all_schemas = HashMap::new();
    for api in Api::iter() {
        let root_schema_path = get_endpoint_cache_path(cache_dir, api, "root");
        let root_schema = tokio::fs::read_to_string(root_schema_path).await?;
        let root_schema: HashMap<String, RootEntry> = serde_json::from_str(&root_schema)?;

        let results: Vec<(String, std::io::Result<Endpoint>)> =
            join_all(root_schema.keys().map(|name| async move {
                let schema = read_schema_from_cache(cache_dir, api, name).await;
                (name.clone(), schema)
            }))
            .await;

        let mut map = HashMap::with_capacity(results.len());
        for (name, r) in results {
            match r {
                Ok(schema) => {
                    map.insert(name, schema);
                }
                Err(e) => {
                    error!("Failed to read schema for endpoint \"{name}\": {e}");
                }
            }
        }

        all_schemas.insert(api, map);
    }

    Ok(all_schemas)
}

pub async fn cache_schemas(api_client: &ApiClient<'_>, cache_dir: &Path) -> anyhow::Result<()> {
    join_all_results(Api::iter().map(|api| cache_api(api_client, cache_dir, api))).await?;

    Ok(())
}

async fn cache_api(api_client: &ApiClient<'_>, cache_dir: &Path, api: Api) -> anyhow::Result<()> {
    let root_request = ApiRequest::ApiSchema { api };
    let json = api_client
        .send(root_request)
        .await?
        .unwrap_content_or_default();
    let root_cache_file_path = get_endpoint_cache_path(cache_dir, api, "root");
    fs::create_dir_all(root_cache_file_path.parent().unwrap())?;
    fs::write(root_cache_file_path, json.to_string())?;
    let root_schema: HashMap<String, RootEntry> = serde_json::from_str(&json.to_string())?;
    join_all_results(
        root_schema
            .keys()
            .map(|endpoint| cache_schema(api_client, cache_dir, api, endpoint)),
    )
    .await?;

    Ok(())
}

async fn cache_schema(
    api_client: &ApiClient<'_>,
    cache_dir: &Path,
    api: Api,
    endpoint: &str,
) -> anyhow::Result<()> {
    let request = ApiRequest::Schema {
        api,
        resource: String::from(endpoint),
    };

    let cache_file_path = get_endpoint_cache_path(cache_dir, api, endpoint);

    let json = api_client.send(request).await?.unwrap_content_or_default();

    fs::create_dir_all(cache_file_path.parent().unwrap())?;
    fs::write(cache_file_path, json.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::future_not_send)]

    use super::*;
    use googletest::prelude::*;
    use serde_json::json;
    use test_case::test_case;
    use wiremock::{
        matchers::{basic_auth, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        mcu::{auth::BasicAuth, ApiClient},
        util::SensitiveString,
    };
    use test_helpers::get_test_context;

    const USERNAME: &str = "test";
    const PASSWORD: &str = "testing123";

    #[test_case(Api::Configuration, "configuration/v1", "configuration")]
    #[test_case(Api::History, "history/v1", "history")]
    #[test_case(Api::Status, "status/v1", "status")]
    #[test_case(
        Api::Command(CommandApi::Conference),
        "command/v1/conference",
        "command/conference"
    )]
    #[test_case(
        Api::Command(CommandApi::Participant),
        "command/v1/participant",
        "command/participant"
    )]
    #[test_case(
        Api::Command(CommandApi::Platform),
        "command/v1/platform",
        "command/platform"
    )]
    #[tokio::test]
    async fn test_cache_api(api: Api, api_path: &str, cache_path_from_root: &str) {
        // Arrange
        let server = MockServer::start().await;
        let test_context = get_test_context();
        let cache_path = test_context.get_cache_dir().to_str().unwrap();
        let root_schema = json!({
            "test_endpoint": {
                "list_endpoint": format!("/api/admin/{api_path}/test_endpoint/"),
                "schema": format!("/api/admin/{api_path}/test_endpoint/schema/")
            },
            "another_test_endpoint": {
                "list_endpoint": format!("/api/admin/{api_path}/another_test_endpoint/"),
                "schema": format!("/api/admin/{api_path}/another_test_endpoint/schema/")
            },
        });
        let mut test_endpoint_schema = json_schema();
        test_endpoint_schema["marker"] = json!("test_endpoint");
        let mut another_test_endpoint_schema = json_schema();
        another_test_endpoint_schema["marker"] = json!("another_test_endpoint");

        Mock::given(method("GET"))
            .and(path(format!("/api/admin/{api_path}/")))
            .and(basic_auth(USERNAME, PASSWORD))
            .respond_with(ResponseTemplate::new(200).set_body_json(&root_schema))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/api/admin/{api_path}/test_endpoint/schema/")))
            .respond_with(ResponseTemplate::new(200).set_body_json(&test_endpoint_schema))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/admin/{api_path}/another_test_endpoint/schema/"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(&another_test_endpoint_schema))
            .mount(&server)
            .await;

        let http_client = reqwest::Client::new();
        let api_client = ApiClient::new_for_testing(
            http_client,
            server.uri(),
            BasicAuth::new(String::from(USERNAME), SensitiveString::from(PASSWORD)),
        );

        // Act
        cache_api(&api_client, &PathBuf::from(&cache_path), api)
            .await
            .unwrap();

        // Assert
        let root_schema_from_cache: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(format!("{cache_path}/{cache_path_from_root}/root.json"))
                .unwrap()
                .as_str(),
        )
        .unwrap();

        let test_endpoint_schema_from_cache: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(format!(
                "{cache_path}/{cache_path_from_root}/test_endpoint.json"
            ))
            .unwrap()
            .as_str(),
        )
        .unwrap();

        let another_test_endpoint_schema_from_cache: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(format!(
                "{cache_path}/{cache_path_from_root}/another_test_endpoint.json"
            ))
            .unwrap()
            .as_str(),
        )
        .unwrap();

        assert_that!(root_schema_from_cache, eq(&root_schema));
        assert_that!(test_endpoint_schema_from_cache, eq(&test_endpoint_schema));
        assert_that!(
            another_test_endpoint_schema_from_cache,
            eq(&another_test_endpoint_schema)
        );
    }

    #[test_case(Api::Configuration, "configuration/v1", "configuration")]
    #[test_case(Api::History, "history/v1", "history")]
    #[test_case(Api::Status, "status/v1", "status")]
    #[test_case(
        Api::Command(CommandApi::Conference),
        "command/v1/conference",
        "command/conference"
    )]
    #[test_case(
        Api::Command(CommandApi::Participant),
        "command/v1/participant",
        "command/participant"
    )]
    #[test_case(
        Api::Command(CommandApi::Platform),
        "command/v1/platform",
        "command/platform"
    )]
    #[tokio::test]
    async fn test_cache_schema(api: Api, api_path: &str, cache_path_from_root: &str) {
        // Arrange
        let server = MockServer::start().await;
        let test_context = get_test_context();
        let cache_path = test_context.get_cache_dir().to_str().unwrap();
        let endpoint = "some_endpoint";

        Mock::given(method("GET"))
            .and(path(format!("/api/admin/{api_path}/{endpoint}/schema/")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json_schema()))
            .mount(&server)
            .await;

        let http_client = reqwest::Client::new();
        let api_client = ApiClient::new_for_testing(
            http_client,
            server.uri(),
            BasicAuth::new(String::from(USERNAME), SensitiveString::from(PASSWORD)),
        );

        // Act
        cache_schema(&api_client, &PathBuf::from(&cache_path), api, endpoint)
            .await
            .unwrap();

        // Assert
        eprintln!("file path: {cache_path}/{cache_path_from_root}/{endpoint}.json");
        let schema: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(format!(
                "{cache_path}/{cache_path_from_root}/{endpoint}.json"
            ))
            .unwrap()
            .as_str(),
        )
        .unwrap();

        assert_that!(schema, eq(&json_schema()));
        std::fs::remove_dir_all(cache_path).unwrap();
    }

    #[expect(clippy::too_many_lines)]
    fn json_schema() -> serde_json::Value {
        json!({
            "allowed_detail_http_methods": [
                "get",
                "post",
                "put",
                "delete",
                "patch"
            ],
            "allowed_list_http_methods": [
                "get",
                "post",
                "put",
                "delete",
                "patch"
            ],
            "default_format": "application/json",
            "default_limit": 20,
            "fields": {
                "aliases": {
                    "blank": false,
                    "default": null,
                    "help_text": "The aliases associated with this conference.",
                    "nullable": true,
                    "readonly": false,
                    "related_type": "to_many",
                    "type": "related",
                    "unique": false
                },
                "creation_time": {
                    "blank": false,
                    "default": "2022-04-01T15:58:06.714308",
                    "help_text": "The time at which the configuration was created.",
                    "nullable": false,
                    "readonly": false,
                    "type": "datetime",
                    "unique": false
                },
                "crypto_mode": {
                    "blank": false,
                    "default": "",
                    "help_text": "Controls the media encryption requirements for participants connecting to this service. Use global setting: Use the global media encryption setting (Platform > Global Settings). Required: All participants (including RTMP participants) must use media encryption. Best effort: Each participant will use media encryption if their device supports it, otherwise the connection will be unencrypted. No encryption: All H.323, SIP and MS-SIP participants must use unencrypted media. (RTMP participants will use encryption if their device supports it, otherwise the connection will be unencrypted.)",
                    "nullable": true,
                    "readonly": false,
                    "type": "string",
                    "unique": false,
                    "valid_choices": [
                        "besteffort",
                        "on",
                        "off"
                    ]
                },
                "description": {
                    "blank": true,
                    "default": "",
                    "help_text": "A description of the service. Maximum length: 250 characters.",
                    "nullable": false,
                    "readonly": false,
                    "type": "string",
                    "unique": false
                },
                "guest_pin": {
                    "blank": true,
                    "default": "",
                    "help_text": "This optional field allows you to set a secure access code for Guest participants who dial in to the service. Length: 4-20 digits, including any terminal #.",
                    "nullable": false,
                    "readonly": false,
                    "type": "string",
                    "unique": false
                },
                "guests_can_present": {
                    "blank": false,
                    "default": true,
                    "help_text": "If enabled, Guests and Hosts can present into the conference. If disabled, only Hosts can present.",
                    "nullable": false,
                    "readonly": false,
                    "type": "boolean",
                    "unique": false,
                    "valid_choices": [
                        true,
                        false
                    ]
                },
                "id": {
                    "blank": true,
                    "default": "",
                    "help_text": "The primary key.",
                    "nullable": false,
                    "readonly": false,
                    "type": "integer",
                    "unique": true
                },
                "ivr_theme": {
                    "blank": false,
                    "default": null,
                    "help_text": "The theme for use with this service.",
                    "nullable": true,
                    "readonly": false,
                    "related_type": "to_one",
                    "type": "related",
                    "unique": false
                },
                "max_pixels_per_second": {
                    "blank": false,
                    "default": null,
                    "help_text": "Sets the maximum call quality for each participant.",
                    "nullable": true,
                    "readonly": false,
                    "type": "string",
                    "unique": false,
                    "valid_choices": [
                        "sd",
                        "hd",
                        "fullhd"
                    ]
                },
                "media_playlist": {
                    "blank": false,
                    "default": null,
                    "help_text": "The playlist to run when this Media Playback Service is used.",
                    "nullable": true,
                    "readonly": false,
                    "related_type": "to_one",
                    "type": "related",
                    "unique": false
                },
                "mute_all_guests": {
                    "blank": false,
                    "default": false,
                    "help_text": "If enabled, all Guest participants will be muted by default.",
                    "nullable": false,
                    "readonly": false,
                    "type": "boolean",
                    "unique": false,
                    "valid_choices": [
                        true,
                        false
                    ]
                },
                "participant_limit": {
                    "blank": false,
                    "default": null,
                    "help_text": "This optional field allows you to limit the number of participants allowed to join this Virtual Meeting Room. Range: 0 to 1000000.",
                    "nullable": true,
                    "readonly": false,
                    "type": "integer",
                    "unique": false
                },
                "pin": {
                    "blank": true,
                    "default": "",
                    "help_text": "This optional field allows you to set a secure access code for participants who dial in to the service. Length: 4-20 digits, including any terminal #.",
                    "nullable": false,
                    "readonly": false,
                    "type": "string",
                    "unique": false
                },
                "resource_uri": {
                    "blank": false,
                    "help_text": "The URI that identifies this resource.",
                    "nullable": false,
                    "readonly": true,
                    "type": "string",
                    "unique": false
                },
            },
            "filtering": {
                "aliases": 2,
                "allow_guests": 1,
                "creation_time": 1,
                "description": 1,
                "enable_chat": 1,
                "enable_overlay_text": 1,
                "gms_access_token": 2,
                "guest_pin": 1,
                "ivr_theme": 2,
                "max_callrate_in": 1,
                "max_callrate_out": 1,
                "media_playlist": 2,
                "mssip_proxy": 2,
                "name": 1,
                "participant_limit": 1,
                "pin": 1,
                "primary_owner_email_address": 1,
                "scheduled_conferences": 2,
                "scheduled_conferences_count": 1,
                "service_type": 1,
                "system_location": 2,
                "tag": 1,
                "two_stage_dial_type": 1
            },
            "ordering": [
                "name",
                "tag",
                "description",
                "service_type",
                "pin",
                "allow_guests",
                "enable_chat",
                "guest_pin",
                "max_callrate_in",
                "max_callrate_out",
                "participant_limit",
                "primary_owner_email_address",
                "enable_overlay_text",
                "creation_time"
            ]
        })
    }
}
