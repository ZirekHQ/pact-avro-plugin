use pact_avro_plugin::constants::AVRO_SCHEMA;
use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPlugin;
use pact_avro_plugin::pact_plugin::*;
use pact_avro_plugin::proto_json::json_object_to_struct;
use pact_avro_plugin::service::PactAvroPluginService;
use serde_json::{json, Value};
use tonic::Request;

fn fixture(name: &str) -> String {
    std::path::absolute(format!("tests/fixtures/{name}"))
        .unwrap()
        .display()
        .to_string()
}

fn configure_request(config: Value) -> Request<ConfigureInteractionRequest> {
    Request::new(ConfigureInteractionRequest {
        content_type: "avro/binary".to_string(),
        contents_config: Some(json_object_to_struct(config.as_object().unwrap())),
    })
}

async fn configure_error(config: Value) -> String {
    let response = PactAvroPluginService
        .configure_interaction(configure_request(config))
        .await;
    response.unwrap().into_inner().error
}

#[tokio::test]
async fn configure_requires_configuration() {
    let response = PactAvroPluginService
        .configure_interaction(Request::new(ConfigureInteractionRequest::default()))
        .await
        .unwrap();
    assert_eq!(response.into_inner().error, "Configuration not found");
}

#[tokio::test]
async fn configure_requires_the_schema_file_path() {
    let error = configure_error(json!({"pact:record-name": "Item"})).await;
    assert_eq!(
        error,
        "Config item with key 'pact:avro' and path to the avro schema file is required"
    );
}

#[tokio::test]
async fn configure_requires_an_existing_schema_file() {
    let error = configure_error(json!({"pact:avro": "non-existing.avsc"})).await;
    assert!(error.starts_with("Failed to parse avro schema from file:"));
    assert!(error.ends_with("non-existing.avsc"));
}

#[tokio::test]
async fn configure_requires_a_valid_schema_file() {
    let error = configure_error(json!({"pact:avro": fixture("invalid.avsc")})).await;
    assert!(error.starts_with("Failed to parse avro schema from file:"));
    assert!(error.ends_with("invalid.avsc"));
}

#[tokio::test]
async fn configure_requires_the_record_name() {
    let error = configure_error(json!({"pact:avro": fixture("item.avsc")})).await;
    assert_eq!(
        error,
        "Config item with key 'pact:record-name' and record-name of the payload is required"
    );
}

#[tokio::test]
async fn configure_builds_a_single_record_interaction() {
    let request = configure_request(json!({
        "pact:avro": fixture("item.avsc"),
        "pact:record-name": "Item",
        "pact:content-type": "avro/binary",
        "name": "notEmpty('Item-41')",
        "id": "notEmpty('41')"
    }));
    let response = PactAvroPluginService
        .configure_interaction(request)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.error, "");
    let interaction = &response.interaction[0];
    assert_eq!(
        interaction.contents.as_ref().unwrap().content_type,
        "avro/binary;record=Item"
    );
    assert_eq!(interaction.rules.len(), 2);
}

fn complex_config() -> Value {
    json!({
        "pact:avro": fixture("schemas.avsc"),
        "pact:record-name": "Complex",
        "pact:content-type": "avro/binary",
        "id": "notEmpty('100')",
        "names": ["notEmpty('name-1')", "notEmpty('name-2')"],
        "enabled": "matching(boolean, true)",
        "no": "matching(integer, 121)",
        "height": "matching(decimal, 15.8)",
        "width": "matching(decimal, 1.8)",
        "ages": {"first": "matching(integer, 2)", "second": "matching(integer, 3)"},
        "color": "matching(equalTo, 'GREEN')",
        "md5": "matching(equalTo, 'abcd')",
        "address": {"street": "notEmpty('street name')"},
        "items": [
            {"name": "notEmpty('Item-1')", "id": "matching(integer, 1)"},
            {"name": "notEmpty('Item-2')", "id": "matching(integer, 2)"}
        ]
    })
}

