# Avro Comparison Null-Safety Fixes (#58) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two pre-existing bugs in `SchemaFieldImplicits` — a `Utf8`/`String` equality asymmetry in the no-matcher comparison branch, and an NPE when a nested record is missing on the actual side — both from GitHub issue #58.

**Architecture:** Both fixes live in `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala`. Fix 1 extracts the existing `Utf8 -> String` normalization (already used in the matcher-defined branch of `compareValue`) into a shared private helper and applies it to the plain-equality branch too. Fix 2 makes the `RECORD` case of `SchemaField.compare` mirror the existing `Option`-guarded pattern already used by the `ARRAY`/`MAP` cases, reusing the existing `expectedNullMismatch` helper instead of dereferencing a possibly-null actual record.

**Tech Stack:** Scala 3, sbt, ScalaTest (`AnyWordSpec`), Apache Avro (`GenericData.Record`, `Utf8`), pact-jvm core matchers.

## Global Constraints

- Build fails on any compiler warning (`-Werror` is enabled) — no unused imports or unused locals in test code.
- Follow the existing test style in this package exactly: `AnyWordSpec with Matchers with EitherValues`, `implicit val context: MatchingContext`, `.value` to unwrap the `Either` result, `result shouldBe List(...)`.
- Both fixes ship in a single PR/branch (confirmed in the design spec) — commit them as two separate commits on that branch, not two PRs.
- Spec: `docs/superpowers/specs/2026-08-05-avro-comparison-null-safety-design.md`

---

### Task 1: Fix Utf8/String equality asymmetry in `compareValue`

**Files:**
- Modify: `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala:256-312`
- Test: `modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsTest.scala`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: a private `normalizeUtf8(value: Any): Any` helper inside `implicit class SchemaField` in `SchemaFieldImplicits.scala`, used by both branches of `compareValue`. Task 2 does not depend on this helper.

- [ ] **Step 1: Write the failing test**

The equality (no-matcher) branch of `compareValue` currently has zero test coverage — every existing test in this package registers a `matching(...)`/`notEmpty(...)` pact rule, so `context.matcherDefined(path)` is always `true`. To exercise the buggy branch, build both records by hand and use an empty `MatchingRuleCategory` (no rules registered anywhere) instead of going through `AvroRecord`'s pact-config parser.

Add `import org.apache.avro.util.Utf8` to the top of `modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsTest.scala` (after the existing `import org.apache.avro.generic.GenericData` on line 11):

```scala
import org.apache.avro.generic.GenericData
import org.apache.avro.util.Utf8
```

Insert this new `should` block right after the closing `}` of `"comparing Bytes fields" should { ... }` (currently line 468), before the two closing braces that end the file (currently lines 469-470):

```scala
    "comparing String fields without a matching rule" should {
      val schema = schemaWithField("""{"name": "street", "type": "string"}""")

      implicit val context: MatchingContext = new MatchingContext(new MatchingRuleCategory("body"), false)

      "return empty BodyMatch list when Utf8 and String values are textually equal" in {
        val record = new GenericData.Record(schema)
        record.put("street", "hello")

        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("street", new Utf8("hello"))

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.street", List()))
      }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `sbt "plugin/testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsTest"`

Expected: FAIL — the new test fails because `expected == actual` compares a `String` (`"hello"`) against a `Utf8` (`new Utf8("hello")`) directly, so it returns a `BodyMismatch` instead of an empty match list, even though the text is identical. (Confirmed manually before writing this plan: the current code produces `BodyMismatch(expected=hello, actual=hello, mismatch=Expected 'hello' (STRING) but received value 'hello', ...)`.)

- [ ] **Step 3: Implement the fix**

In `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala`, replace the `compareValue` method (lines 256-312):

```scala
    private def compareValue[T](
      path: List[String],
      field: Schema.Field,
      expected: T,
      actual: T,
      diffCallback: () => String,
      context: MatchingContext
    ): List[AvroBodyItemMatchResult] = {
      val valuePath = path.constructPath
      logger.debug(s">>> compareValue($path, $field, $expected, $actual, $context)")
      if (context.matcherDefined(path.asJava)) {
        logger.debug(s"compareValue: Matcher defined for path $path")

        val expectedJava = expected match {
          case s: Utf8 => s.toString
          case _       => expected
        }
        val actualJava = actual match {
          case s: Utf8 => s.toString
          case _       => actual
        }

        List(
          new AvroBodyItemMatchResult(
            valuePath,
            Matchers.domatch(
              context,
              path.asJava,
              expectedJava,
              actualJava,
              (expected: Any, actual: Any, message: String, path: java.util.List[String]) =>
                BodyMismatch(expected, actual, message, constructPath(path), diffCallback())
            )
          )
        )
      } else {
        logger.debug(s"compareValue: No matcher defined for path $path, using equality")
        if (expected == actual) {
          List(BodyItemMatchResult(valuePath, List()))
        } else {
          List(
            BodyItemMatchResult(
              valuePath,
              List(
                BodyMismatch(
                  expected,
                  actual,
                  s"Expected '$expected' (${field.schema().getType}) but received value '$actual'",
                  valuePath,
                  diffCallback()
                )
              )
            )
          )
        }
      }
    }
