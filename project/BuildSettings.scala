import org.typelevel.sbt.tpolecat.TpolecatPlugin.autoImport.tpolecatExcludeOptions
import org.typelevel.scalacoptions.ScalacOptions
import sbt.*
import sbt.Keys.*

import java.net.URI
import java.util

object BuildSettings {
  private val javaVersion = 17
  private val env: util.Map[String, String] = System.getenv()
  val scalaV = "3.6.1"

  lazy val basicSettings: Seq[Def.Setting[?]] = Seq(
    homepage := Some(URI.create("https://github.com/austek/pact-avro-plugin").toURL),
    organization := "io.pact",
    description := "Pact Avro Plugin",
    scalaVersion := scalaV,
    resolvers += Resolver.mavenLocal,
    resolvers ++= Resolver.sonatypeOssRepos("releases"),
    javacOptions ++= Seq(
      "-encoding",
      "UTF-8",
      "-source",
      s"$javaVersion",
      "-target",
      s"$javaVersion"
    ),
    Test / fork := true,
    scalacOptions --= {
      if (sys.env.contains("CI"))
        Seq.empty
      else
        Seq("-Xfatal-warnings")
    },
    scalacOptions ++= Seq(
      "-Wconf:src=src_managed/.*:silent"
    ),
    tpolecatExcludeOptions += ScalacOptions.warnNonUnitStatement,
    initialize := {
      val _ = initialize.value
      val javaVersionFound = sys.props("java.specification.version").toDouble
      if (javaVersionFound < javaVersion)
        sys.error(s"Minimum Java $javaVersion is required for this project. Found $javaVersionFound instead")
    },
    // Configures eviction reports
    evicted / evictionWarningOptions := EvictionWarningOptions.default
      .withWarnDirectEvictions(true)
      .withWarnEvictionSummary(true)
      .withWarnScalaVersionEviction(true)
      .withWarnTransitiveEvictions(true)
      .withShowCallers(true),
    // Checks evictions on resolving dependencies
    update := update.dependsOn(evicted).value
  )
}
