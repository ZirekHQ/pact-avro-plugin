import sbt.*
import sbt.librarymanagement.syntax.ExclusionRule

object Dependencies extends DependencyUtils {

  object Versions {
    val assertjCore = "3.27.7"
    val avro = "1.12.1"
    val jupiterInterface = "0.19.0"
    val logback = "1.6.1"
    val pact = "4.7.3"
    val pactDriverCore = "1.0.0-beta.5"
    val pulsar4sVersion = "2.12.0"
    val scalacheck = "1.19.0"
    val scalaLogging = "3.9.6"
    val scalaTest = "3.2.20"
    val slf4jApi = "2.0.18"
  }

  // protobuf Dependencies
  val scalaPB: ModuleID = "com.thesamet.scalapb" %% "scalapb-runtime" % scalapb.compiler.Version.scalapbVersion

  // Compile Dependencies
  val apacheAvro: ModuleID = "org.apache.avro"           % "avro"     % Versions.avro excludeAll ExclusionRule("org.slf4j")
  val auPactMatchers: ModuleID = "au.com.dius.pact.core" % "matchers" % Versions.pact excludeAll (
    ExclusionRule("com.google.guava"),
    ExclusionRule("org.slf4j")
  )
  val logback: ModuleID = "ch.qos.logback"         % "logback-classic" % Versions.logback
  val scalaLogging: ModuleID = "com.typesafe.scala-logging" %% "scala-logging"        % Versions.scalaLogging excludeAll ExclusionRule("org.slf4j")
  val scalaPBRuntime = "com.thesamet.scalapb"               %% "scalapb-runtime-grpc" % scalapb.compiler.Version.scalapbVersion
  val slf4jApi: ModuleID = "org.slf4j"                      %% "slf4j-api"            % Versions.slf4jApi

  // Test dependencies
  val assertJCore: ModuleID = "org.assertj"                     % "assertj-core"      % Versions.assertjCore
  val avroCompiler: ModuleID = "org.apache.avro"                % "avro-compiler"     % Versions.avro excludeAll ExclusionRule("org.slf4j")
  val jUnitInterface: ModuleID = "com.github.sbt.junit"         % "jupiter-interface" % Versions.jupiterInterface
  val pactConsumerJunit: ModuleID = "au.com.dius.pact.consumer" % "junit5"            % Versions.pact
  val pactProviderJunit: ModuleID = "au.com.dius.pact.provider" % "junit5"            % Versions.pact
  val pulsar4sAvro: ModuleID = "com.clever-cloud.pulsar4s"     %% "pulsar4s-avro"     % Versions.pulsar4sVersion excludeAll ExclusionRule("org.slf4j")
  val pulsar4sCore: ModuleID = "com.clever-cloud.pulsar4s"     %% "pulsar4s-core"     % Versions.pulsar4sVersion excludeAll ExclusionRule("org.slf4j")
  val scalacheck: ModuleID = "org.scalacheck"                  %% "scalacheck"        % Versions.scalacheck
  val scalaTest: ModuleID = "org.scalatest"                    %% "scalatest"         % Versions.scalaTest

  // Overrides
  val grpcApi: ModuleID = "io.grpc"   % "grpc-api"   % scalapb.compiler.Version.grpcJavaVersion
  val grpcCore: ModuleID = "io.grpc"  % "grpc-core"  % scalapb.compiler.Version.grpcJavaVersion
  val grpcNetty: ModuleID = "io.grpc" % "grpc-netty" % scalapb.compiler.Version.grpcJavaVersion
}
