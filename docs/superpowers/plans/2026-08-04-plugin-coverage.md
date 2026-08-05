# Plugin Module Test Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Instrument the `plugin` sbt module with `sbt-scoverage`, enforce a minimum coverage threshold in CI, and wire coverage reporting into SonarCloud.

**Architecture:** Pure build/CI tooling change — no application code changes, with one exception discovered during execution (Task 2, see amendment note below). `sbt-scoverage` instruments `modules/plugin` only; CI is generated from `github-actions.sbt` via `sbt-github-actions`, so all workflow changes go through that file and `sbt githubWorkflowGenerate`, never a hand-edit of `.github/workflows/ci.yml`.

**Tech Stack:** sbt 1.12.14, Scala 3.8.4, sbt-scoverage 2.4.4, sbt-github-actions (already in use), SonarSource/sonarqube-scan-action v8.

## Global Constraints

- Scope is `modules/plugin` only — do not instrument `modules/examples/{provider,consumer}` (spec: Scope).
- `coverageMinimumStmtTotal` must be set from a measured local baseline, not a guessed number (spec: Components §2).
- `.github/workflows/ci.yml` is generated — never hand-edit it; always regenerate via `sbt githubWorkflowGenerate` and verify with `sbt githubWorkflowCheck` (spec: Components §4).
- Sonar CI step must be gated to exactly one matrix leg (`runner.os == 'Linux' && matrix.java == 'zulu@17'`) to avoid duplicate analysis submissions (spec: Components §4).
- SonarCloud project creation and `SONAR_TOKEN` secret are manual, out-of-band steps — the Sonar CI step is expected to fail until a human does this; do not attempt to work around it (spec: Manual step).

Full design reference: `docs/superpowers/specs/2026-08-04-plugin-coverage-design.md`.

## Plan Amendment (added during execution)

The original Task 2 implementer discovered that turning on scoverage instrumentation
(`plugin/coverage; plugin/test`) doesn't just measure `modules/plugin`'s test suite —
it changes its outcome. 6 of 115 tests fail only when instrumentation is on
(deterministic, reproduced twice; plain `plugin/test` is 115/115 clean). Root cause:
`RecordImplicits.valueOf[T]: T = record.get(name).asInstanceOf[T]`
(`modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/RecordImplicits.scala:51`)
lets `null` flow through an erased generic type parameter that is sometimes primitive
(`Int`, `Long`, `Double`, `Float`, `Boolean`) — whether the JVM ends up unboxing that
`null` to a primitive zero value or keeping it as `null`, and whether a `Utf8` gets
compared as a `String` without a real cast, is bytecode-shape sensitive, and
scoverage's instrumentation flips it for some fields. This was a pre-existing latent
fragility (relying on unboxing-of-null behavior that was never actually guaranteed),
not a new bug scoverage introduces — but coverage tooling can't run cleanly until it's
fixed. A new Task 2 (below) was inserted to fix it; the original Task 2 is renumbered
Task 3, and so on. Human sign-off: fix the fragile cast (not exclude the files from
instrumentation, not just work around it).

---

### Task 1: Add sbt-scoverage plugin

**Files:**
- Modify: `project/plugins.sbt`

**Interfaces:**
- Produces: the `coverage`, `coverageReport`, `coverageOff` sbt commands and `coverageMinimumStmtTotal`/`coverageFailOnMinimum`/`coverageHighlighting` settings keys, consumed by Task 3 and Task 5.

- [x] **Step 1: Add the plugin dependency**

Append to `project/plugins.sbt`:

```scala
addSbtPlugin("org.scoverage" % "sbt-scoverage" % "2.4.4")
```

- [x] **Step 2: Verify the plugin resolves and the build still loads**

Run: `sbt "plugin/coverage; plugin/coverageOff"`
Expected: sbt starts, resolves `sbt-scoverage` 2.4.4, both commands complete with no errors (they just toggle instrumentation on/off — no build required yet).

- [x] **Step 3: Commit**

```bash
git add project/plugins.sbt
git commit -m "chore: add sbt-scoverage plugin"
```

**Status: complete.** Commits `293c825..938ae70`, review clean (one deferred minor: verification ran on the root project instead of scoped to `plugin/`, no functional impact).

