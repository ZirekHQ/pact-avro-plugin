use pact_avro_plugin::avro::schema::parse_file;
use pact_avro_plugin::avro::schema::parse_str;
use pact_avro_plugin::interaction::build;
use pact_avro_plugin::proto_json::json_object_to_struct;
use serde_json::json;
use std::path::Path;

const AVRO_DIR: &str = "../examples/consumer/src/main/resources/avro";

fn record_schema(uuid_type: &str, timestamp_type: &str) -> String {
    format!(
        r#"{{"type":"record","name":"Event","namespace":"x","fields":[
            {{"name":"id","type":{uuid_type}}},
            {{"name":"at","type":{timestamp_type}}}
        ]}}"#
    )
}

fn body_of(schema_text: &str) -> Vec<u8> {
    let schema = parse_str(schema_text).unwrap();
    let config = json!({
        "id": "matching(type, '6f1b3c2e-0000-4000-8000-000000000001')",
        "at": "matching(integer, 1700000000000000)"
    });
    let response = build(
        &json_object_to_struct(config.as_object().unwrap()),
        &schema,
        "Event",
    )
    .unwrap();
    response.interaction[0]
        .contents
        .as_ref()
        .and_then(|body| body.content.clone())
        .unwrap()
}

#[test]
fn example_schemas_with_logical_types_parse() {
    ["orders.avsc", "order-v1.avsc"].iter().for_each(|name| {
        let path = Path::new(AVRO_DIR).join(name);
        assert!(parse_file(&path).is_ok(), "{name} must parse");
    });
}

#[test]
fn logical_types_produce_the_same_body_as_their_base_types() {
    let logical = record_schema(
        r#"{"type":"string","logicalType":"uuid"}"#,
        r#"{"type":"long","logicalType":"timestamp-micros"}"#,
    );
    let plain = record_schema(r#""string""#, r#""long""#);
    assert_eq!(body_of(&logical), body_of(&plain));
}
