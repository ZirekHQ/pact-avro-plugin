mod common;

use common::{graceful_kill, read_handshake, spawn_plugin};
use pact_avro_plugin::pact_plugin::pact_plugin_client::PactPluginClient;
use pact_avro_plugin::pact_plugin::Catalogue;
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::{Request, Status};

fn call_update_catalogue(port: u16, authorization: Option<&str>) -> Result<(), Status> {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let channel = tonic::transport::Endpoint::new(format!("http://127.0.0.1:{port}"))
            .expect("invalid plugin endpoint")
            .connect()
            .await
            .expect("failed to connect to the plugin");
        let auth = authorization.map(|key| {
            MetadataValue::try_from(key).expect("authorization value is not valid metadata")
        });
        let mut client = PactPluginClient::with_interceptor(channel, move |mut req: Request<()>| {
            if let Some(auth) = &auth {
                req.metadata_mut().insert("authorization", auth.clone());
            }
            Ok(req)
        });
        client
            .update_catalogue(Catalogue::default())
            .await
            .map(|_| ())
    })
}

#[test]
fn rejects_call_missing_the_authorization_header() {
    let mut child = spawn_plugin();
    let handshake = read_handshake(&mut child, Duration::from_secs(5));

    let result = call_update_catalogue(handshake.port, None);

    assert_eq!(
        result
            .expect_err("call without an authorization header should be rejected")
            .code(),
        tonic::Code::Unauthenticated
    );
    graceful_kill(&mut child, Duration::from_secs(2));
}

#[test]
fn rejects_call_with_the_wrong_authorization_header() {
    let mut child = spawn_plugin();
    let handshake = read_handshake(&mut child, Duration::from_secs(5));

    let result = call_update_catalogue(handshake.port, Some("not-the-server-key"));

    assert_eq!(
        result
            .expect_err("call with a mismatched server key should be rejected")
            .code(),
        tonic::Code::Unauthenticated
    );
    graceful_kill(&mut child, Duration::from_secs(2));
}

#[test]
fn accepts_call_with_the_correct_authorization_header() {
    let mut child = spawn_plugin();
    let handshake = read_handshake(&mut child, Duration::from_secs(5));

    let result = call_update_catalogue(handshake.port, Some(&handshake.server_key));

    assert!(
        result.is_ok(),
        "call with the correct server key should be accepted"
    );
    graceful_kill(&mut child, Duration::from_secs(2));
}
