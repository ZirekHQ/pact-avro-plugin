use crate::avro::codec::encode;
use crate::avro::node::{to_value, Node};
use crate::avro::record::RecordBuilder;
use crate::avro::rules::{generator_to_proto, rule_to_proto};
use crate::avro::schema::SchemaCtx;
use crate::avro::schema_hash::base16_hash;
use crate::constants::{AVRO_SCHEMA, RECORD, SCHEMA_KEY};
use crate::error::PluginError;
use crate::pact_plugin::{
    body::ContentTypeHint, Body, ConfigureInteractionResponse, InteractionResponse, MatchingRules,
    PluginConfiguration,
};
use crate::proto_json::text_value;
use apache_avro::schema::Schema;
use prost_types::{value::Kind, Struct, Value};
use std::collections::HashMap;

fn single(error: PluginError) -> Vec<PluginError> {
    vec![error]
}

fn texts(error: PluginError) -> Vec<String> {
    match error {
        PluginError::Messages(inner) => inner,
        other => vec![other.to_string()],
    }
}

fn collapse(errors: Vec<PluginError>) -> PluginError {
    let all: Vec<String> = errors.into_iter().flat_map(texts).collect();
    all.iter().for_each(|text| tracing::error!("{text}"));
    PluginError::Messages(all)
}

fn struct_of(entries: Vec<(&str, Value)>) -> Struct {
    Struct {
        fields: entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    }
}

fn pact_configuration(hash: &str, schema_text: &str) -> PluginConfiguration {
    let inner = Value {
        kind: Some(Kind::StructValue(struct_of(vec![(
            AVRO_SCHEMA,
            text_value(schema_text),
        )]))),
    };
    PluginConfiguration {
        interaction_configuration: None,
        pact_configuration: Some(struct_of(vec![(hash, inner)])),
    }
}

fn rules_of(node: &Node) -> HashMap<String, MatchingRules> {
    node.rules_by_path()
        .into_iter()
        .map(|(path, rules)| {
            let rule = rules.iter().map(rule_to_proto).collect();
            (path, MatchingRules { rule })
        })
        .collect()
}

fn generators_of(node: &Node) -> HashMap<String, crate::pact_plugin::Generator> {
    node.generators_by_path()
        .into_iter()
        .filter_map(|(path, generator)| generator.map(|g| (path, generator_to_proto(&g))))
        .collect()
}

fn interaction_response(
    record_name: &str,
    hash: &str,
    node: &Node,
    body: Vec<u8>,
) -> InteractionResponse {
    InteractionResponse {
        contents: Some(Body {
            content_type: format!("avro/binary;record={record_name}"),
            content: Some(body),
            content_type_hint: ContentTypeHint::Binary as i32,
        }),
        rules: rules_of(node),
        generators: generators_of(node),
        plugin_configuration: Some(PluginConfiguration {
            interaction_configuration: Some(struct_of(vec![
                (RECORD, text_value(record_name)),
                (SCHEMA_KEY, text_value(hash)),
            ])),
            pact_configuration: None,
        }),
        ..Default::default()
    }
}

fn build_interaction(
    schema: &Schema,
    record_name: &str,
    hash: &str,
    config: &Struct,
) -> Result<InteractionResponse, Vec<PluginError>> {
    let ctx = SchemaCtx::new(schema).map_err(single)?;
    let record = ctx.find_record(record_name).map_err(single)?;
    let node = RecordBuilder::new(&ctx).build(record, &config.fields)?;
    let value = to_value(&ctx, record, &node).map_err(single)?;
    let body = encode(&ctx, record, value).map_err(single)?;
    Ok(interaction_response(record_name, hash, &node, body))
}

