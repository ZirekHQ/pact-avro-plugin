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
