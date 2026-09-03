fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_client(false)
        .build_server(true)
        .extern_path(".google.protobuf.Struct", "::prost_types::Struct")
        .extern_path(".google.protobuf.Value", "::prost_types::Value")
        .extern_path(".google.protobuf.ListValue", "::prost_types::ListValue")
        .extern_path(".google.protobuf.NullValue", "::prost_types::NullValue")
        .compile_protos(
            &["../plugin/src/main/protobuf/pact-plugin.proto"],
            &["../plugin/src/main/protobuf"],
        )?;
    Ok(())
}
