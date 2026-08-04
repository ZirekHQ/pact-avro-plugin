# Plugin Module Test Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Instrument the `plugin` sbt module with `sbt-scoverage`, enforce a minimum coverage threshold in CI, and wire coverage reporting into SonarCloud.

**Architecture:** Pure build/CI tooling change — no application code changes. `sbt-scoverage` instruments `modules/plugin` only; CI is generated from `github-actions.sbt` via `sbt-github-actions`, so all workflow changes go through that file and `sbt githubWorkflowGenerate`, never a hand-edit of `.github/workflows/ci.yml`.

**Tech Stack:** sbt 1.12.14, Scala 3.8.4, sbt-scoverage 2.4.4, sbt-github-actions (already in use), SonarSource/sonarqube-scan-action v8.

## Global Constraints

- Scope is `modules/plugin` only — do not instrument `modules/examples/{provider,consumer}` (spec: Scope).
- `coverageMinimumStmtTotal` must be set from a measured local baseline, not a guessed number (spec: Components §2).
- `.github/workflows/ci.yml` is generated — never hand-edit it; always regenerate via `sbt githubWorkflowGenerate` and verify with `sbt githubWorkflowCheck` (spec: Components §4).
- Sonar CI step must be gated to exactly one matrix leg (`runner.os == 'Linux' && matrix.java == 'zulu@17'`) to avoid duplicate analysis submissions (spec: Components §4).
- SonarCloud project creation and `SONAR_TOKEN` secret are manual, out-of-band steps — the Sonar CI step is expected to fail until a human does this; do not attempt to work around it (spec: Manual step).

Full design reference: `docs/superpowers/specs/2026-08-04-plugin-coverage-design.md`.

---

### Task 1: Add sbt-scoverage plugin

**Files:**
- Modify: `project/plugins.sbt`

**Interfaces:**
- Produces: the `coverage`, `coverageReport`, `coverageOff` sbt commands and `coverageMinimumStmtTotal`/`coverageFailOnMinimum`/`coverageHighlighting` settings keys, consumed by Task 2 and Task 4.

- [ ] **Step 1: Add the plugin dependency**

Append to `project/plugins.sbt`:

```scala
addSbtPlugin("org.scoverage" % "sbt-scoverage" % "2.4.4")
```

- [ ] **Step 2: Verify the plugin resolves and the build still loads**

Run: `sbt "plugin/coverage; plugin/coverageOff"`
Expected: sbt starts, resolves `sbt-scoverage` 2.4.4, both commands complete with no errors (they just toggle instrumentation on/off — no build required yet).

- [ ] **Step 3: Commit**

```bash
git add project/plugins.sbt
git commit -m "chore: add sbt-scoverage plugin"
```

---

### Task 2: Measure baseline coverage and add threshold gate

