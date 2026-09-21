use pact_avro_plugin::avro::codec::decode;
use pact_avro_plugin::avro::schema::{parse_file, SchemaCtx};
use pact_avro_plugin::avro::schema_hash::base16_hash;
use pact_avro_plugin::interaction::build;
use pact_avro_plugin::pact_plugin::MatchingRules;
use pact_avro_plugin::proto_json::{json_object_to_struct, struct_to_json, value_to_json};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(format!("tests/fixtures/golden/{name}.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn rules_json(rules: &HashMap<String, MatchingRules>) -> Value {
    let entries: Map<String, Value> = rules
        .iter()
        .map(|(path, group)| {
            let items = group
                .rule
                .iter()
                .map(|rule| {
                    let values = rule.values.as_ref().map_or(json!({}), struct_to_json);
                    json!({"type": rule.r#type, "values": values})
                })
                .collect::<Vec<_>>();
            (path.clone(), Value::Array(items))
        })
        .collect();
    Value::Object(entries)
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}

fn check(name: &str) {
    let fixture = load(name);
    let schema = parse_file(Path::new(&format!(
        "tests/fixtures/{}",
        fixture["schema"].as_str().unwrap()
    )))
    .unwrap();
    let record_name = fixture["record"].as_str().unwrap();
    let config = json_object_to_struct(fixture["config"].as_object().unwrap());
    let response = build(&config, &schema, record_name).unwrap();
    let interaction = &response.interaction[0];
    let body = interaction.contents.as_ref().unwrap();
    assert_eq!(body.content_type, fixture["contentType"].as_str().unwrap());
    let fixture_bytes = unhex(fixture["bodyHex"].as_str().unwrap());
    let rust_bytes = body.content.as_deref().unwrap();
    assert_eq!(rust_bytes.len(), fixture_bytes.len(), "{name}: body length");
    let ctx = SchemaCtx::new(&schema).unwrap();
    let record = ctx.find_record(record_name).unwrap();
    assert_eq!(
        decode(&ctx, record, rust_bytes).unwrap(),
        decode(&ctx, record, &fixture_bytes).unwrap(),
        "{name}: decoded body"
    );
    assert_eq!(
        rules_json(&interaction.rules),
        fixture["rules"],
        "{name}: rules"
    );
}

#[test]
fn item_matches_the_golden_fixture() {
    check("item");
}

#[test]
fn complex_matches_the_golden_fixture() {
    check("complex");
}

#[test]
fn schema_key_is_the_md5_of_the_emitted_schema_text() {
    let fixture = load("item");
    let schema = parse_file(Path::new("tests/fixtures/item.avsc")).unwrap();
    let config = json_object_to_struct(fixture["config"].as_object().unwrap());
    let response = build(&config, &schema, "Item").unwrap();
    let key = response.interaction[0]
        .plugin_configuration
        .as_ref()
        .and_then(|c| c.interaction_configuration.as_ref())
        .map(|s| value_to_json(&s.fields["schemaKey"]))
        .unwrap();
    assert_eq!(
        key.as_str().unwrap(),
        base16_hash(&serde_json::to_string(&schema).unwrap())
    );
}
