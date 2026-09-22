//! Exercises the built plugin binary end to end: `pact_consumer` loads the
//! installed "avro" plugin, calls its real `ConfigureInteraction` RPC over
//! gRPC to build each message, and the assertions below decode the bytes it
//! actually generated. Requires the release binary installed at
//! `$PACT_PLUGIN_DIR` first — see `scripts/pluginLocalInstall.sh`.
#![cfg(test)]

use apache_avro::types::Value as AvroValue;
use pact_avro_plugin::avro::codec::decode;
use pact_avro_plugin::avro::schema::{parse_file, SchemaCtx};
use pact_consumer::prelude::*;
use pact_models::matchingrules::Category;
use pact_models::path_exp::DocPath;
use serde_json::json;
use std::path::Path;

fn fixture_path(name: &str) -> String {
    std::fs::canonicalize(format!("tests/fixtures/e2e/{name}"))
        .unwrap_or_else(|e| panic!("fixture '{name}' not found: {e}"))
        .to_string_lossy()
        .to_string()
}

fn record_field<'a>(fields: &'a [(String, AvroValue)], name: &str) -> &'a AvroValue {
    fields
        .iter()
        .find(|(field_name, _)| field_name == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing field '{name}'"))
}

// pact_consumer merges into an existing pact file by default instead of
// replacing it, so a stale file from an earlier run (e.g. a renamed or
// removed interaction) would otherwise keep getting verified forever.
fn reset_pact_file(consumer: &str, provider: &str) {
    let path = Path::new("tests/e2e/pacts").join(format!("{consumer}-{provider}.json"));
    if let Err(err) = std::fs::remove_file(&path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            panic!("failed to reset stale pact file {}: {err}", path.display());
        }
    }
}

#[tokio::test]
#[ignore]
async fn order_created_message_matches_the_built_plugin() {
    reset_pact_file("avro-plugin-consumer", "avro-plugin-provider");

    // tag::configuration[]
    let mut builder = PactBuilder::new_v4("avro-plugin-consumer", "avro-plugin-provider")
        .using_plugin("avro", None)
        .await;
    builder.output_dir("tests/e2e/pacts");

    builder
        .message_interaction("Order Created", |mut i| async move {
            i.contents_from(json!({
                "pact:content-type": "avro/binary",
                "pact:avro": fixture_path("orders.avsc"),
                "pact:record-name": "Order",
                "id": "notEmpty('100')",
                "names": "notEmpty('name-1')",
                "enabled": "matching(boolean, true)",
                "height": "matching(decimal, 15.8)",
                "width": "matching(decimal, 1.8)",
                "status": "matching(equalTo, 'CREATED')",
                "address": {
                    "no": "matching(integer, 121)",
                    "street": "matching(equalTo, 'street name')"
                },
                "items": [
                    { "name": "notEmpty('Item-1')", "id": "notEmpty('1')" },
                    { "name": "notEmpty('Item-2')", "id": "notEmpty('2')" }
                ],
                "userId": "notEmpty('20bef962-8cbd-4b8c-8337-97ae385ac45d')"
            }))
            .await;
            i
        })
        .await;
    // end::configuration[]

    // tag::consumer_test[]
    let messages: Vec<_> = builder.messages().collect();
    assert_eq!(messages.len(), 1, "expected exactly one generated message");
    for message in messages {
        assert_eq!(
            message
                .contents
                .contents
                .content_type()
                .expect("plugin must set a content type")
                .to_string(),
            "avro/binary;record=Order"
        );

        let schema = parse_file(Path::new("tests/fixtures/e2e/orders.avsc")).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let record = ctx.find_record("Order").unwrap();
        let bytes = message
            .contents
            .contents
            .value()
            .expect("plugin must generate a body");
        let decoded = decode(&ctx, record, &bytes).unwrap();

        let fields = match decoded {
            AvroValue::Record(fields) => fields,
            other => panic!("expected a Record, got {other:?}"),
        };
        assert_eq!(*record_field(&fields, "id"), AvroValue::Long(100));
        assert_eq!(
            *record_field(&fields, "names"),
            AvroValue::String("name-1".to_string())
        );
        assert_eq!(*record_field(&fields, "enabled"), AvroValue::Boolean(true));
        assert_eq!(
            *record_field(&fields, "status"),
            AvroValue::Enum(0, "CREATED".to_string())
        );

        let body_rules = message
            .contents
            .matching_rules
            .rules_for_category(Category::BODY)
            .expect("body matching rules must be present");
        for path in ["id", "names", "enabled", "height", "width", "userId"] {
            assert!(
                body_rules.rules.contains_key(&DocPath::root().join(path)),
                "expected a matching rule for $.{path}"
            );
        }
    }
    // end::consumer_test[]
}

#[tokio::test]
#[ignore]
async fn order_new_event_message_matches_the_built_plugin() {
    reset_pact_file("OrderTopicConsumer", "OrderTopicV1");

    let mut builder = PactBuilder::new_v4("OrderTopicConsumer", "OrderTopicV1")
        .using_plugin("avro", None)
        .await;
    builder.output_dir("tests/e2e/pacts");

    builder
        .message_interaction("Order Created", |mut i| async move {
            i.contents_from(json!({
                "pact:content-type": "avro/binary",
                "pact:avro": fixture_path("order-v1.avsc"),
                "pact:record-name": "OrderNewEvent",
                "orderId": "notEmpty('0c7cbb5a-9a9a-4088-9713-c0c88475c903')",
                "userId": "notEmpty('20bef962-8cbd-4b8c-8337-97ae385ac45d')",
                "items": [
                    {
                        "itemId": "notEmpty('e41c5f30-fa8e-4cfd-989d-95ca5a04037f')",
                        "quantity": "notEmpty('1')"
                    },
                    {
                        "itemId": "notEmpty('8a62474a-7157-4c67-9126-c6dcecb1df08')",
                        "quantity": "notEmpty('2')"
                    }
                ]
            }))
            .await;
            i
        })
        .await;

    let messages: Vec<_> = builder.messages().collect();
    assert_eq!(messages.len(), 1, "expected exactly one generated message");
    for message in messages {
        assert_eq!(
            message
                .contents
                .contents
                .content_type()
                .expect("plugin must set a content type")
                .to_string(),
            "avro/binary;record=OrderNewEvent"
        );

        let schema = parse_file(Path::new("tests/fixtures/e2e/order-v1.avsc")).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let record = ctx.find_record("OrderNewEvent").unwrap();
        let bytes = message
            .contents
            .contents
            .value()
            .expect("plugin must generate a body");
        let decoded = decode(&ctx, record, &bytes).unwrap();

        let fields = match decoded {
            AvroValue::Record(fields) => fields,
            other => panic!("expected a Record, got {other:?}"),
        };
        assert_eq!(
            *record_field(&fields, "orderId"),
            AvroValue::Union(
                1,
                Box::new(AvroValue::String(
                    "0c7cbb5a-9a9a-4088-9713-c0c88475c903".to_string()
                ))
            )
        );
    }
}
