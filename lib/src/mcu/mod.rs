mod error;
pub mod schema;

use std::fmt;
use std::sync::Arc;
use std::{collections::HashMap, error::Error};

use async_stream::try_stream;
use async_trait::async_trait;
use futures::stream::StreamExt;
use futures::Stream;
use log::{debug, info, trace, warn};
use serde::Deserialize;
use serde_json::Value;
use strum::{Display, EnumIter, IntoEnumIterator};
use thiserror::Error;
use tokio::sync::Semaphore;

pub use error::*;

use crate::util::{self, SensitiveString};

#[derive(EnumIter, Clone, Copy, Debug, PartialEq, Eq, Hash, Display)]
#[strum(serialize_all = "snake_case")]
pub enum CommandApi {
    Conference,
    Participant,
    Platform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]

pub enum Api {
    Command(CommandApi),
    Configuration,
    History,
    Status,
}

impl IntoEnumIterator for Api {
    type Iterator = IntoApiIter;

    fn iter() -> Self::Iterator {
        IntoApiIter {
            items: Box::new([
                Self::Configuration,
                Self::History,
                Self::Status,
                Self::Command(CommandApi::Conference),
                Self::Command(CommandApi::Participant),
                Self::Command(CommandApi::Platform),
            ]),
            current: 0,
        }
    }
}

impl fmt::Display for Api {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(command) => {
                write!(f, "command-{command}")
            }
            _ => {
                write!(f, "{self:?}")
            }
        }
    }
}

pub struct IntoApiIter {
    items: Box<[Api]>,
    current: usize,
}

