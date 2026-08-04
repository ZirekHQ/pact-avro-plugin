import com.typesafe.sbt.SbtNativePackager.Universal
import com.typesafe.sbt.packager.Keys.*
import sbt.Keys.*
import sbt.{Def, *}

object TestEnvironment {

  lazy val buildTestPluginDir = taskKey[Unit]("Build plugin directory for testing purposes")

  lazy val testEnvSettings: Seq[Def.Setting[?]] =
    Seq(
      buildTestPluginDir := {
        import Path.*

        val testPluginDir = target.value / "plugin"

        val artifactsDir: File = target.value / "artifacts"
        val universalStageDir: File = (Universal / com.typesafe.sbt.packager.Keys.stagingDirectory).value

        val artifactFiles: Seq[File] = (artifactsDir ** "*.json").get()
        val universalStageFiles: Seq[File] = universalStageDir.allPaths.get()

        val pairs1: Seq[(File, File)] = artifactFiles pair rebase(artifactsDir, testPluginDir)
        val pairs2: Seq[(File, File)] = universalStageFiles pair rebase(universalStageDir, testPluginDir)
        val pairs = pairs1 ++ pairs2

        // Copy files to source files to target
        IO.copy(pairs, CopyOptions.apply(overwrite = true, preserveLastModified = true, preserveExecutable = true))

      },
      buildTestPluginDir := buildTestPluginDir.dependsOn(Universal / stage).value
    )

}