```

with:

```scala
    private def normalizeUtf8(value: Any): Any = value match {
      case s: Utf8 => s.toString
      case _       => value
    }

    private def compareValue[T](
      path: List[String],
      field: Schema.Field,
      expected: T,
      actual: T,
      diffCallback: () => String,
      context: MatchingContext
    ): List[AvroBodyItemMatchResult] = {
      val valuePath = path.constructPath
      logger.debug(s">>> compareValue($path, $field, $expected, $actual, $context)")
      if (context.matcherDefined(path.asJava)) {
        logger.debug(s"compareValue: Matcher defined for path $path")

        val expectedJava = normalizeUtf8(expected)
        val actualJava = normalizeUtf8(actual)

        List(
          new AvroBodyItemMatchResult(
            valuePath,
            Matchers.domatch(
              context,
              path.asJava,
              expectedJava,
              actualJava,
              (expected: Any, actual: Any, message: String, path: java.util.List[String]) =>
                BodyMismatch(expected, actual, message, constructPath(path), diffCallback())
            )
          )
        )
      } else {
        logger.debug(s"compareValue: No matcher defined for path $path, using equality")
        if (normalizeUtf8(expected) == normalizeUtf8(actual)) {
          List(BodyItemMatchResult(valuePath, List()))
        } else {
          List(
            BodyItemMatchResult(
              valuePath,
              List(
                BodyMismatch(
                  expected,
                  actual,
                  s"Expected '$expected' (${field.schema().getType}) but received value '$actual'",
                  valuePath,
                  diffCallback()
                )
              )
            )
          )
        }
      }
    }
```

Note: the mismatch message on the `else` branch still uses the original `expected`/`actual` (not normalized) — `Utf8.toString` and `String` render identically, so the message text is unaffected; only the equality check changes.

- [ ] **Step 4: Run test to verify it passes**

Run: `sbt "plugin/testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsTest"`

Expected: PASS — all tests in `RecordImplicitsTest`, including the new one.

- [ ] **Step 5: Commit**

```bash
git add modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsTest.scala
git commit -m "Fix Utf8/String equality asymmetry in Avro field comparison

Normalize Utf8 to String before the plain-equality comparison branch
in compareValue, matching what the matcher-defined branch already did.
Part of #58."
```

---

### Task 2: Fix missing nested-record NPE in `SchemaField.compare`

**Files:**
- Modify: `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala:33-35`
- Create: `modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsRecordsTest.scala`

**Interfaces:**
- Consumes: the existing private `expectedNullMismatch[T](path: List[String], expected: T, valueType: String): List[AvroBodyItemMatchResult]` helper, already defined at the bottom of `SchemaFieldImplicits.scala` (lines 315-321) and already used by the `ARRAY`/`MAP` `None` branches. Not affected by Task 1.
- Produces: nothing consumed by other tasks.

- [ ] **Step 1: Write the failing test**

Create `modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsRecordsTest.scala`:

```scala
package com.github.austek.plugin.avro.implicits

import au.com.dius.pact.core.matchers.MatchingContext
import au.com.dius.pact.core.model.matchingrules.MatchingRuleCategory
import com.github.austek.plugin.avro.Avro.AvroRecord
import com.github.austek.plugin.avro.TestSchemas.*
import RecordImplicits.*
import com.github.austek.plugin.avro.matchers.{BodyItemMatchResult, BodyMismatch}
import com.google.protobuf.struct.{Struct, Value}
import com.google.protobuf.struct.Value.Kind.*
import org.apache.avro.generic.GenericData
import org.scalatest.EitherValues
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpec

