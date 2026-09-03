use crate::constants::CONTENT_TYPES_STR;
use crate::pact_plugin::pact_plugin_server::PactPlugin;
use crate::pact_plugin::*;
use std::collections::HashMap;
use tonic::{Request, Response, Status};

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
        _request: Request<ConfigureInteractionRequest>,
    ) -> Result<Response<ConfigureInteractionResponse>, Status> {
        // TODO(Plan 2): port InteractionResponseBuilder/InteractionBuilder.
        Err(Status::unimplemented(
            "ConfigureInteraction not yet ported to Rust — see Plan 2",
        ))
    }

    async fn compare_contents(
        &self,
        _request: Request<CompareContentsRequest>,
    ) -> Result<Response<CompareContentsResponse>, Status> {
        // TODO(Plan 2): port CompareContentsResponseBuilder/AvroContentMatcher.
        Err(Status::unimplemented(
            "CompareContents not yet ported to Rust — see Plan 2",
        ))
    }

    async fn generate_content(
        &self,
        _request: Request<GenerateContentRequest>,
    ) -> Result<Response<GenerateContentResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.GenerateContent is unimplemented",
        ))
    }

    async fn start_mock_server(
        &self,
        _request: Request<StartMockServerRequest>,
    ) -> Result<Response<StartMockServerResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.StartMockServer is unimplemented",
        ))
    }

    async fn shutdown_mock_server(
        &self,
        _request: Request<ShutdownMockServerRequest>,
    ) -> Result<Response<ShutdownMockServerResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.ShutdownMockServer is unimplemented",
        ))
    }

    async fn get_mock_server_results(
        &self,
        _request: Request<MockServerRequest>,
    ) -> Result<Response<MockServerResults>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.GetMockServerResults is unimplemented",
        ))
    }

    async fn prepare_interaction_for_verification(
        &self,
        _request: Request<VerificationPreparationRequest>,
    ) -> Result<Response<VerificationPreparationResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.PrepareInteractionForVerification is unimplemented",
        ))
    }

    async fn verify_interaction(
        &self,
        _request: Request<VerifyInteractionRequest>,
    ) -> Result<Response<VerifyInteractionResponse>, Status> {
        Err(Status::unimplemented(
            "Method io.pact.plugin.PactPlugin.VerifyInteraction is unimplemented",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pact_plugin::pact_plugin_server::PactPlugin;
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
    async fn generate_content_is_unimplemented() {
        let service = PactAvroPluginService;
        let err = service
            .generate_content(Request::new(GenerateContentRequest {
                contents: None,
                generators: Default::default(),
                plugin_configuration: None,
            }))
            .await
            .expect_err("GenerateContent must return an error");
        assert_eq!(err.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn configure_interaction_is_pending_plan_2() {
        let service = PactAvroPluginService;
        let err = service
            .configure_interaction(Request::new(ConfigureInteractionRequest {
                content_type: "avro/binary;record=Test".to_string(),
                contents_config: None,
            }))
            .await
            .expect_err("ConfigureInteraction must return an error until Plan 2 lands");
        assert_eq!(err.code(), tonic::Code::Unimplemented);
    }
}
