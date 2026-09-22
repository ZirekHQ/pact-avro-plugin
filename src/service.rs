use crate::avro::schema::{parse_file, parse_str};
use crate::constants::{AVRO_SCHEMA, CONTENT_TYPES_STR, RECORD_NAME, SCHEMA_KEY};
use crate::error::PluginError;
use crate::pact_plugin::pact_plugin_server::PactPlugin;
use crate::pact_plugin::{
    catalogue_entry, Catalogue, CatalogueEntry, CompareContentsRequest, CompareContentsResponse,
    ConfigureInteractionRequest, ConfigureInteractionResponse, GenerateContentRequest,
    GenerateContentResponse, InitPluginRequest, InitPluginResponse, MockServerRequest,
    MockServerResults, ShutdownMockServerRequest, ShutdownMockServerResponse,
    StartMockServerRequest, StartMockServerResponse, VerificationPreparationRequest,
    VerificationPreparationResponse, VerifyInteractionRequest, VerifyInteractionResponse,
};
use prost_types::{value::Kind, Struct, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tonic::{Request, Response, Status};

fn message(text: impl Into<String>) -> PluginError {
    PluginError::Message(text.into())
}

fn config_string(
    fields: &BTreeMap<String, Value>,
    key: &str,
    error: &str,
) -> Result<String, PluginError> {
    match fields.get(key).and_then(|value| value.kind.as_ref()) {
        Some(Kind::StringValue(text)) if !text.trim().is_empty() => Ok(text.clone()),
        _ => Err(message(error)),
    }
}

fn required<'s>(value: &'s Option<Struct>, error: &str) -> Result<&'s Struct, PluginError> {
    value.as_ref().ok_or_else(|| message(error))
}

fn configure(
    request: &ConfigureInteractionRequest,
) -> Result<ConfigureInteractionResponse, PluginError> {
    let config = required(&request.contents_config, "Configuration not found")?;
    let path = config_string(
        &config.fields,
        "pact:avro",
        "Config item with key 'pact:avro' and path to the avro schema file is required",
    )?;
    let schema = parse_file(Path::new(&path))?;
    let record_name_key = format!("pact:{RECORD_NAME}");
    let record_name = config_string(
        &config.fields,
        &record_name_key,
        &format!(
            "Config item with key '{record_name_key}' and {RECORD_NAME} of the payload is required"
        ),
    )?;
    crate::interaction::build(config, &schema, &record_name)
}

fn schema_text(request: &CompareContentsRequest) -> Result<String, PluginError> {
    let configuration = request.plugin_configuration.clone().unwrap_or_default();
    let interaction = required(
        &configuration.interaction_configuration,
        "Interaction configuration not found",
    )?;
    let key = config_string(
        &interaction.fields,
        SCHEMA_KEY,
        &format!("Plugin configuration item with key '{SCHEMA_KEY}' is required"),
    )?;
    let pact = required(
        &configuration.pact_configuration,
        "Pact configuration not found",
    )?;
    let entry = pact.fields.get(&key).and_then(|value| match &value.kind {
        Some(Kind::StructValue(inner)) => Some(inner),
        _ => None,
    });
    let inner = entry.ok_or_else(|| {
        message(format!(
            "Plugin Avro Schema configuration item with key '{key}' is required"
        ))
    })?;
    config_string(
        &inner.fields,
        AVRO_SCHEMA,
        &format!("Avro Schema configuration item with key '{AVRO_SCHEMA}' is required"),
    )
}

