// sbt-github-actions

ThisBuild / githubWorkflowOSes := Seq("ubuntu-latest", "macos-latest", "windows-latest")
ThisBuild / githubWorkflowJavaVersions := Seq(
  JavaSpec.zulu("17"),
  JavaSpec.zulu("21"),
  JavaSpec.zulu("25")
)
ThisBuild / githubWorkflowTargetBranches := Seq("main")
ThisBuild / githubWorkflowTargetTags := Seq("v*")

ThisBuild / githubWorkflowEnv := Map(
  "GITHUB_TOKEN" -> "${{ secrets.GITHUB_TOKEN }}"
)
ThisBuild / githubWorkflowBuildMatrixFailFast := Some(false)
ThisBuild / githubWorkflowBuild := Seq(
  WorkflowStep.Run(
    name = Some("Set outputs"),
    id = Some("vars"),
    commands = List(
      """echo "sha_short=$(git rev-parse --short ${{ github.sha }})" >> $GITHUB_OUTPUT""",
      """echo "git_tag=$(git describe --tags)" >> $GITHUB_OUTPUT"""
    )
  ),
  // Service containers only run on Linux GitHub-hosted runners, so the pact-broker
  // (and pact publish / provider verification against it) only runs on ubuntu-latest.
  // macOS and Windows legs still compile and run the plugin/consumer test suites.
  WorkflowStep.Run(
    name = Some("Start pact-broker"),
    commands = List("docker compose up -d pact-broker"),
    cond = Some("runner.os == 'Linux'")
  ),
  WorkflowStep.Run(
    name = Some("Wait for pact-broker"),
    commands = List("""for i in $(seq 1 30); do
        |  if curl -sf http://localhost:9292/diagnostic/status/heartbeat > /dev/null; then
        |    echo "pact-broker is up"
        |    exit 0
        |  fi
        |  echo "waiting for pact-broker..."
        |  sleep 2
        |done
        |echo "pact-broker did not become ready in time" >&2
        |exit 1
        |""".stripMargin),
    cond = Some("runner.os == 'Linux'")
  ),
  WorkflowStep.Use(
    UseRef.Public("pactflow", "actions", "main"),
    name = Some("Install pact-broker CLI"),
    cond = Some("runner.os == 'Linux'")
  ),
  WorkflowStep.Sbt(
    name = Some("Build project"),
    commands = List("compile", "scalafmtCheckAll", "javafmtCheckAll", "plugin/test")
  ),
  WorkflowStep.Sbt(
    name = Some("Test Consumer"),
    commands = List("consumer/test")
  ),
  WorkflowStep.Run(
    name = Some("Pact publish"),
    commands = List("""pact-broker publish
        | "modules/examples/consumer/target/pacts"
        | --consumer-app-version=${{ steps.vars.outputs.git_tag }}-${{ runner.os }}
        | --tag=${{ steps.vars.outputs.git_tag }}-${{ runner.os }}
        | """.stripMargin.replaceAll("\n", "")),
    cond = Some("runner.os == 'Linux'"),
    env = Map("PACT_BROKER_BASE_URL" -> "http://localhost:9292")
  ),
  WorkflowStep.Sbt(
    name = Some("Test Provider"),
    commands = List("provider/test"),
    cond = Some("runner.os == 'Linux'"),
    env = Map(
      "PACT_BROKER_BASE_URL" -> "http://localhost:9292",
      "PACT_BROKER_TAG" -> "${{ steps.vars.outputs.git_tag }}-${{ runner.os }}",
    )
  )
)

ThisBuild / githubWorkflowPublishTargetBranches := Seq(
  RefPredicate.StartsWith(Ref.Tag("v"))
)

ThisBuild / githubWorkflowPublish := Seq(
  WorkflowStep.Use(
    UseRef.Public("actions", "setup-node", "v7"),
    name = Some("Doc - Install node"),
    params = Map("node-version" -> "24.x")
  ),
  WorkflowStep.Run(
    name = Some("Doc - Install dependencies"),
    commands = List("npm ci")
  ),
  WorkflowStep.Run(
    name = Some("Doc - build"),
    commands = List("./scripts/docBuild.sh ${{ github.ref }}")
  ),
  WorkflowStep.Sbt(
    name = Some("Build package"),
    commands = List("universal:packageZipTarball")
  ),
  WorkflowStep.Run(
    name = Some("Prepare Artifacts"),
    commands = List("./scripts/prepArtifacts.sh")
  ),
  WorkflowStep.Use(
    UseRef.Public("svenstaro", "upload-release-action", "v2"),
    name = Some("Upload Release Assets"),
    id = Some("upload-release-asset"),
    params = Map(
      "repo_token" -> "${{ secrets.GITHUB_TOKEN }}",
      "file" -> "target/artifacts/*",
      "file_glob" -> "true",
      "tag" -> "${{ github.ref }}"
    )
  ),
  WorkflowStep.Sbt(
    name = Some("Publish docs"),
    commands = List("publishToGitHubPages"),
    env = Map(
      "GITHUB_TOKEN" -> "${{ secrets.GITHUB_TOKEN }}"
    )
  )
)