---

### Task 2: Fix instrumentation-sensitive null/Utf8 handling in the Avro value comparison path

**Why:** see "Plan Amendment" above. Without this fix, `coverage; plugin/test`
fails 6 tests that pass under plain `plugin/test`, so no valid coverage baseline can be
measured (blocks Task 3).

**Files:**
- Modify: `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/RecordImplicits.scala`
- Modify: `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala`
- Test: `modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsTest.scala` (existing — must go from 6-failing-under-coverage to passing; add regression coverage if the existing tests don't already pin the missing-field behavior precisely enough)

**Interfaces:**
- Consumes: nothing from Task 1 (this task doesn't touch build config).
- Produces: a `coverage; plugin/test` run that is 115/115 green (or however many tests exist after any regression tests are added), consumed by Task 3's baseline measurement.

**Root cause (confirmed by the Task 2-original implementer):** scoverage instrumentation
made previously-elided `asInstanceOf` casts real, so a `null` that used to pass through
untouched could now get forced through primitive unboxing or a genuine `Utf8`-to-`String`
cast, producing wrong zero-values or a `ClassCastException` depending on bytecode shape.
The fix removes the casts entirely rather than working around them, since
`compareValue`'s generic body never actually needed the concrete type — see full detail
below.

`RecordImplicits.valueOf[T](name: String): T = record.get(name).asInstanceOf[T]` is
called for every scalar Avro field type (`STRING`, `BYTES`, `INT`, `LONG`, `FLOAT`,
`DOUBLE`, `BOOLEAN`, `ENUM`, `FIXED`) from `SchemaFieldImplicits.compare`
(lines 32-44). When a field is missing on a `GenericRecord`, `record.get(name)`
returns `null`. Casting that `null` through an *unconstrained* generic `T` is only
safe as long as the value never gets forced through a real primitive-unboxing
conversion — and that is exactly what's ambiguous here: whether the compiled bytecode
treats this `null` as a boxed reference (stays `null`, correct) or triggers unboxing to
a primitive zero value (`0`, `0.0`, `false` — wrong) depends on how the surrounding
statements are shaped, which scoverage's inserted instrumentation calls perturb. The
`Utf8`-vs-`String` `ClassCastException` has the same shape: casting `null` to `String`
never throws (`checkcast` on `null` always succeeds), so the crash only happens when a
real non-`null` `Utf8` value reaches a cast meant for a case that used to see `null` —
same class of bug, same fix.

**The fix:** never call `.asInstanceOf[T]` on a value that might be `null` for a
primitive `T`. Make "missing field" an explicit `Option`, and only cast real,
non-`null` values — this is deterministic regardless of erasure, specialization, or
instrumentation, because `Option(x).map(_.asInstanceOf[T])` only ever invokes the cast
inside `Some`, never on `null`.

- [x] **Step 1: Read the current failing behavior**

Run: `sbt "project plugin; coverage; testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsTest com.github.austek.plugin.avro.PactPluginServiceTest"`
Expected: reproduces the 6 failures described in the Plan Amendment (missing-Int/Long/Double/Float/Boolean fields return the zero value instead of `null`; one `Utf8`-cast `ClassCastException`). Read `RecordImplicitsTest.scala` around lines 109, 158, 207, 256, 305 to see exactly what each missing-field test expects, and read `PactPluginServiceTest.scala` for the failing case's setup (what record/schema it builds, to understand why a genuine `Utf8` reaches the `STRING` branch there).

- [x] **Step 2: Add a null-safe accessor to `RecordImplicits`**

In `RecordImplicits.scala`, add alongside the existing `valueOf[T]` (do not delete
`valueOf` — `SchemaFieldImplicits.compare`'s `RECORD` case and other call sites may
still use direct field access; check with a search for `.valueOf[` across the module
before deciding whether any other call site needs updating too):

```scala
def valueOfOption[T](name: String): Option[T] = Option(record.get(name)).map(_.asInstanceOf[T])
```

- [x] **Step 3: Route the scalar comparison branches through the null-safe accessor**

In `SchemaFieldImplicits.compare` (lines 28-53), the `STRING`/`BYTES`/`INT`/`LONG`/
`FLOAT`/`DOUBLE`/`BOOLEAN`/`ENUM`/`FIXED` cases currently call `compareValue` directly
with `expected.valueOf[T](...)` / `actual.valueOf[T](...)`. Replace each with a call
through `valueOfOption`, handling the four presence combinations explicitly. Add a new
private helper next to `compareValue` that mirrors the existing `expectedNullMismatch`
pattern (already used by `compareArrayField`/`compareMapField` for this exact
"expected present, actual missing" shape — follow that established convention rather
than inventing a new one):

```scala
private def compareOptionalValue[T](
  path: List[String],
  field: Schema.Field,
  expectedOpt: Option[T],
  actualOpt: Option[T],
  diffCallback: () => String,
  context: MatchingContext
): List[AvroBodyItemMatchResult] = (expectedOpt, actualOpt) match {
  case (Some(expected), Some(actual)) => compareValue(path, field, expected, actual, diffCallback, context)
  case (Some(expected), None)         => expectedNullMismatch(path, expected, field.schema().getType.toString)
  case (None, Some(actual)) =>
    List(
      BodyItemMatchResult(
        path.constructPath,
        List(BodyMismatch(null, actual, s"Expected null (Null) but received '$actual' (${field.schema().getType})", path.constructPath, diffCallback()))
      )
    )
  case (None, None) => List(BodyItemMatchResult(path.constructPath, List()))
}
```

Then change each scalar case, e.g.:

```scala
case STRING =>
  Right(compareOptionalValue(path, field, expected.valueOfOption[String](field.name()), actual.valueOfOption[String](field.name()), () => "", context))
```

...and correspondingly for `BYTES` (`ByteBuffer`), `INT` (`Int`), `LONG` (`Long`),
`FLOAT` (`Float`), `DOUBLE` (`Double`), `BOOLEAN` (`Boolean`), `ENUM` (`EnumSymbol`),
`FIXED` (`Fixed`). Leave `ARRAY`, `MAP`, and `RECORD` cases untouched — they already
use `Option`-safe patterns or are out of scope for this bug.

Check whether `expectedNullMismatch`'s existing signature (`path: List[String],
expected: T, valueType: String` at `SchemaFieldImplicits.scala:329`) fits this call
directly, or needs its `expected`/message text adjusted to read correctly for scalar
fields (it was written for `Array`/`Map` — the message says
`"Expected null (Null) to be equal to '$expected' ($valueType)"`, check this reads
sensibly for e.g. an `Int`).

- [x] **Step 4: Run the previously-failing tests under coverage**

Run: `sbt "project plugin; coverage; testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsTest com.github.austek.plugin.avro.PactPluginServiceTest"`
Expected: all pass. If any still fail, read the new failure carefully — do not
loosen the test assertions to make them pass; the fix must produce the same
`null`-vs-missing semantics the tests already encode, just deterministically.

- [x] **Step 5: Run the full plugin suite, both with and without coverage**

Run: `sbt "plugin/test"` then `sbt "project plugin; coverage; test"`
Expected: 115/115 (or current total) passing in both runs, byte-for-byte same pass
count. This is the actual acceptance criterion for this task — instrumented and
uninstrumented runs must agree.

- [x] **Step 6: Commit**

```bash
git add modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/RecordImplicits.scala modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala
git commit -m "fix: make missing-field comparison null-safe regardless of coverage instrumentation"
```

If Step 3 required touching test files (e.g. a test's expectation was actually wrong,
not just instrumentation-sensitive), stage those separately with a clear note in the
report — do not fold a test-expectation change into this commit silently.

**Status: complete.** Commits `3815cc5..960ec53`, review clean. 115/115 tests pass
under both `plugin/test` and `coverage; plugin/test`.

---

### Task 3: Measure baseline coverage and add threshold gate

**Files:**
- Modify: `build.sbt` (the `plugin` project's `.settings(...)` block, alongside the existing `libraryDependencies` and `dependencyOverrides` entries)

**Interfaces:**
- Consumes: `coverage`/`coverageReport` commands from Task 1; a `coverage; plugin/test` run that is fully green from Task 2.
- Produces: `coverage`, `plugin/test`, `plugin/coverageReport` sequence that later fails the build if statement coverage regresses below the committed threshold — consumed by Task 5's CI step and Task 6's final validation.

- [x] **Step 1: Run the instrumented test suite to measure the current baseline**

Run: `sbt "coverage; plugin/test; plugin/coverageReport"`
Expected: build succeeds, tests pass (all of them — Task 2 must be complete first), and
the console output ends with a line like:
```
[info] Statement coverage.: 62.34%
[info] Branch coverage....: 55.10%
```
Note the statement coverage percentage — this is the baseline. The full HTML report is at `modules/plugin/target/scala-3.8.4/scoverage-report/index.html`.

- [x] **Step 2: Add coverage settings to the `plugin` project**

In `build.sbt`, inside `lazy val plugin = moduleProject(...)....settings(...)`, add (as a new top-level setting alongside the existing ones, e.g. after `dependencyOverrides`):

```scala
    coverageMinimumStmtTotal := <baseline_rounded_down_to_nearest_5>,
    coverageFailOnMinimum := true,
    coverageHighlighting := true
```

Replace `<baseline_rounded_down_to_nearest_5>` with the actual number from Step 1, rounded down to the nearest multiple of 5 (e.g. a measured 62.34% becomes `60`). This leaves headroom so the gate doesn't flake on minor coverage noise while still catching real regressions.

- [x] **Step 3: Verify the gate passes at the committed threshold**

Run: `sbt "coverage; plugin/test; plugin/coverageReport"`
Expected: build succeeds (exit code 0), no "Coverage is below minimum" error.

- [x] **Step 4: Verify the gate actually fails below threshold (sanity check)**

Temporarily run: `sbt "coverage; plugin/test; set plugin/coverageMinimumStmtTotal := 99.0; plugin/coverageReport"`
Expected: build fails with a "Coverage is below minimum" error, confirming the gate is wired correctly. This is a throwaway in-session `set` — it does not modify `build.sbt`.

- [x] **Step 5: Commit**

```bash
git add build.sbt
git commit -m "chore: enforce minimum statement coverage on plugin module"
```

**Status: complete.** Commit `3e63d44` (baseline threshold gate), amended by the final
fix wave's commit adding `coverageExcludedPackages` and re-baselining the threshold to
`55` against hand-written-only statement coverage (see final-fix-report.md).

---

### Task 4: Add SonarCloud project configuration

**Files:**
- Create: `sonar-project.properties`

**Interfaces:**
- Consumes: the scoverage XML report path produced by Task 3's `plugin/coverageReport` (`modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml`).
- Produces: the Sonar project/org keys consumed by Task 5's CI step.

- [x] **Step 1: Create the properties file**

```properties
sonar.organization=austek
sonar.projectKey=austek_pact-avro-plugin
sonar.sources=modules/plugin/src/main/scala
sonar.tests=modules/plugin/src/test/scala
sonar.sourceEncoding=UTF-8
sonar.scala.coverage.reportPaths=modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml
```

- [x] **Step 2: Verify the referenced report path exists after a coverage run**

Run: `sbt "coverage; plugin/test; plugin/coverageReport"` (if not already run in this session), then:
```bash
test -f modules/plugin/target/scala-3.8.4/scoverage-report/scoverage.xml && echo "OK: report exists"
```
Expected: prints `OK: report exists`. This confirms the path in `sonar.scala.coverage.reportPaths` is correct — actual Sonar ingestion can't be tested locally without `SONAR_TOKEN` (flagged as a manual step in the spec).

- [x] **Step 3: Commit**

```bash
git add sonar-project.properties
git commit -m "chore: add SonarCloud project configuration"
```

**Status: complete.** Commit `7d0fff7`, review clean.

---

### Task 5: Wire coverage and Sonar scan into CI

**Files:**
- Modify: `github-actions.sbt`
- Generated (do not hand-edit): `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `coverage`/`plugin/coverageReport` from Task 1/3, `sonar-project.properties` from Task 4.

- [x] **Step 1: Extend the "Build project" step to run coverage**

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
      "compile",
      "scalafmtCheckAll",
      "javafmtCheckAll",
      "coverage",
      "plugin/test",
      "plugin/coverageReport"
    )
  ),
