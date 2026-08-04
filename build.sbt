import BuildSettings.*
import Dependencies.*
import PublishSettings.*
import TestEnvironment.*

ThisBuild / scalaVersion := scalaV
//ThisBuild / conflictManager := ConflictManager.strict

lazy val pactOptions: Seq[Tests.Argument] = Seq(
  Some(sys.env.getOrElse("PACT_BROKER_BASE_URL", "http://localhost:9292")).map(s => s"-Dpactbroker.url=$s"),
  sys.env.get("PACT_BROKER_USERNAME").map(s => s"-Dpactbroker.auth.username=$s"),
  sys.env.get("PACT_BROKER_PASSWORD").map(s => s"-Dpactbroker.auth.password=$s"),
  sys.env.get("PACT_BROKER_TAG").map(s => s"-Dpactbroker.consumerversionselectors.tags=$s"),
).flatten.map(o => Tests.Argument(jupiterTestFramework, o))

lazy val plugin = moduleProject("plugin", "plugin")
  .enablePlugins(
    GitHubPagesPlugin,
    JavaAppPackaging,
    // https://sbt-native-packager.readthedocs.io/en/stable/recipes/longclasspath.html#long-classpaths
    LauncherJarPlugin
  )
  .settings(
    git.useGitDescribe := true,
    name := "plugin",
    maintainer := "aliustek@gmail.com",
    publishSettings,
    testEnvSettings,
    gitHubPagesOrgName := "austek",
    gitHubPagesRepoName := "pact-avro-plugin",
    gitHubPagesSiteDir := (`pact-avro-plugin` / baseDirectory).value / "build" / "site",
    gitHubPagesAcceptedTextExtensions := Set(".css", ".html", ".js", ".svg", ".txt", ".woff", ".woff2", ".xml"),
    Compile / PB.targets := Seq(
      scalapb.gen() -> (Compile / sourceManaged).value / "scalapb"
    ),
    libraryDependencies ++=
      Dependencies.compile(apacheAvro, auPactMatchers, logback, scalaLogging, scalaPBRuntime) ++
        Dependencies.protobuf(scalaPB) ++
        Dependencies.test(scalaTest),
    dependencyOverrides ++= Seq(grpcApi, grpcCore, grpcNetty)
  )
lazy val pluginRef = LocalProject("plugin")

lazy val provider = moduleProject("provider", "examples/provider")
  .enablePlugins(SbtAvro)
  .settings(
    avroVersion := Versions.avro,
    Test / sbt.Keys.test := (Test / sbt.Keys.test).dependsOn(pluginRef / buildTestPluginDir).value,
    Test / envVars := Map("PACT_PLUGIN_DIR" -> ((pluginRef / target).value / "plugin").absolutePath),
    testOptions ++= pactOptions,
    libraryDependencies ++=
      Dependencies.compile(Dependencies.avroCompiler, logback, pulsar4sCore, pulsar4sAvro, scalacheck) ++
        Dependencies.test(assertJCore, jUnitInterface, pactProviderJunit),
    publish / skip := false
  )

lazy val consumer = moduleProject("consumer", "examples/consumer")
  .enablePlugins(SbtAvro)
  .settings(
    avroVersion := Versions.avro,
    Compile / avroSource := (Compile / resourceDirectory).value / "avro",
    Test / sbt.Keys.test := (Test / sbt.Keys.test).dependsOn(pluginRef / buildTestPluginDir).value,
    Test / envVars := Map("PACT_PLUGIN_DIR" -> ((pluginRef / target).value / "plugin").absolutePath),
    libraryDependencies ++=
      Dependencies.compile(Dependencies.avroCompiler, logback, pulsar4sCore, pulsar4sAvro, scalaLogging) ++
        Dependencies.test(assertJCore, jUnitInterface, pactConsumerJunit),
    publish / skip := false
  )

lazy val `pact-avro-plugin` = (project in file("."))
  .aggregate(
    pluginRef,
    consumer,
    provider
  )
  .settings(
    basicSettings,
    publish / skip := false
  )

def moduleProject(name: String, path: String): Project = {
  Project(name, file(s"modules/$path"))
    .enablePlugins(GitVersioning, ScalafmtPlugin)
    .settings(
      basicSettings,
      moduleName := name,
      git.useGitDescribe := true,
      git.gitTagToVersionNumber := { tag: String =>
        if(tag matches "v[0-9].*") {
          Some(tag.drop(1).replaceAll("-[0-9]+-.+", ""))
        }
        else None
      }
    )
}
