package com.github.austek.plugin.avro.implicits

import au.com.dius.pact.core.matchers.{BodyMismatch, MatchingContext}
import au.com.dius.pact.core.model.matchingrules.MatchingRuleCategory
import com.github.austek.plugin.avro.Avro.AvroRecord
import com.github.austek.plugin.avro.TestSchemas.*
import RecordImplicits.*
import com.github.austek.plugin.avro.matchers.BodyItemMatchResult
import com.google.protobuf.struct.Value
import com.google.protobuf.struct.Value.Kind.*
import org.apache.avro.generic.GenericData
import org.apache.avro.util.Utf8
import org.scalatest.EitherValues
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpec

import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets

class RecordImplicitsTest extends AnyWordSpec with Matchers with EitherValues {

  "GenericRecord" when {
    "comparing String fields" should {
      val schema = schemaWithField("""{"name": "street", "type": "string"}""")
      val pactConfiguration: Map[String, Value] = Map("street" -> Value(StringValue("matching(equalTo, 'hello')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("street", "hello")

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.street", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("street", "other")

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.street",
            List(
              new BodyMismatch("hello", "other", "Expected 'other' (String) to be equal to 'hello' (String)", "$.street", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.street",
            List(
              new BodyMismatch("hello", null, "Expected null (Null) to be equal to 'hello' (String)", "$.street", "")
            )
          )
        )
      }
    }

    "comparing Int fields" should {
      val schema = schemaWithField("""{"name": "no", "type": "int"}""")
      val pactConfiguration: Map[String, Value] = Map("no" -> Value(StringValue("matching(equalTo, 121)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("no", 121)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.no", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("no", 3)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.no",
            List(
              new BodyMismatch(121, 3, "Expected 3 (Integer) to be equal to 121 (Integer)", "$.no", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.no",
            List(
              new BodyMismatch(121, null, "Expected null (Null) to be equal to 121 (Integer)", "$.no", "")
            )
          )
        )
      }
    }

    "comparing Long fields" should {
      val schema = schemaWithField("""{"name": "id", "type": "long"}""")
      val pactConfiguration: Map[String, Value] = Map("id" -> Value(StringValue("matching(equalTo, 121)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("id", 121L)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.id", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("id", 3L)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.id",
            List(
              new BodyMismatch(121L, 3L, "Expected 3 (Long) to be equal to 121 (Long)", "$.id", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.id",
            List(
              new BodyMismatch(121L, null, "Expected null (Null) to be equal to 121 (Long)", "$.id", "")
            )
          )
        )
      }
    }

    "comparing Double fields" should {
      val schema = schemaWithField("""{"name": "width", "type": "double"}""")
      val pactConfiguration: Map[String, Value] = Map("width" -> Value(StringValue("matching(equalTo, 1.8)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("width", 1.8d)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.width", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("width", 3d)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.width",
            List(
              new BodyMismatch(1.8d, 3d, "Expected 3.0 (Double) to be equal to 1.8 (Double)", "$.width", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.width",
            List(
              new BodyMismatch(1.8d, null, "Expected null (Null) to be equal to 1.8 (Double)", "$.width", "")
            )
          )
        )
      }
    }

    "comparing Float fields" should {
      val schema = schemaWithField("""{"name": "height", "type": "float"}""")
      val pactConfiguration: Map[String, Value] = Map("height" -> Value(StringValue("matching(equalTo, 1.8)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("height", 1.8f)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.height", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("height", 3f)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.height",
            List(
              new BodyMismatch(1.8f, 3f, "Expected 3.0 (Float) to be equal to 1.8 (Float)", "$.height", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.height",
            List(
              new BodyMismatch(1.8f, null, "Expected null (Null) to be equal to 1.8 (Float)", "$.height", "")
            )
          )
        )
      }
    }

    "comparing Boolean fields" should {
      val schema = schemaWithField("""{"name": "enabled", "type": "boolean"}""")
      val pactConfiguration: Map[String, Value] = Map("enabled" -> Value(StringValue("matching(equalTo, true)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("enabled", true)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.enabled", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("enabled", false)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.enabled",
            List(
              new BodyMismatch(true, false, "Expected false (Boolean) to be equal to true (Boolean)", "$.enabled", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.enabled",
            List(
              new BodyMismatch(true, null, "Expected null (Null) to be equal to true (Boolean)", "$.enabled", "")
            )
          )
        )
      }
    }

    "comparing Enum fields" should {
      val schema = schemaWithField("""{"name": "color", "type": {"type": "enum", "name": "Color", "symbols": [ "UNKNOWN", "GREEN", "RED"]}}""")
      val pactConfiguration: Map[String, Value] = Map("color" -> Value(StringValue("matching(equalTo, 'GREEN')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)
      val green = new GenericData.EnumSymbol(schema, "GREEN")

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("color", green)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.color", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        val red = new GenericData.EnumSymbol(schema, "RED")
        otherRecord.put("color", red)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.color",
            List(
              new BodyMismatch(green, red, "Expected RED (EnumSymbol) to be equal to GREEN (EnumSymbol)", "$.color", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.color",
            List(
              new BodyMismatch(green, null, "Expected null (Null) to be equal to GREEN (EnumSymbol)", "$.color", "")
            )
          )
        )
      }
    }

    "comparing Fixed fields" should {
      val schema = schemaWithField("""{"name": "md5", "type": {"name": "MD5", "type": "fixed", "size": 4}}""")
      val pactConfiguration: Map[String, Value] = Map("md5" -> Value(StringValue("matching(equalTo, '\\\u0000\\\u0001\\\u0002\\\u0003')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)
      val fixed = new GenericData.Fixed(schema.getField("md5").schema(), "\\\u0000\\\u0001\\\u0002\\\u0003".getBytes)

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("md5", fixed)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.md5", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        val red = new GenericData.Fixed(schema.getField("md5").schema(), "\\\u0000\\\u0001\\\u0002".getBytes)
        otherRecord.put("md5", red)

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.md5",
            List(
              new BodyMismatch(fixed, red, s"Expected $red (Fixed) to be equal to $fixed (Fixed)", "$.md5", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.md5",
            List(
              new BodyMismatch(fixed, null, s"Expected null (Null) to be equal to $fixed (Fixed)", "$.md5", "")
            )
          )
        )
      }
    }

    "comparing Bytes fields" should {
      val schema = schemaWithField("""{"name": "MAC", "type": "bytes"}""")
      val pactConfiguration: Map[String, Value] = Map("MAC" -> Value(StringValue("matching(equalTo, '\\\u0000')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val record = avroRecord.toGenericRecord(schema)

      val matchingRules: MatchingRuleCategory = avroRecord.matchingRules
      implicit val context: MatchingContext = new MatchingContext(matchingRules, false)

      val expectedBytes = ByteBuffer.wrap("\\\u0000".getBytes(StandardCharsets.UTF_8))
      val otherBytes = ByteBuffer.wrap("\\\u0001".getBytes(StandardCharsets.UTF_8))

      "return empty BodyMatch list for equal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("MAC", expectedBytes.duplicate())

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(BodyItemMatchResult("$.MAC", List()))
      }

      "return a BodyMatch for unequal fields" in {
        val otherRecord = new GenericData.Record(schema)
        otherRecord.put("MAC", otherBytes.duplicate())

        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.MAC",
            List(
              new BodyMismatch(expectedBytes, otherBytes, s"Expected $otherBytes (HeapByteBuffer) to be equal to $expectedBytes (HeapByteBuffer)", "$.MAC", "")
            )
          )
        )
      }

      "return a BodyMatch for missing field value" in {
        val otherRecord = new GenericData.Record(schema)
        val result = record.compare(List("$"), otherRecord).value
        result should have size 1
        result shouldBe List(
          BodyItemMatchResult(
            "$.MAC",
            List(
              new BodyMismatch(expectedBytes, null, s"Expected null (Null) to be equal to $expectedBytes (HeapByteBuffer)", "$.MAC", "")
            )
          )
        )
      }
    }

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
  }
}
