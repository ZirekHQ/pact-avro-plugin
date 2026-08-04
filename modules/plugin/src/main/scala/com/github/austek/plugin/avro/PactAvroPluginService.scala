package com.github.austek.plugin.avro

import com.github.austek.plugin.avro.AvroPactConstants.*
import com.github.austek.plugin.avro.AvroPluginConstants.*
import com.github.austek.plugin.avro.ContentTypeConstants.*
import com.github.austek.plugin.avro.compare.CompareContentsResponseBuilder
import com.github.austek.plugin.avro.error.{PluginError, PluginErrorException, PluginErrorMessage, PluginErrorMessages}
import com.github.austek.plugin.avro.interaction.InteractionResponseBuilder
import com.github.austek.plugin.avro.utils.*
import com.google.protobuf.empty.Empty
import com.google.protobuf.struct.{Struct, Value}
import com.typesafe.scalalogging.StrictLogging
import io.grpc.Status.UNIMPLEMENTED
import io.grpc.StatusException
import io.pact.plugin.pact_plugin.PactPluginGrpc.PactPlugin
import io.pact.plugin.pact_plugin.*
import org.apache.avro.Schema

import java.nio.file.Path
import scala.concurrent.Future

class PactAvroPluginService extends PactPlugin with StrictLogging {

  /** Check that the plugin loaded OK. Returns the catalogue entries describing what the plugin provides
    */
  override def initPlugin(request: InitPluginRequest): Future[InitPluginResponse] = {
    logger.debug(s"Init request from ${request.implementation}/${request.version}")
    Future.successful(
      InitPluginResponse.apply(
        Seq(
          CatalogueEntry(
            CatalogueEntry.EntryType.CONTENT_MATCHER,
            "avro",
            Map("content-types" -> ContentTypesStr)
          )
        )
      )
    )
  }

  /** Updated catalogue. This will be sent when the core catalogue has been updated (probably by a plugin loading).
    */
  override def updateCatalogue(in: Catalogue): Future[Empty] = {
    logger.debug("Got update catalogue request: TODO")
    Future.successful(Empty())
  }

  /** Request to configure/setup the interaction for later verification. Data returned will be persisted in the pact file.
    */
  override def configureInteraction(in: ConfigureInteractionRequest): Future[ConfigureInteractionResponse] = {
    logger.info(s"Configure interaction request for content type '${in.contentType}': $in")

    (for {
      contentsConfig <- getConfiguration(in.contentsConfig, "Configuration not found")
      avroSchema <- getAvroSchema(contentsConfig)
      recordName <- getRecordName(contentsConfig)
      response <- InteractionResponseBuilder.build(contentsConfig, avroSchema, recordName)
    } yield response) match {
      case Right(response) =>
        logger.debug(s"Responding: $response")
        Future.successful(response)
      case Left(PluginErrorMessage(msg)) =>
        logger.error(s"Configure interaction failed: $msg")
        Future.successful(ConfigureInteractionResponse(error = msg))
      case Left(PluginErrorMessages(values)) =>
        values.foreach(v => logger.error(v))
        Future.successful(ConfigureInteractionResponse(error = "Multiple errors detected and logged, please check logs"))
      case Left(PluginErrorException(e)) =>
        logger.error(s"Configure interaction failed", e)
        Future.successful(ConfigureInteractionResponse(error = e.getMessage))
    }
  }

