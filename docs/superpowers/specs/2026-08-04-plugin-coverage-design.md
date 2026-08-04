# Test coverage for the `plugin` module

## Goal

The `plugin` module (`modules/plugin`) contains the correctness-sensitive Avro/Pact
matching logic (`AvroContentMatcher`, `RuleParser`, schema handling) but has no
coverage instrumentation, so gaps in the 9-file test suite are invisible. Add
statement coverage, enforce a minimum threshold in CI, and surface the report on
SonarCloud.

## Scope

`modules/plugin` only. `modules/examples/{provider,consumer}` are integration-test
harnesses that round-trip through a real Pact broker rather than exercising plugin
logic directly, so they're excluded.

## Components

1. **`project/plugins.sbt`** — add
   `addSbtPlugin("org.scoverage" % "sbt-scoverage" % "2.4.4")`.

2. **`build.sbt`** (plugin module settings) — add:
   - `coverageMinimumStmtTotal := <baseline>` — value determined by running
     `sbt plugin/coverage plugin/test plugin/coverageReport` locally and reading the
     measured statement coverage, then setting the threshold at or a few points below
     that number. This makes it a real regression guard from day one instead of an
     arbitrary number that immediately fails CI.
   - `coverageFailOnMinimum := true`
   - `coverageHighlighting := true`

3. **`sonar-project.properties`** (repo root):
   ```properties
   sonar.organization=austek
   sonar.projectKey=austek_pact-avro-plugin
   sonar.sources=modules/plugin/src/main/scala
   sonar.tests=modules/plugin/src/test/scala
   sonar.sourceEncoding=UTF-8
   sonar.scala.coverage.reportPaths=modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml
   ```

4. **`github-actions.sbt`** — this repo generates `.github/workflows/ci.yml` from sbt
   config via `sbt-github-actions` (`githubWorkflowGenerate`); CI itself checks the
   generated file is up to date, so it must not be hand-edited.
   - Extend the existing `"Build project"` step's commands to
     `plugin/coverage`, `compile`, `scalafmtCheckAll`, `javafmtCheckAll`, `plugin/test`,
     `plugin/coverageReport` (coverage instrumentation on, then report generated after
     tests run).
   - Add a new `WorkflowStep.Use(UseRef.Public("SonarSource", "sonarqube-scan-action", "v8"))`
     step, gated with `cond = Some("runner.os == 'Linux' && matrix.java == 'zulu@17'")`
     so SonarCloud only receives one analysis submission per commit instead of nine.
     Needs `env = Map("SONAR_TOKEN" -> "${{ secrets.SONAR_TOKEN }}")`.
   - Run `sbt githubWorkflowGenerate` to regenerate `ci.yml` from the updated config.

5. **`README.adoc`** — add a SonarCloud coverage badge next to the existing CI badge.

## Manual step (outside agent control)

Before this CI step can pass, a human needs to:
- Create the SonarCloud project by importing the GitHub repo (auto-import sets
  `organization=austek`, `projectKey=austek_pact-avro-plugin` to match the values
  above — if SonarCloud picks different values, `sonar-project.properties` needs
  updating to match).
- Add `SONAR_TOKEN` as a GitHub Actions repo secret.

Until that's done, the Sonar step will fail on the gated matrix leg; the scoverage
threshold gate (component 2) still enforces coverage locally and in CI independent of
Sonar being configured.

## Testing / validation

- Run `sbt plugin/coverage plugin/test plugin/coverageReport` locally: confirm tests
  still pass with instrumentation on, and the HTML/XML report generates under
  `modules/plugin/target/scala-3.8.4/scoverage-report/`.
- Run `sbt githubWorkflowGenerate` and diff `.github/workflows/ci.yml`: it should
  contain exactly the new coverage/Sonar steps with no unrelated drift.
- Confirm `sbt githubWorkflowCheck` passes (this is the existing CI step that
  verifies the workflow file matches the generated config).