class RecordImplicitsRecordsTest extends AnyWordSpec with Matchers with EitherValues {
  "GenericRecord" when {
    "comparing nested Record fields" should {
      val schema = schemaWithField("""{
                                     |  "name": "address",
                                     |  "type": {
                                     |    "name": "address",
                                     |    "type": "record",
                                     |    "fields": [ { "name": "street", "type": "string" } ]
                                     |  }
                                     |}""".stripMargin)
      val pactConfiguration: Map[String, Value] = Map(
        "address" -> Value(StructValue(Struct(Map("street" -> Value(StringValue("matching(equalTo, 'street name')"))))))
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      val addressRecord = new GenericData.Record(schema.getField("address").schema())
      addressRecord.put("street", "street name")

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("address", addressRecord)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.address.street", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherAddressRecord = new GenericData.Record(schema.getField("address").schema())
        otherAddressRecord.put("street", "other")
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("address", otherAddressRecord)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe
          List(
            BodyItemMatchResult(
              "$.address.street",
              List(
                BodyMismatch("street name", "other", "Expected 'other' (String) to be equal to 'street name' (String)", "$.address.street", "")
              )
            )
          )
      }

      "return a BodyMatch instead of throwing NPE for a missing nested record" in {
        val otherRecord = new GenericData.Record(schema)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.address",
            List(
              BodyMismatch(addressRecord, null, s"Expected null (Null) to be equal to '$addressRecord' (Record)", "$.address", null)
            )
          )
        )
      }
    }
  }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `sbt "plugin/testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsRecordsTest"`

Expected: the first two tests (`equal fields`, `unequal fields`) PASS already (they don't touch the missing-record path). The third test, `"return a BodyMatch instead of throwing NPE for a missing nested record"`, FAILS with a `NullPointerException` — `.value` on the `Either` throws because `record.compare(...)` itself throws `NullPointerException: Cannot invoke "org.apache.avro.generic.GenericRecord.getSchema()" because "other" is null` before ever returning an `Either`. (Confirmed manually before writing this plan.)

- [ ] **Step 3: Implement the fix**

In `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala`, replace the `RECORD` case inside `SchemaField.compare` (currently lines 33-35):

```scala
            case RECORD =>
              val fieldName = path.last
              expected.get(fieldName).asInstanceOf[GenericRecord].compare(path, actual.get(fieldName).asInstanceOf[GenericRecord])
```

with:

```scala
            case RECORD =>
              val fieldName = path.last
              val expectedRecord = expected.get(fieldName).asInstanceOf[GenericRecord]
              Option(actual.get(fieldName).asInstanceOf[GenericRecord]) match {
                case Some(actualRecord) => expectedRecord.compare(path, actualRecord)
                case None                => Right(expectedNullMismatch(path, expectedRecord, "Record"))
              }
```

This mirrors the existing `ARRAY`/`MAP` cases exactly: only the `actual` fetch is `Option`-wrapped (`expected` is assumed always present, per the established convention in `compareArrayField`/`compareMapField`), and a missing `actual` produces a proper mismatch via the existing `expectedNullMismatch` helper instead of NPEing.

- [ ] **Step 4: Run test to verify it passes**

Run: `sbt "plugin/testOnly com.github.austek.plugin.avro.implicits.RecordImplicitsRecordsTest"`

Expected: PASS — all three tests.

- [ ] **Step 5: Commit**

```bash
git add modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala modules/plugin/src/test/scala/com/github/austek/plugin/avro/implicits/RecordImplicitsRecordsTest.scala
git commit -m "Fix NPE when a nested record is missing on the actual side

The RECORD case in SchemaField.compare dereferenced a possibly-null
actual record directly. Guard it the same way ARRAY/MAP already do,
routing a missing nested record through expectedNullMismatch instead
of NPEing. Part of #58."
```

---

### Task 3: Full regression run

**Files:** none (verification only).

**Interfaces:**
- Consumes: the completed fixes and tests from Tasks 1 and 2.
- Produces: confirmation the whole plugin module test suite passes with both fixes applied together.

- [ ] **Step 1: Run the full plugin test suite**

Run: `sbt "plugin/test"`

Expected: BUILD SUCCESS, all tests pass, including the pre-existing `RecordImplicitsTest`, `RecordImplicitsArraysTest`, `RecordImplicitsMapsTest`, and the two new/modified suites from Tasks 1-2.

- [ ] **Step 2: Confirm no unrelated diffs**

Run: `git status --short` and `git diff main --stat` (or the base branch)

Expected: only the two files modified in Task 1/2 (`SchemaFieldImplicits.scala`, `RecordImplicitsTest.scala`) and the one file created in Task 2 (`RecordImplicitsRecordsTest.scala`) show up — no incidental changes.