#[tokio::test]
async fn configure_builds_a_complex_record_with_eighteen_rule_paths() {
    let response = PactAvroPluginService
        .configure_interaction(configure_request(complex_config()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.error, "");
    assert_eq!(response.interaction[0].rules.len(), 18);
}

#[tokio::test]
async fn compare_requires_interaction_configuration() {
    let response = PactAvroPluginService
        .compare_contents(Request::new(CompareContentsRequest::default()))
        .await
        .unwrap();
    assert_eq!(
        response.into_inner().error,
        "Interaction configuration not found"
    );
}

#[tokio::test]
async fn compare_contents_of_a_configured_interaction_matches() {
    let configured = PactAvroPluginService
        .configure_interaction(configure_request(complex_config()))
        .await
        .unwrap()
        .into_inner();
    let interaction = configured.interaction[0].clone();
    let plugin_configuration = PluginConfiguration {
        interaction_configuration: interaction
            .plugin_configuration
            .unwrap()
            .interaction_configuration,
        pact_configuration: configured.plugin_configuration.unwrap().pact_configuration,
    };
    let body = interaction.contents.unwrap();
    let request = CompareContentsRequest {
        expected: Some(body.clone()),
        actual: Some(body),
        allow_unexpected_keys: false,
        rules: interaction.rules,
        plugin_configuration: Some(plugin_configuration),
    };
    let response = PactAvroPluginService
        .compare_contents(Request::new(request))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.error, "");
    assert!(!response.results.is_empty());
    assert!(response
        .results
        .values()
        .all(|item| item.mismatches.is_empty()));
}

/// pact-jvm merges interactions from multiple `@Pact` methods but keeps only the first
/// pact's `metadata.plugins` (see https://github.com/pact-foundation/pact-jvm/issues/1938),
/// so a pact merged that way arrives with no `pact_configuration` at all.
#[tokio::test]
async fn compare_contents_matches_when_the_pact_level_configuration_is_missing() {
    let configured = PactAvroPluginService
        .configure_interaction(configure_request(complex_config()))
        .await
        .unwrap()
        .into_inner();
    let interaction = configured.interaction[0].clone();
    let plugin_configuration = PluginConfiguration {
        interaction_configuration: interaction
            .plugin_configuration
            .unwrap()
            .interaction_configuration,
        pact_configuration: None,
    };
    let body = interaction.contents.unwrap();
    let request = CompareContentsRequest {
        expected: Some(body.clone()),
        actual: Some(body),
        allow_unexpected_keys: false,
        rules: interaction.rules,
        plugin_configuration: Some(plugin_configuration),
    };
    let response = PactAvroPluginService
        .compare_contents(Request::new(request))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.error, "");
    assert!(!response.results.is_empty());
    assert!(response
        .results
        .values()
        .all(|item| item.mismatches.is_empty()));
}

/// A pact published before this plugin started embedding `avroSchema` on the interaction
/// itself must still verify: the `schemaKey`/`pact_configuration` lookup stays intact.
#[tokio::test]
async fn compare_contents_matches_a_pact_published_under_the_legacy_schema_key_layout() {
    let configured = PactAvroPluginService
        .configure_interaction(configure_request(complex_config()))
        .await
        .unwrap()
        .into_inner();
    let interaction = configured.interaction[0].clone();
    let mut interaction_configuration = interaction
        .plugin_configuration
        .clone()
        .unwrap()
        .interaction_configuration
        .unwrap();
    interaction_configuration.fields.remove(AVRO_SCHEMA);
    let plugin_configuration = PluginConfiguration {
        interaction_configuration: Some(interaction_configuration),
        pact_configuration: configured.plugin_configuration.unwrap().pact_configuration,
    };
    let body = interaction.contents.unwrap();
    let request = CompareContentsRequest {
        expected: Some(body.clone()),
        actual: Some(body),
        allow_unexpected_keys: false,
        rules: interaction.rules,
        plugin_configuration: Some(plugin_configuration),
    };
    let response = PactAvroPluginService
        .compare_contents(Request::new(request))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.error, "");
    assert!(!response.results.is_empty());
    assert!(response
        .results
        .values()
        .all(|item| item.mismatches.is_empty()));
}
