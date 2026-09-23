use crate::avro::schema::{parse_file, parse_str};
use crate::constants::{AVRO_SCHEMA, CONTENT_TYPES_STR, RECORD_NAME, SCHEMA_KEY};
use crate::error::PluginError;
use crate::pact_plugin::pact_plugin_server::PactPlugin;
use crate::pact_plugin::{
    catalogue_entry, Catalogue, CatalogueEntry, CompareContentsRequest, CompareContentsResponse,
    ConfigureInteractionRequest, ConfigureInteractionResponse, GenerateContentRequest,
    GenerateContentResponse, InitPluginRequest, InitPluginResponse, MockServerRequest,
    MockServerResults, PluginConfiguration, ShutdownMockServerRequest, ShutdownMockServerResponse,
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

/// Builds the error returned for the RPCs pact core only calls on a plugin
/// registered as a `Transport` — this plugin registers only as a
/// `ContentMatcher`, so pact core never invokes `method`.
fn not_a_transport_plugin(method: &str) -> Status {
    Status::unimplemented(format!(
        "Method io.pact.plugin.PactPlugin.{method} is unimplemented: this plugin registers only \
         as a ContentMatcher, never as a Transport, so pact core does not call this RPC for it"
    ))
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

fn schema_text(configuration: Option<&PluginConfiguration>) -> Result<String, PluginError> {
    let configuration = configuration.cloned().unwrap_or_default();
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
    let schema = parse_str(&schema_text(request.plugin_configuration.as_ref())?)?;
    crate::compare::build(request, &schema)
}

fn generate(request: &GenerateContentRequest) -> Result<GenerateContentResponse, PluginError> {
    let schema = parse_str(&schema_text(request.plugin_configuration.as_ref())?)?;
    crate::generate::build(request, &schema)
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
        request: Request<GenerateContentRequest>,
    ) -> Result<Response<GenerateContentResponse>, Status> {
        let response = generate(&request.into_inner()).map_err(|error| {
            tracing::error!("Generate content failed: {error}");
            Status::invalid_argument(error.to_string())
        })?;
        Ok(Response::new(response))
    }

    async fn start_mock_server(
        &self,
        _request: Request<StartMockServerRequest>,
    ) -> Result<Response<StartMockServerResponse>, Status> {
        Err(not_a_transport_plugin("StartMockServer"))
    }

    async fn shutdown_mock_server(
        &self,
        _request: Request<ShutdownMockServerRequest>,
    ) -> Result<Response<ShutdownMockServerResponse>, Status> {
        Err(not_a_transport_plugin("ShutdownMockServer"))
    }

    async fn get_mock_server_results(
        &self,
        _request: Request<MockServerRequest>,
    ) -> Result<Response<MockServerResults>, Status> {
        Err(not_a_transport_plugin("GetMockServerResults"))
    }

    async fn prepare_interaction_for_verification(
        &self,
        _request: Request<VerificationPreparationRequest>,
    ) -> Result<Response<VerificationPreparationResponse>, Status> {
        Err(not_a_transport_plugin("PrepareInteractionForVerification"))
    }

    async fn verify_interaction(
        &self,
        _request: Request<VerifyInteractionRequest>,
    ) -> Result<Response<VerifyInteractionResponse>, Status> {
        Err(not_a_transport_plugin("VerifyInteraction"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pact_plugin::pact_plugin_server::PactPlugin;
    use crate::pact_plugin::{Body, Generator};
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
    async fn generate_content_applies_a_random_int_generator() {
        let schema_str =
            r#"{"type":"record","name":"Item","fields":[{"name":"id","type":"long"}]}"#;
        let schema = crate::avro::schema::parse_str(schema_str).unwrap();
        let ctx = crate::avro::schema::SchemaCtx::new(&schema).unwrap();
        let value = apache_avro::types::Value::Record(vec![(
            "id".into(),
            apache_avro::types::Value::Long(1),
        )]);
        let content = crate::avro::codec::encode(&ctx, &schema, value).unwrap();
        let mut generators = HashMap::new();
        generators.insert(
            "$.id".to_string(),
            Generator {
                r#type: "RandomInt".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"min": 42, "max": 42})
                        .as_object()
                        .unwrap(),
                )),
            },
        );

        let schema_text = schema_str.to_string();
        let hash = crate::avro::schema_hash::base16_hash(&schema_text);
        let plugin_configuration = Some(crate::pact_plugin::PluginConfiguration {
            interaction_configuration: Some(crate::proto_json::json_object_to_struct(
                serde_json::json!({"record": "Item", "schemaKey": hash})
                    .as_object()
                    .unwrap(),
            )),
            pact_configuration: Some(crate::proto_json::json_object_to_struct(
                serde_json::json!({hash.clone(): {"avroSchema": schema_text}})
                    .as_object()
                    .unwrap(),
            )),
        });

        let service = PactAvroPluginService;
        let response = service
            .generate_content(Request::new(GenerateContentRequest {
                contents: Some(Body {
                    content_type: "avro/binary;record=Item".to_string(),
                    content: Some(content),
                    content_type_hint: 0,
                }),
                generators,
                plugin_configuration,
                ..Default::default()
            }))
            .await
            .expect("GenerateContent must succeed")
            .into_inner();

        let decode_schema = crate::avro::schema::parse_str(schema_str).unwrap();
        let ctx = crate::avro::schema::SchemaCtx::new(&decode_schema).unwrap();
        let apache_avro::types::Value::Record(fields) = crate::avro::codec::decode(
            &ctx,
            &decode_schema,
            response.contents.unwrap().content.as_deref().unwrap(),
        )
        .unwrap() else {
            panic!("record expected")
        };
        assert_eq!(fields[0].1, apache_avro::types::Value::Long(42));
    }

    fn assert_not_a_transport_plugin_error(err: tonic::Status) {
        assert_eq!(err.code(), tonic::Code::Unimplemented);
        assert!(
            err.message().contains("ContentMatcher"),
            "expected the error to explain this plugin only registers as a ContentMatcher, got: {}",
            err.message()
        );
    }

    #[tokio::test]
    async fn start_mock_server_explains_this_plugin_is_not_a_transport() {
        let service = PactAvroPluginService;
        let err = service
            .start_mock_server(Request::new(StartMockServerRequest::default()))
            .await
            .expect_err("StartMockServer must return an error");
        assert_not_a_transport_plugin_error(err);
    }

    #[tokio::test]
    async fn shutdown_mock_server_explains_this_plugin_is_not_a_transport() {
        let service = PactAvroPluginService;
        let err = service
            .shutdown_mock_server(Request::new(ShutdownMockServerRequest::default()))
            .await
            .expect_err("ShutdownMockServer must return an error");
        assert_not_a_transport_plugin_error(err);
    }

    #[tokio::test]
    async fn get_mock_server_results_explains_this_plugin_is_not_a_transport() {
        let service = PactAvroPluginService;
        let err = service
            .get_mock_server_results(Request::new(MockServerRequest::default()))
            .await
            .expect_err("GetMockServerResults must return an error");
        assert_not_a_transport_plugin_error(err);
    }

    #[tokio::test]
    async fn prepare_interaction_for_verification_explains_this_plugin_is_not_a_transport() {
        let service = PactAvroPluginService;
        let err = service
            .prepare_interaction_for_verification(Request::new(
                VerificationPreparationRequest::default(),
            ))
            .await
            .expect_err("PrepareInteractionForVerification must return an error");
        assert_not_a_transport_plugin_error(err);
    }

    #[tokio::test]
    async fn verify_interaction_explains_this_plugin_is_not_a_transport() {
        let service = PactAvroPluginService;
        let err = service
            .verify_interaction(Request::new(VerifyInteractionRequest::default()))
            .await
            .expect_err("VerifyInteraction must return an error");
        assert_not_a_transport_plugin_error(err);
    }

    #[tokio::test]
    async fn generate_content_reports_a_missing_schema_configuration_clearly() {
        let service = PactAvroPluginService;
        let error = service
            .generate_content(Request::new(GenerateContentRequest {
                contents: Some(Body {
                    content_type: "avro/binary;record=Item".to_string(),
                    content: Some(vec![]),
                    content_type_hint: 0,
                }),
                generators: HashMap::new(),
                plugin_configuration: None,
                ..Default::default()
            }))
            .await
            .expect_err("GenerateContent must fail without a schema configuration");
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "Interaction configuration not found");
    }
}