```

**Why `"coverage"` (bare) instead of `"plugin/coverage"`, and why it comes after
`compile`/format checks instead of first:** Task 3's implementer discovered
`plugin/coverage` is invalid sbt syntax — `coverage` is an sbt **Command** (it sets
`ThisBuild / coverageEnabled := true`), not a scopable task/setting key, so it cannot
take a `project/` prefix at all; it's already build-wide by design. Ordering matters
because of that build-wide scope: `compile` in this same step is the root aggregate
project's compile, which also builds `provider`/`consumer` (aggregation). Running
`coverage` before `compile` would instrument `provider`/`consumer` too, violating the
"scope is `modules/plugin` only" constraint. Running `coverage` after the plain
`compile` has already built everything uninstrumented, then only running `plugin/test`
(which triggers `plugin`'s own incremental recompile, now instrumented, without
touching `provider`/`consumer`) keeps instrumentation scoped to `plugin` exactly as
Task 3 configured it.

- [x] **Step 2: Add the SonarCloud scan step**

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

- [x] **Step 3: Regenerate the CI workflow**

Run: `sbt githubWorkflowGenerate`
Expected: `.github/workflows/ci.yml` is rewritten. Run `git diff .github/workflows/ci.yml` and confirm the only changes are: the new `coverage`/`plugin/coverageReport` entries in the "Build project" step's `run` command, and a new "SonarCloud Scan" step using `SonarSource/sonarqube-scan-action@v8` with the `cond`/`env` from Step 2. No other lines should change.

- [x] **Step 4: Verify the generated workflow is self-consistent**

Run: `sbt githubWorkflowCheck`
Expected: exits 0 (this is the same check CI runs to catch hand-edited/stale workflow files).

- [x] **Step 5: Commit**

```bash
git add github-actions.sbt .github/workflows/ci.yml
git commit -m "ci: run plugin coverage and SonarCloud scan in CI"
```

**Status: complete.** Commits `901cfbd..e0135b7`, review clean. Note: the final fix
wave's fork-PR mitigation (`continue-on-error` on the SonarCloud step) could not be
applied — `sbt-github-actions` 0.31.0 (latest available) has no `continue-on-error`
field on any `WorkflowStep`, so this remains a known limitation; see
final-fix-report.md.

---

### Task 6: Add README badge and final validation

**Files:**
- Modify: `README.adoc`

- [x] **Step 1: Add the SonarCloud coverage badge**

In `README.adoc`, the current first line is:

```adoc
= Pact Avro Plugin image:https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml/badge.svg[Pact-Avro-Plugin Build,link=https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml]
```

Append a second badge image to the same line (space-separated, matching the existing single-line badge convention):

```adoc
= Pact Avro Plugin image:https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml/badge.svg[Pact-Avro-Plugin Build,link=https://github.com/austek/pact-avro-plugin/actions/workflows/ci.yml] image:https://sonarcloud.io/api/project_badges/measure?project=austek_pact-avro-plugin&metric=coverage[Coverage,link=https://sonarcloud.io/summary/new_code?id=austek_pact-avro-plugin]
```

- [x] **Step 2: Full local validation run**

Run: `sbt "compile; scalafmtCheckAll; javafmtCheckAll; coverage; plugin/test; plugin/coverageReport; githubWorkflowCheck"`
Expected: all tasks succeed, exit code 0 — this is the same sequence CI will run (minus the OS-specific pact-broker steps and the Sonar scan, which needs `SONAR_TOKEN`).

- [x] **Step 3: Commit**

```bash
git add README.adoc
git commit -m "docs: add SonarCloud coverage badge"
```

- [x] **Step 4: Note remaining manual step for the user**

No file change. Confirm to the user that before merging/pushing, they still need to: create the SonarCloud project (GitHub auto-import) and add `SONAR_TOKEN` as a repo secret, per the spec's "Manual step" section — otherwise the gated Sonar CI leg will fail (the scoverage threshold gate itself is independent and will still work).

**Status: complete.** Commit `ce34ec7`, review clean.