fn compare(request: &CompareContentsRequest) -> Result<CompareContentsResponse, PluginError> {
    let schema = parse_str(&schema_text(request)?)?;
    crate::compare::build(request, &schema)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PactAvroPluginService;

#[tonic::async_trait]
impl PactPlugin for PactAvroPluginService {
    async fn init_plugin(
        &self,
        request: Request<InitPluginRequest>,
    ) -> Result<Response<InitPluginResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("Init request from {}/{}", req.implementation, req.version);

        let mut values = HashMap::new();
        values.insert("content-types".to_string(), CONTENT_TYPES_STR.to_string());

        Ok(Response::new(InitPluginResponse {
            catalogue: vec![CatalogueEntry {
                r#type: catalogue_entry::EntryType::ContentMatcher as i32,
                key: "avro".to_string(),
                values,
            }],
        }))
    }

    async fn update_catalogue(&self, _request: Request<Catalogue>) -> Result<Response<()>, Status> {
        tracing::debug!("Got update catalogue request: TODO");
        Ok(Response::new(()))
    }

    async fn configure_interaction(
        &self,
        request: Request<ConfigureInteractionRequest>,
    ) -> Result<Response<ConfigureInteractionResponse>, Status> {
        let request = request.into_inner();
        tracing::info!(
            "Configure interaction request for content type '{}'",
            request.content_type
        );
        let response = configure(&request).unwrap_or_else(|error| {
            tracing::error!("Configure interaction failed: {error}");
            ConfigureInteractionResponse {
                error: error.to_string(),
                ..Default::default()
            }
        });
        Ok(Response::new(response))
    }

    async fn compare_contents(
        &self,
        request: Request<CompareContentsRequest>,
    ) -> Result<Response<CompareContentsResponse>, Status> {
        let response = compare(&request.into_inner()).unwrap_or_else(|error| {
            tracing::error!("Compare contents failed: {error}");
            CompareContentsResponse {
                error: error.to_string(),
                ..Default::default()
            }
        });
        Ok(Response::new(response))
    }

    async fn generate_content(
        &self,
        _request: Request<GenerateContentRequest>,
    ) -> Result<Response<GenerateContentResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.GenerateContent is unimplemented",
        ))
    }

    async fn start_mock_server(
        &self,
        _request: Request<StartMockServerRequest>,
    ) -> Result<Response<StartMockServerResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.StartMockServer is unimplemented",
        ))
    }

    async fn shutdown_mock_server(
        &self,
        _request: Request<ShutdownMockServerRequest>,
    ) -> Result<Response<ShutdownMockServerResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.ShutdownMockServer is unimplemented",
        ))
    }

    async fn get_mock_server_results(
        &self,
        _request: Request<MockServerRequest>,
    ) -> Result<Response<MockServerResults>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.GetMockServerResults is unimplemented",
        ))
    }

    async fn prepare_interaction_for_verification(
        &self,
        _request: Request<VerificationPreparationRequest>,
    ) -> Result<Response<VerificationPreparationResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.PrepareInteractionForVerification is unimplemented",
        ))
    }

    async fn verify_interaction(
        &self,
        _request: Request<VerifyInteractionRequest>,
    ) -> Result<Response<VerifyInteractionResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.VerifyInteraction is unimplemented",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pact_plugin::pact_plugin_server::PactPlugin;
    use tonic::Request;

    #[tokio::test]
    async fn init_plugin_returns_the_avro_content_matcher_catalogue_entry() {
        let service = PactAvroPluginService;
        let response = service
            .init_plugin(Request::new(InitPluginRequest {
                implementation: "pact-jvm".to_string(),
                version: "4.6.0".to_string(),
            }))
            .await
            .expect("init_plugin must succeed")
            .into_inner();

        assert_eq!(response.catalogue.len(), 1);
        let entry = &response.catalogue[0];
        assert_eq!(entry.key, "avro");
        assert_eq!(
            entry.r#type,
            catalogue_entry::EntryType::ContentMatcher as i32
        );
        assert_eq!(
            entry.values.get("content-types").map(String::as_str),
            Some("application/avro;avro/bytes;avro/binary;application/*+avro")
        );
    }

    #[tokio::test]
    async fn generate_content_is_unimplemented() {
        let service = PactAvroPluginService;
        let err = service
            .generate_content(Request::new(GenerateContentRequest {
                contents: None,
                generators: Default::default(),
                plugin_configuration: None,
                ..Default::default()
            }))
            .await
            .expect_err("GenerateContent must return an error");
        assert_eq!(err.code(), tonic::Code::Unimplemented);
    }
}
