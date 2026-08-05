# Fix Utf8/String equality asymmetry and missing-nested-record NPE (#58)

## Context

Two pre-existing bugs in `SchemaFieldImplicits`/`RecordImplicits`, found while investigating #57 (fixed by removing a fragile generic cast in `RecordImplicits.valueOf`). Neither is caused by that fix; both are narrow enough in the live path that they haven't surfaced as bug reports, but both are real defects with no test coverage.

Source: `modules/plugin/src/main/scala/com/github/austek/plugin/avro/implicits/SchemaFieldImplicits.scala`

## Fix 1: Utf8/String equality asymmetry

`compareValue`'s plain-equality branch (no matcher defined, ~line 293) compares `expected == actual` directly. The matcher-defined branch (~line 269) normalizes `Utf8 -> String` first. A `Utf8` compared against a `String` with equal text fails equality in the plain branch only.

**Fix:** extract the existing `case s: Utf8 => s.toString` normalization into a small shared private helper, and apply it in both branches of `compareValue` before the equality check. Mismatch messages continue to use the original `expected`/`actual` values — `Utf8.toString` and `String` render identically, so this is a comparison-only fix with no message-text change.

## Fix 2: Missing nested record NPE

`SchemaField.compare`'s `RECORD` case (~line 35) does:

```scala
expected.get(fieldName).asInstanceOf[GenericRecord].compare(path, actual.get(fieldName).asInstanceOf[GenericRecord])
```

If the nested record is missing on the `actual` side, `.get` returns `null`, and `RichAvroRecord(null).compare(...)` NPEs on `record.getSchema` instead of producing a mismatch result, unlike the scalar/array/map cases.

**Fix:** mirror the existing array/map convention exactly — only `actual`'s fetch is `Option`-wrapped (`expected` is assumed always present, per the established pattern in `compareArrayField`/`compareMapField`). On `None`, return `Right(expectedNullMismatch(path, expectedRecord, "Record"))`, reusing the existing private `expectedNullMismatch` helper already used by the array/map `None` branches.

```scala
case RECORD =>
  val fieldName = path.last
  val expectedRecord = expected.get(fieldName).asInstanceOf[GenericRecord]
  Option(actual.get(fieldName).asInstanceOf[GenericRecord]) match {
    case Some(actualRecord) => expectedRecord.compare(path, actualRecord)
    case None                => Right(expectedNullMismatch(path, expectedRecord, "Record"))
  }
```

Guarding `expected` as well was considered and rejected: it would diverge from the existing array/map convention and add an untested case (nullable nested record in the pact config itself) not required by the issue.

## Tests

- New `RecordImplicitsRecordsTest.scala`, mirroring the "Array fields with Record values" section of `RecordImplicitsArraysTest.scala` but for a plain (non-array) nested-record field: equal fields, unequal fields, and missing nested record (the last currently NPEs — this is the regression test for Fix 2).
- A test exercising `compareValue`'s plain-equality branch with one side `Utf8` and the other `String`, for Fix 1. This branch currently has zero coverage — every existing test in this test package goes through a `matching(...)` or `notEmpty(...)` rule, so `matcherDefined` is always true. Getting a field with no matching rule registered (so the equality branch actually runs) is TDD work for the implementation phase, not pinned down here.

## Scope

Both fixes ship in a single PR — same file, same GitHub issue, both small and semantic (not mechanical), so splitting would be pure overhead.