pub fn build(
    config: &Struct,
    schema: &Schema,
    record_name: &str,
) -> Result<ConfigureInteractionResponse, PluginError> {
    let schema_text =
        serde_json::to_string(schema).map_err(|error| PluginError::Exception(error.to_string()))?;
    let hash = base16_hash(&schema_text);
    build_interaction(schema, record_name, &hash, config)
        .map(|interaction| ConfigureInteractionResponse {
            error: String::new(),
            interaction: vec![interaction],
            plugin_configuration: Some(pact_configuration(&hash, &schema_text)),
        })
        .map_err(collapse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_file;
    use crate::proto_json::{json_object_to_struct, value_to_json};
    use serde_json::json;
    use std::path::Path;

    fn config(value: serde_json::Value) -> Struct {
        json_object_to_struct(value.as_object().unwrap())
    }

    fn item_schema() -> Schema {
        parse_file(Path::new("tests/fixtures/item.avsc")).unwrap()
    }

    #[test]
    fn builds_body_rules_and_plugin_configuration_for_a_record() {
        let response = build(
            &config(json!({"name": "notEmpty('Item-41')", "id": "notEmpty('41')"})),
            &item_schema(),
            "Item",
        )
        .unwrap();
        let interaction = &response.interaction[0];
        let body = interaction.contents.as_ref().unwrap();
        assert_eq!(body.content_type, "avro/binary;record=Item");
        assert_eq!(body.content_type_hint, ContentTypeHint::Binary as i32);
        let mut expected = vec![0x0e];
        expected.extend_from_slice(b"Item-41");
        expected.push(0x52);
        assert_eq!(body.content.as_deref(), Some(expected.as_slice()));
        assert_eq!(interaction.rules.len(), 2);
    }

    #[test]
    fn a_provider_state_expression_populates_the_generators_map() {
        let response = build(
            &config(json!({
                "name": "notEmpty(fromProviderState('exp', 'x'))",
                "id": "notEmpty('41')"
            })),
            &item_schema(),
            "Item",
        )
        .unwrap();
        let interaction = &response.interaction[0];
        assert_eq!(interaction.generators.len(), 1);
        let generator = &interaction.generators["$.name"];
        assert_eq!(generator.r#type, "ProviderState");
    }

    #[test]
    fn a_record_with_no_generators_has_an_empty_generators_map() {
        let response = build(
            &config(json!({"name": "notEmpty('a')", "id": "notEmpty('1')"})),
            &item_schema(),
            "Item",
        )
        .unwrap();
        assert!(response.interaction[0].generators.is_empty());
    }

    #[test]
    fn plugin_configuration_links_the_schema_hash_to_the_schema() {
        let schema = item_schema();
        let response = build(
            &config(json!({"name": "notEmpty('a')", "id": "notEmpty('1')"})),
            &schema,
            "Item",
        )
        .unwrap();
        let interaction_config = response.interaction[0]
            .plugin_configuration
            .as_ref()
            .and_then(|c| c.interaction_configuration.clone())
            .unwrap();
        let hash = value_to_json(&interaction_config.fields["schemaKey"]);
        let pact_config = response
            .plugin_configuration
            .unwrap()
            .pact_configuration
            .unwrap();
        let stored = value_to_json(&pact_config.fields[hash.as_str().unwrap()]);
        assert_eq!(
            stored["avroSchema"],
            json!(serde_json::to_string(&schema).unwrap())
        );
        assert_eq!(
            value_to_json(&interaction_config.fields["record"]),
            json!("Item")
        );
    }

    #[test]
    fn every_failure_collapses_into_the_multiple_errors_message() {
        let error = build(&config(json!({})), &item_schema(), "Nope").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Multiple errors detected and logged, please check logs"
        );
    }

    #[test]
    fn configuration_errors_collapse_too() {
        let error = build(&config(json!({})), &item_schema(), "Item").unwrap_err();
        assert!(matches!(error, PluginError::Messages(messages) if messages.len() == 2));
    }

    #[test]
    fn nested_messages_are_flattened_into_the_collapsed_payload() {
        let nested = vec![
            PluginError::Messages(vec!["a".into()]),
            PluginError::Message("b".into()),
        ];
        let PluginError::Messages(texts) = collapse(nested) else {
            panic!("messages expected")
        };
        assert_eq!(texts, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn reference_rules_surface_their_unsupported_text_in_the_payload() {
        let error = build(
            &config(json!({"name": "matching($'x')", "id": "notEmpty('1')"})),
            &item_schema(),
            "Item",
        )
        .unwrap_err();
        let PluginError::Messages(texts) = error else {
            panic!("messages expected")
        };
        assert_eq!(texts.len(), 1);
        assert!(texts[0].contains("not supported for now"), "{texts:?}");
    }
}
