package com.github.austek.plugin.avro

import au.com.dius.pact.core.model.matchingrules.*
import au.com.dius.pact.core.model.matchingrules.NumberTypeMatcher.NumberType
import com.github.austek.plugin.avro.Avro.AvroRecord
import com.github.austek.plugin.avro.TestSchemas.*
import com.github.austek.plugin.avro.utils.MatchingRuleCategoryImplicits.*
import com.google.protobuf.struct.Value.Kind.*
import com.google.protobuf.struct.{ListValue as StructListValue, Struct, Value}
import org.apache.avro.Schema.Type.NULL
import org.apache.avro.generic.{GenericData, GenericDatumReader, GenericRecord}
import org.apache.avro.io.DecoderFactory
import org.scalatest.EitherValues
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpecLike

import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets
import java.util
import scala.jdk.CollectionConverters.*

class AvroRecordComplexTypesTest extends AnyWordSpecLike with Matchers with EitherValues {
  import com.github.austek.plugin.avro.utils.MatchingRuleCategoryImplicits.given
  def provide: AfterWord = afterWord("provide")

  "Enum field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "color", "type": {"type": "enum", "name": "Color", "symbols": [ "UNKNOWN", "GREEN", "RED"]}}""")
      val pactConfiguration: Map[String, Value] = Map("color" -> Value(StringValue("matching(equalTo, 'GREEN')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("color") shouldBe new GenericData.EnumSymbol(schema, "GREEN")
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.color")
          rules shouldBe Seq(EqualsMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema =
        schemaWithField("""{"name": "color", "type": {"type": "enum", "name": "Color", "symbols": [ "UNKNOWN", "GREEN", "RED"]}, "default": "UNKNOWN"}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("color") shouldBe new GenericData.EnumSymbol(schema, "UNKNOWN")
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Fixed field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "md5", "type": {"name": "MD5", "type": "fixed", "size": 4}}""")
      val pactConfiguration: Map[String, Value] = Map("md5" -> Value(StringValue("matching(equalTo, '\\\u0000\\\u0001\\\u0002\\\u0003')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("md5") shouldBe new GenericData.Fixed(schema.getField("md5").schema(), "\\\u0000\\\u0001\\\u0002\\\u0003".getBytes)
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.md5")
          rules shouldBe Seq(EqualsMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "md5", "type": {"name": "MD5", "type": "fixed", "size": 4}, "default": "\\u0000"}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("md5") shouldBe new GenericData.Fixed(schema.getField("md5").schema(), "\\u0000".getBytes)
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Array simple type field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "names", "type": {"type": "array", "items": "string"}}""")
      val pactConfiguration: Map[String, Value] = Map(
        "names" -> Value(
          ListValue(
            StructListValue(
              Seq(
                Value(StringValue("notEmpty('name-1')")),
                Value(StringValue("notEmpty('name-2')"))
              )
            )
          )
        )
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("names").asInstanceOf[util.List[String]] should contain theSameElementsAs List("name-1", "name-2")
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 2
          avroRecord.matchingRules.getRules("$.names.0") shouldBe List(NotEmptyMatcher.INSTANCE)
          avroRecord.matchingRules.getRules("$.names.1") shouldBe List(NotEmptyMatcher.INSTANCE)
        }
      }
    }
  }

  "Array complex type field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{
          |  "name": "addresses",
          |  "type": {
          |    "type": "array",
          |    "items": {
          |      "name": "address",
          |      "type": "record",
          |      "fields": [
          |        { "name": "street", "type": "string" }
          |      ]
          |    }
          |  }
          |}""".stripMargin)
      val pactConfiguration: Map[String, Value] = Map(
        "addresses" -> Value(
          ListValue(
            StructListValue(
              Seq(
                Value(StructValue(Struct(Map("street" -> Value(StringValue("matching(equalTo, 'street name')"))))))
              )
            )
          )
        )
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val addressSchema = schema.getField("addresses").schema().getElementType
      val addressRecord = new GenericData.Record(addressSchema)
      addressRecord.put("street", "street name")

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("addresses").asInstanceOf[util.List[GenericRecord]] should contain theSameElementsAs List(addressRecord)
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          avroRecord.matchingRules.getRules("$.addresses.0.street") shouldBe List(EqualsMatcher.INSTANCE)
        }
      }
    }
  }

  "Map simple type field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "ages","type": { "type": "map", "values": "int"}}""")
      val pactConfiguration: Map[String, Value] = Map(
        "ages" -> Value(
          StructValue(
            Struct(
              Map(
                "first" -> Value(StringValue("matching(integer, 2)")),
                "second" -> Value(StringValue("matching(integer, 3)"))
              )
            )
          )
        )
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("ages").asInstanceOf[util.Map[String, Int]].asScala should contain theSameElementsAs Map("first" -> 2, "second" -> 3)
          avroRecord.value should have size 1
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 2
          avroRecord.matchingRules.getRules("$.ages.first") shouldBe List(new NumberTypeMatcher(NumberType.INTEGER))
          avroRecord.matchingRules.getRules("$.ages.second") shouldBe List(new NumberTypeMatcher(NumberType.INTEGER))
        }
      }
    }
  }

  "Map bytes type field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "hashes","type": { "type": "map", "values": "bytes"}}""")
      val pactConfiguration: Map[String, Value] = Map(
        "hashes" -> Value(
          StructValue(
            Struct(
              Map(
                "sha-256" -> Value(StringValue("matching(equalTo, 'abc123')"))
              )
            )
          )
        )
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "can be encoded to avro binary and decoded back to the same bytes" in {
          val bytes = avroRecord.toByteString(schema).value.toByteArray
          val reader = new GenericDatumReader[GenericRecord](schema)
          val decoder = DecoderFactory.get.binaryDecoder(bytes, null)
          val decoded = reader.read(null, decoder)
          val decodedHashes = decoded.get("hashes").asInstanceOf[util.Map[CharSequence, ByteBuffer]].asScala.map { case (k, v) => k.toString -> v }
          decodedHashes should contain theSameElementsAs Map(
            "sha-256" -> ByteBuffer.wrap("abc123".getBytes(StandardCharsets.UTF_8))
          )
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          avroRecord.matchingRules.getRules("$.hashes.sha-256") shouldBe List(EqualsMatcher.INSTANCE)
        }
      }
    }
  }

  "Map complex type field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{
          |  "name": "addresses",
          |  "type": {
          |    "type": "map",
          |    "values": {
          |      "name": "address",
          |      "type": "record",
          |      "fields": [
          |        { "name": "street", "type": "string" }
          |      ]
          |    }
          |  }
          |}""".stripMargin)
      val pactConfiguration: Map[String, Value] = Map(
        "addresses" -> Value(
          StructValue(
            Struct(
              Map(
                "first" -> Value(StructValue(Struct(Map("street" -> Value(StringValue("matching(equalTo, 'first street')")))))),
                "second" -> Value(StructValue(Struct(Map("street" -> Value(StringValue("matching(equalTo, 'second street')"))))))
              )
            )
          )
        )
      )
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val addressSchema = schema.getField("addresses").schema().getValueType
      val firstAddressRecord = new GenericData.Record(addressSchema)
      firstAddressRecord.put("street", "first street")
      val secondAddressRecord = new GenericData.Record(addressSchema)
      secondAddressRecord.put("street", "second street")

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("addresses").asInstanceOf[util.Map[String, GenericRecord]].asScala should contain theSameElementsAs Map(
            "first" -> firstAddressRecord,
            "second" -> secondAddressRecord
          )
          avroRecord.value should have size 1
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 2
          avroRecord.matchingRules.getRules("$.addresses.first.street") shouldBe List(EqualsMatcher.INSTANCE)
          avroRecord.matchingRules.getRules("$.addresses.second.street") shouldBe List(EqualsMatcher.INSTANCE)
        }
      }
    }
  }

  "Optional Complex field" when {
    val schema = schemaWithField("""{
                                   |  "name": "address",
                                   |  "type": [ "null",
                                   |    {
                                   |      "name": "Address",
                                   |      "type": "record",
                                   |      "fields": [
                                   |        { "name": "street", "type": "string" }
                                   |      ]
                                   |    }
                                   |  ]
                                   |}""".stripMargin)
    "value provided" should provide {
      val pactConfiguration = Map("address" -> Value(StructValue(Struct(Map("street" -> Value(StringValue("matching(equalTo, 'first street')")))))))
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val addressSchema = schema.getField("address").schema().getTypes.asScala.filterNot(_.getType == NULL).head
      val addressRecord = new GenericData.Record(addressSchema)
      addressRecord.put("street", "first street")

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("address") shouldBe addressRecord
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          avroRecord.matchingRules.getRules("$.address.street") shouldBe List(EqualsMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{
                                     |  "name": "address",
                                     |  "type": [ "null",
                                     |    {
                                     |      "name": "Address",
                                     |      "type": "record",
                                     |      "fields": [
                                     |        { "name": "street", "type": "string", "default": "first street" }
                                     |      ]
                                     |    }
                                     |  ],
                                     |  "default": null
                                     |}""".stripMargin)
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value
      val addressSchema = schema.getField("address").schema().getTypes.asScala.filterNot(_.getType == NULL).head
      val addressRecord = new GenericData.Record(addressSchema)
      addressRecord.put("street", "first street")

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("address") shouldBe null
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }

    "value not provided" should provide {
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("address") shouldBe null
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }
}