  /** Request to perform a comparison of some contents (matching request)
    */
  override def compareContents(request: CompareContentsRequest): Future[CompareContentsResponse] = {
    logger.debug(s"Got compareContents request $request")

    (for {
      interactionConfig <- getConfiguration(request.getPluginConfiguration.interactionConfiguration, "Interaction configuration not found")
      schemaKey <- getConfigStringValue(interactionConfig.fields, SchemaKey, s"Plugin configuration item with key '$SchemaKey' is required")
      pactConfiguration <- getConfiguration(request.getPluginConfiguration.pactConfiguration, "Pact configuration not found")
      avroSchemaConfig <- getConfigValue(pactConfiguration.fields, schemaKey, s"Plugin Avro Schema configuration item with key '$schemaKey' is required")
      avroSchemaString <-
        getConfigStringValue(avroSchemaConfig.getStructValue.fields, AvroSchema, s"Avro Schema configuration item with key '$AvroSchema' is required")
      avroSchema <- AvroUtils.parseSchema(avroSchemaString)
      response <- CompareContentsResponseBuilder.build(request, avroSchema)
    } yield response) match {
      case Right(response) =>
        logger.debug(s"Compare contents responding: $response")
        Future.successful(response)
      case Left(PluginErrorMessage(msg)) =>
        logger.error(s"Compare contents failed: $msg")
        Future.successful(CompareContentsResponse(error = msg))
      case Left(PluginErrorMessages(values)) =>
        values.foreach(v => logger.error(v))
        Future.successful(CompareContentsResponse(error = "Multiple errors detected and logged, please check logs"))
      case Left(PluginErrorException(e)) =>
        logger.error(s"Compare contents failed", e)
        Future.successful(CompareContentsResponse(error = e.getMessage))
    }
  }

  /** Request to generate the content using any defined generators
    */
  override def generateContent(in: GenerateContentRequest): Future[GenerateContentResponse] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.GenerateContent is unimplemented"))

  /** Start a mock server
    */
  override def startMockServer(in: StartMockServerRequest): Future[StartMockServerResponse] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.StartMockServer is unimplemented"))

  /** Shutdown a running mock server
    */
  override def shutdownMockServer(in: ShutdownMockServerRequest): Future[ShutdownMockServerResponse] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.ShutdownMockServer is unimplemented"))

  /** Get the matching results from a running mock server
    */
  override def getMockServerResults(in: MockServerRequest): Future[MockServerResults] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.GetMockServerResults is unimplemented"))

  /** Prepare an interaction for verification. This should return any data required to construct any request so that it can be amended before the verification
    * is run
    */
  override def prepareInteractionForVerification(in: VerificationPreparationRequest): Future[VerificationPreparationResponse] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.PrepareInteractionForVerification is unimplemented"))

  /** Execute the verification for the interaction.
    */
  override def verifyInteraction(in: VerifyInteractionRequest): Future[VerifyInteractionResponse] =
    throw new StatusException(UNIMPLEMENTED.withDescription("Method io.pact.plugin.PactPlugin.VerifyInteraction is unimplemented"))

  private def getConfigValue(configs: Map[String, Value], id: String, msg: String): Either[PluginErrorMessage, Value] =
    configs.get(id) match {
      case Some(value) => Right(value)
      case None        => Left(PluginErrorMessage(msg))
    }

  private def getConfigStringValue(configs: Map[String, Value], id: String, msg: String): Either[PluginErrorMessage, String] =
    getConfigValue(configs, id, msg).map(_.getStringValue).filterOrElse(!_.isBlank, PluginErrorMessage(msg))

  // noinspection SameParameterValue
  private def getConfiguration(structOpt: Option[Struct], errorMsg: String): Either[PluginErrorMessage, Struct] =
    structOpt match {
      case Some(struct) => Right(struct)
      case None         => Left(PluginErrorMessage(errorMsg))
    }

  private def getAvroSchema(configuration: Struct): Either[PluginError[?], Schema] = {
    getConfigStringValue(
      configuration.fields,
      "pact:avro",
      "Config item with key 'pact:avro' and path to the avro schema file is required"
    ).flatMap { avroFilePath =>
      AvroUtils.parseSchema(Path.of(avroFilePath).toFile)
    }
  }

  private def getRecordName(configuration: Struct): Either[PluginErrorMessage, String] =
    getConfigStringValue(
      configuration.fields,
      s"pact:$RecordName",
      s"Config item with key 'pact:$RecordName' and $RecordName of the payload is required"
    )
}