**Files:**
- Modify: `build.sbt` (the `plugin` project's `.settings(...)` block, alongside the existing `libraryDependencies` and `dependencyOverrides` entries)

**Interfaces:**
- Consumes: `coverage`/`coverageReport` commands from Task 1.
- Produces: `plugin/coverage`, `plugin/test`, `plugin/coverageReport` sequence that later fails the build if statement coverage regresses below the committed threshold — consumed by Task 4's CI step and Task 5's final validation.

- [ ] **Step 1: Run the instrumented test suite to measure the current baseline**

Run: `sbt "plugin/coverage; plugin/test; plugin/coverageReport"`
Expected: build succeeds, tests pass, and the console output ends with a line like:
```
[info] Statement coverage.: 62.34%
[info] Branch coverage....: 55.10%
```
Note the statement coverage percentage — this is the baseline. The full HTML report is at `modules/plugin/target/scala-3.8.4/scoverage-report/index.html`.

- [ ] **Step 2: Add coverage settings to the `plugin` project**

In `build.sbt`, inside `lazy val plugin = moduleProject(...)....settings(...)`, add (as a new top-level setting alongside the existing ones, e.g. after `dependencyOverrides`):

```scala
    coverageMinimumStmtTotal := <baseline_rounded_down_to_nearest_5>,
    coverageFailOnMinimum := true,
    coverageHighlighting := true
```

Replace `<baseline_rounded_down_to_nearest_5>` with the actual number from Step 1, rounded down to the nearest multiple of 5 (e.g. a measured 62.34% becomes `60`). This leaves headroom so the gate doesn't flake on minor coverage noise while still catching real regressions.

- [ ] **Step 3: Verify the gate passes at the committed threshold**

Run: `sbt "plugin/coverage; plugin/test; plugin/coverageReport"`
Expected: build succeeds (exit code 0), no "Coverage is below minimum" error.

- [ ] **Step 4: Verify the gate actually fails below threshold (sanity check)**

Temporarily run: `sbt "plugin/coverage; plugin/test; set plugin/coverageMinimumStmtTotal := 99.0; plugin/coverageReport"`
Expected: build fails with a "Coverage is below minimum" error, confirming the gate is wired correctly. This is a throwaway in-session `set` — it does not modify `build.sbt`.

- [ ] **Step 5: Commit**

```bash
git add build.sbt
git commit -m "chore: enforce minimum statement coverage on plugin module"
```

---

### Task 3: Add SonarCloud project configuration

**Files:**
- Create: `sonar-project.properties`

**Interfaces:**
- Consumes: the scoverage XML report path produced by Task 2's `plugin/coverageReport` (`modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml`).
- Produces: the Sonar project/org keys consumed by Task 4's CI step.

- [ ] **Step 1: Create the properties file**

```properties
sonar.organization=austek
sonar.projectKey=austek_pact-avro-plugin
sonar.sources=modules/plugin/src/main/scala
sonar.tests=modules/plugin/src/test/scala
sonar.sourceEncoding=UTF-8
sonar.scala.coverage.reportPaths=modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml
```

- [ ] **Step 2: Verify the referenced report path exists after a coverage run**

Run: `sbt "plugin/coverage; plugin/test; plugin/coverageReport"` (if not already run in this session), then:
```bash
test -f modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml && echo "OK: report exists"
```
Expected: prints `OK: report exists`. This confirms the path in `sonar.scala.coverage.reportPaths` is correct — actual Sonar ingestion can't be tested locally without `SONAR_TOKEN` (flagged as a manual step in the spec).

- [ ] **Step 3: Commit**

```bash
git add sonar-project.properties
git commit -m "chore: add SonarCloud project configuration"
```

---

### Task 4: Wire coverage and Sonar scan into CI

**Files:**
- Modify: `github-actions.sbt`
- Generated (do not hand-edit): `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `plugin/coverage`/`plugin/coverageReport` from Task 1/2, `sonar-project.properties` from Task 3.

- [ ] **Step 1: Extend the "Build project" step to run coverage**

In `github-actions.sbt`, find:

```scala
  WorkflowStep.Sbt(
    name = Some("Build project"),
    commands = List("compile", "scalafmtCheckAll", "javafmtCheckAll", "plugin/test")
  ),
```

Replace with:

```scala
  WorkflowStep.Sbt(
    name = Some("Build project"),
    commands = List(
      "plugin/coverage",
      "compile",
      "scalafmtCheckAll",
      "javafmtCheckAll",
      "plugin/test",
      "plugin/coverageReport"
    )
  ),
```

- [ ] **Step 2: Add the SonarCloud scan step**

In `github-actions.sbt`, after the `"Test Provider"` `WorkflowStep.Sbt` entry (the last element in the `githubWorkflowBuild` `Seq`), add a new element:

```scala
  WorkflowStep.Use(
    UseRef.Public("SonarSource", "sonarqube-scan-action", "v8"),
    name = Some("SonarCloud Scan"),
    cond = Some("runner.os == 'Linux' && matrix.java == 'zulu@17'"),
    env = Map("SONAR_TOKEN" -> "${{ secrets.SONAR_TOKEN }}")
  )
```

Make sure it's a sibling of the existing steps inside the same `Seq(...)` (comma-separated), not nested inside another step.

- [ ] **Step 3: Regenerate the CI workflow**

Run: `sbt githubWorkflowGenerate`
Expected: `.github/workflows/ci.yml` is rewritten. Run `git diff .github/workflows/ci.yml` and confirm the only changes are: the new `plugin/coverage`/`plugin/coverageReport` entries in the "Build project" step's `run` command, and a new "SonarCloud Scan" step using `SonarSource/sonarqube-scan-action@v8` with the `cond`/`env` from Step 2. No other lines should change.

- [ ] **Step 4: Verify the generated workflow is self-consistent**

Run: `sbt githubWorkflowCheck`
Expected: exits 0 (this is the same check CI runs to catch hand-edited/stale workflow files).

- [ ] **Step 5: Commit**

```bash
git add github-actions.sbt .github/workflows/ci.yml
git commit -m "ci: run plugin coverage and SonarCloud scan in CI"
```

---

### Task 5: Add README badge and final validation

**Files:**
- Modify: `README.adoc`

- [ ] **Step 1: Add the SonarCloud coverage badge**

In `README.adoc`, the current first line is:

```adoc
= Pact Avro Plugin image:https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml/badge.svg[Pact-Avro-Plugin Build,link=https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml]
```

Append a second badge image to the same line (space-separated, matching the existing single-line badge convention):

```adoc
= Pact Avro Plugin image:https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml/badge.svg[Pact-Avro-Plugin Build,link=https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml] image:https://sonarcloud.io/api/project_badges/measure?project=austek_pact-avro-plugin&metric=coverage[Coverage,link=https://sonarcloud.io/summary/new_code?id=austek_pact-avro-plugin]
```

- [ ] **Step 2: Full local validation run**

Run: `sbt "compile; scalafmtCheckAll; javafmtCheckAll; plugin/coverage; plugin/test; plugin/coverageReport; githubWorkflowCheck"`
Expected: all tasks succeed, exit code 0 — this is the same sequence CI will run (minus the OS-specific pact-broker steps and the Sonar scan, which needs `SONAR_TOKEN`).

- [ ] **Step 3: Commit**

```bash
git add README.adoc
git commit -m "docs: add SonarCloud coverage badge"
```

- [ ] **Step 4: Note remaining manual step for the user**

No file change. Confirm to the user that before merging/pushing, they still need to: create the SonarCloud project (GitHub auto-import) and add `SONAR_TOKEN` as a repo secret, per the spec's "Manual step" section — otherwise the gated Sonar CI leg will fail (the scoverage threshold gate itself is independent and will still work).
