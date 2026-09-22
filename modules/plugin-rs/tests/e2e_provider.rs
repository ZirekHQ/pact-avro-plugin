//! Verifies the built plugin binary's `CompareContents` RPC against the pact
//! `e2e_consumer.rs` wrote. Rust's `pact_verifier` verifies message pacts
//! only over HTTP (POST {description, providerStates} -> message bytes +
//! metadata headers) — there is no in-process callback path like JVM's
//! `@PactVerifyProvider`, so this starts a throwaway local HTTP server that
//! returns an independently-constructed `Order` value, to genuinely exercise
//! matching rather than replay the bytes the consumer test generated.
#![cfg(test)]

use apache_avro::types::Value as AvroValue;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use pact_avro_plugin::avro::codec::encode;
use pact_avro_plugin::avro::schema::{parse_file, SchemaCtx};
use pact_verifier::callback_executors::HttpRequestProviderStateExecutor;
use pact_verifier::{
    verify_provider_async, FilterInfo, NullRequestFilterExecutor, PactSource, ProviderInfo,
    ProviderTransport, VerificationOptions,
};
use std::path::Path;
use std::sync::Arc;

fn order_bytes() -> Vec<u8> {
    let schema = parse_file(Path::new("tests/fixtures/e2e/orders.avsc")).unwrap();
    let ctx = SchemaCtx::new(&schema).unwrap();
    let record = ctx.find_record("Order").unwrap();

    let value = AvroValue::Record(vec![
        ("id".to_string(), AvroValue::Long(100)),
        ("names".to_string(), AvroValue::String("name-1".to_string())),
        ("enabled".to_string(), AvroValue::Boolean(true)),
        ("height".to_string(), AvroValue::Float(15.8)),
        ("width".to_string(), AvroValue::Double(1.8)),
        (
            "status".to_string(),
            AvroValue::Enum(0, "CREATED".to_string()),
        ),
        (
            "address".to_string(),
            AvroValue::Record(vec![
                ("no".to_string(), AvroValue::Int(121)),
                (
                    "street".to_string(),
                    AvroValue::String("street name".to_string()),
                ),
                (
                    "zipcode".to_string(),
                    AvroValue::Union(1, Box::new(AvroValue::Null)),
                ),
            ]),
        ),
        (
            "items".to_string(),
            AvroValue::Array(vec![
                AvroValue::Record(vec![
                    ("name".to_string(), AvroValue::String("Item-1".to_string())),
                    ("id".to_string(), AvroValue::Long(1)),
                ]),
                AvroValue::Record(vec![
                    ("name".to_string(), AvroValue::String("Item-2".to_string())),
                    ("id".to_string(), AvroValue::Long(2)),
                ]),
            ]),
        ),
        (
            "userId".to_string(),
            AvroValue::Union(
                1,
                Box::new(AvroValue::String(
                    "20bef962-8cbd-4b8c-8337-97ae385ac45d".to_string(),
                )),
            ),
        ),
    ]);

    encode(&ctx, record, value).unwrap()
}

#[tokio::test]
#[ignore]
async fn order_provider_satisfies_the_consumer_pact() {
    let bytes = order_bytes();

    let app = Router::new().route(
        "/",
        post(move || {
            let bytes = bytes.clone();
            async move { ([("content-type", "avro/binary;record=Order")], bytes).into_response() }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let provider_info = ProviderInfo {
        name: "avro-plugin-provider".to_string(),
        transports: vec![ProviderTransport {
            transport: "message".to_string(),
            port: Some(port),
            path: None,
            scheme: Some("http".to_string()),
        }],
        ..ProviderInfo::default()
    };

    let source = vec![PactSource::Dir("tests/e2e/pacts".to_string())];
    let options = VerificationOptions::<NullRequestFilterExecutor>::default();
    let state_executor = Arc::new(HttpRequestProviderStateExecutor::default());

    let result = verify_provider_async(
        provider_info,
        source,
        FilterInfo::None,
        vec![],
        &options,
        None,
        &state_executor,
        None,
    )
    .await
    .expect("verification run must not error");

    eprintln!("{}", result.output.join("\n"));
    assert!(result.result, "provider verification must pass");
}
