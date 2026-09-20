pub mod avro;
pub mod constants;
pub mod error;
pub mod proto_json;
pub mod service;

pub mod pact_plugin {
    tonic::include_proto!("io.pact.plugin");
}