impl Iterator for IntoApiIter {
    type Item = Api;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.items.len() {
            None
        } else {
            let res = self.items[self.current];
            self.current += 1;
            Some(res)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestType {
    Schema,
    Get,
    Post,
    Update,
    Delete,
}

impl Default for CommandApi {
    fn default() -> Self {
        Self::Conference
    }
}

#[async_trait]
pub trait IApiClient {
    async fn send(&self, request: ApiRequest) -> anyhow::Result<ApiResponse>;
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    http_client: reqwest::Client,
    base_address: String,
    username: String,
    password: SensitiveString,
    semaphore: Arc<Semaphore>,
}

impl ApiClient {
    #[cfg(test)]
    #[must_use]
    pub fn new_for_testing(
        http_client: reqwest::Client,
        mcu_address: String,
        mcu_username: String,
        mcu_password: SensitiveString,
    ) -> Self {
        Self {
            http_client,
            base_address: mcu_address,
            username: mcu_username,
            password: mcu_password,
            semaphore: Arc::new(Semaphore::new(5)),
        }
    }

    #[must_use]
    pub fn new(
        http_client: reqwest::Client,
        mcu_address: &str,
        mcu_username: String,
        mcu_password: SensitiveString,
    ) -> Self {
        let base_address = if mcu_address.starts_with("http://") {
            warn!("Using insecure http protocol!");
            String::from(mcu_address)
        } else if mcu_address.starts_with("https://") {
            String::from(mcu_address)
        } else {
            format!("https://{mcu_address}")
        };

        Self {
            http_client,
            base_address,
            username: mcu_username,
            password: mcu_password,
            semaphore: Arc::new(Semaphore::new(5)), // This limit is fairly arbitrary, but too many requests causes the management node to get bogged down!
        }
    }

    fn get_base_uri_for_api(&self, api: Api) -> String {
        match api {
            Api::Command(command) => {
                format!(
                    "{}/api/admin/command/v1/{}",
                    &self.base_address,
                    &command.to_string(),
                )
            }
            _ => {
                format!(
                    "{}/api/admin/{}/v1",
                    &self.base_address,
                    &api.to_string().to_lowercase()
                )
            }
        }
    }

    fn build_request(&self, request: ApiRequest) -> Result<reqwest::Request, reqwest::Error> {
        match request {
            ApiRequest::Get {
                api,
                resource,
                object_id,
            } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!("{uri}/{resource}/{object_id}/");

                info!("GET {}", &uri);
                self.http_client
                    .get(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .build()
            }
            ApiRequest::GetAll {
                api,
                resource,
                filter_args,
                page_size,
                limit: _,
                offset,
            } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!(
                    "{}/{}/?limit={}&offset={}",
                    &uri, &resource, &page_size, &offset
                );

                info!(
                    "GET_ALL {}  (query parameters are excluded since they may be sensitive)",
                    &uri
                );
                self.http_client
                    .get(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .query(&filter_args)
                    .build()
            }
            ApiRequest::Post {
                api,
                resource,
                args,
            } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!("{}/{}/", &uri, &resource);

                info!("POST {}", &uri);
                self.http_client
                    .post(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .json(&args)
                    .build()
            }
            ApiRequest::Patch {
                api,
                resource,
                object_id,
                args,
            } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!("{uri}/{resource}/{object_id}/");

                info!("PATCH {}", &uri);
                self.http_client
                    .patch(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .json(&args)
                    .build()
            }
            ApiRequest::Delete {
                api,
                resource,
                object_id: resource_id,
            } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!("{}/{}/{}/", &uri, &resource, &resource_id);

                info!("DELETE {}", &uri);
                self.http_client
                    .delete(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .build()
            }
            ApiRequest::ApiSchema { api } => {
                let uri = self.get_base_uri_for_api(api) + "/";
                debug!("API_SCHEMA {}", &uri);
                self.http_client
                    .get(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .build()
            }
            ApiRequest::Schema { api, resource } => {
                let uri = self.get_base_uri_for_api(api);
                let uri = format!("{uri}/{resource}/schema/");
                debug!("SCHEMA {}", &uri);
                self.http_client
                    .get(uri)
                    .basic_auth(&self.username, Some(self.password.secret()))
                    .build()
            }
        }
    }

    async fn handle_api_errors(
        response: reqwest::Result<reqwest::Response>,
    ) -> Result<reqwest::Response, ApiError> {
        match response {
            Err(error) => {
                if let Some(inner) = error.source() {
                    if let Some(inner) = inner.downcast_ref::<hyper::Error>() {
                        Err(ApiError::new(
                            error.status(),
                            format!("error sending request: {inner}"),
                            Some(error.into()),
                        ))
                    } else {
                        Err(ApiError::new(
                            error.status(),
                            "error sending request",
                            Some(error.into()),
                        ))
                    }
                } else {
                    Err(ApiError::new(
                        error.status(),
                        "error sending request",
                        Some(error.into()),
                    ))
                }
            }
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    Ok(response)
                } else {
                    let contents = response.text().await;

                    #[allow(clippy::option_if_let_else)]
                    let error_message = match contents {
                        Ok(contents) if !contents.is_empty() => {
                            if let Ok(json_error) = serde_json::from_str::<JsonError>(&contents) {
                                json_error.error
                            } else if let Ok(json_error) = serde_json::from_str::<Value>(&contents)
                            {
                                if let Ok(mut result) = serde_json::to_string_pretty(&json_error) {
                                    result.insert(0, '\n');
                                    result
                                } else {
                                    contents
                                }
                            } else {
                                contents
                            }
                        }
                        _ => format!("response code \"{status}\" did not indicate success"),
                    };
                    Err(ApiError::new(
                        Some(status),
                        format!("http error: {error_message}"),
                        None,
                    ))
                }
            }
        }
    }

    fn streamed_response(
        self,
        api_request: ApiRequest,
    ) -> impl Stream<Item = Result<Value, ApiClientError>> + Send {
        try_stream! {
            let client = self;
            if let ApiRequest::GetAll {
                mut limit,
                ..
            } = api_request {
                if limit == 0 {
                    limit = usize::MAX;
                }
                let mut request = client.build_request(api_request.clone())?;

                loop {
                    let _hold = client.semaphore.acquire().await.expect("semaphore should never be closed");
                    let response = Self::handle_api_errors(client.http_client.execute(request).await).await?;
                    let response_code = response.status();

                    let response_text = response.text().await?;
                    let api_response: GetApiResponse = match serde_json::from_str(&response_text) {
                        Ok(json) => json,
                        Err(e) => {
                            Err(ApiClientError::ApiError(error::ApiError::new(
                                Some(response_code),
                                format!(
                                    "failed to parse API response to JSON ({}):\n\n{}",
                                    e, &response_text
                                ),
                                Some(e.into()),
                            )))?
                        }
                    };

                    for obj in api_response.objects {
                        yield obj;
                        limit -= 1;
                        if limit == 0 {
                            break;
                        }
                    }
                    if limit == 0 {
                        break;
                    }

                    if let Some(uri) = api_response.meta.next {
                        request = client.http_client
                                .get(format!("{}{}", client.base_address, uri))
                                .basic_auth(&client.username, Some(client.password.secret()))
                                .build()?;
                    } else {
                        break;
                    }
                }
            } else {
                panic!("Request was not GetAll - response cannot be streamed!");
            }
        }
        .boxed()
    }
}

#[derive(Error)]
pub enum ApiClientError {
    #[error(transparent)]
    Web(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    ApiError(#[from] ApiError),
}

impl std::fmt::Debug for ApiClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Web(e) => std::fmt::Debug::fmt(&e, f),
            Self::Json(e) => std::fmt::Debug::fmt(&e, f),
            Self::ApiError(e) => std::fmt::Debug::fmt(&e, f),
        }
    }
}

#[derive(Deserialize, Debug)]
struct JsonError {
    error: String,
}

#[async_trait]
#[allow(clippy::no_effect_underscore_binding)]
impl IApiClient for ApiClient {
    async fn send(&self, request: ApiRequest) -> anyhow::Result<ApiResponse> {
        let is_command = matches!(
            request,
            ApiRequest::Post {
                api: Api::Command(_),
                ..
            }
        );
        if let r @ ApiRequest::GetAll { .. } = request {
            let stream_client = self.clone();
            Ok(ApiResponse::ContentStream(util::StreamWrapper::new(
                Box::pin(stream_client.streamed_response(r)),
            )))
        } else {
            let request = self.build_request(request).map_err(|e| {
                ApiError::new(
                    e.status(),
                    format!("error building request: {e}"),
                    Some(e.into()),
                )
            })?;
            let method = request.method().clone();
            let url = request.url().clone();

            let _hold = self
                .semaphore
                .acquire()
                .await
                .expect("semaphore should never be closed");
            trace!("--> {} {}", method, url);
            let response = Self::handle_api_errors(self.http_client.execute(request).await).await?;
            let response_code = response.status();

            let location = response.headers().get("Location").cloned();

            let response_text = response.text().await?;
            trace!("<-- {} {}", method, url);
            if !response_text.is_empty() {
                if is_command {
                    Ok(ApiResponse::Nothing)
                } else {
                    Ok(ApiResponse::Content({
                        match serde_json::from_str(&response_text) {
                            Ok(json) => json,
                            Err(e) => {
                                return Err(error::ApiError::new(
                                    Some(response_code),
                                    format!(
                                        "failed to parse API response to JSON ({}):\n\n{}",
                                        e, &response_text
                                    ),
                                    Some(e.into()),
                                )
                                .into());
                            }
                        }
                    }))
                }
            } else if let Some(location) = location {
                location
                    .to_str()
                    .map_or(Ok(ApiResponse::Nothing), |location| {
                        Ok(ApiResponse::Location(String::from(location)))
                    })
            } else {
                Ok(ApiResponse::Nothing)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ApiRequest {
    ApiSchema {
        api: Api,
    },
    Schema {
        api: Api,
        resource: String,
    },
    Get {
        api: Api,
        resource: String,
        object_id: String,
    },
    GetAll {
        api: Api,
        resource: String,
        filter_args: HashMap<String, String>,
        page_size: usize,
        limit: usize,
        offset: usize,
    },
    Post {
        api: Api,
        resource: String,
        args: serde_json::Value,
    },
    Patch {
        api: Api,
        resource: String,
        object_id: String,
        args: serde_json::Value,
    },
    Delete {
        api: Api,
        resource: String,
        object_id: String,
    },
}

impl ApiRequest {
    #[must_use]
    pub fn with_offset(&self, offset: usize) -> Option<Self> {
        if let Self::GetAll {
            api,
            resource,
            filter_args,
            page_size,
            limit,
            offset: _,
        } = self
        {
            Some(Self::GetAll {
                api: *api,
                resource: resource.clone(),
                filter_args: filter_args.clone(),
                page_size: *page_size,
                limit: *limit,
                offset,
            })
        } else {
            None
        }
    }
}

pub enum ApiResponse {
    Nothing,
    Location(String),
    Content(Value),
    ContentStream(util::StreamWrapper<Result<Value, ApiClientError>>),
}

impl ApiResponse {
    #[must_use]
    pub fn unwrap_content_or_default(self) -> Value {
        if let Self::Content(content) = self {
            content
        } else {
            Value::default()
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Meta {
    limit: usize,
    next: Option<String>,
    offset: usize,
    previous: Option<String>,
    total_count: usize,
}

#[derive(Deserialize, Debug)]
struct GetApiResponse {
    objects: Vec<Value>,
    meta: Meta,
}
