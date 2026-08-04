package com.github.austek.plugin.avro

import au.com.dius.pact.core.model.matchingrules.*
import au.com.dius.pact.core.model.matchingrules.NumberTypeMatcher.NumberType
import com.github.austek.plugin.avro.Avro.AvroRecord
import com.github.austek.plugin.avro.TestSchemas.*
import com.github.austek.plugin.avro.utils.MatchingRuleCategoryImplicits.*
import com.google.protobuf.struct.Value
import com.google.protobuf.struct.Value.Kind.*
import org.apache.avro.generic.{GenericDatumReader, GenericRecord}
import org.apache.avro.io.DecoderFactory
import org.scalatest.EitherValues
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpecLike

import java.nio.charset.StandardCharsets

class AvroRecordPrimitiveTypesTest extends AnyWordSpecLike with Matchers with EitherValues {
  import com.github.austek.plugin.avro.utils.MatchingRuleCategoryImplicits.given

  private val byteValue = "\\\u0000\\\u0001\\\u0002\\\u0003\\\u0004\\\u0005\\\u0006\\\u0007"
  def provide: AfterWord = afterWord("provide")

  private def decodeRecord(schema: org.apache.avro.Schema, bytes: Array[Byte]): GenericRecord = {
    val reader = new GenericDatumReader[GenericRecord](schema)
    val decoder = DecoderFactory.get.binaryDecoder(bytes, null)
    reader.read(null, decoder)
  }

  "String field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "street", "type": "string"}""")
      val pactConfiguration: Map[String, Value] = Map("street" -> Value(StringValue("matching(equalTo, 'hello')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("street") shouldBe "hello"
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.street")
          rules shouldBe Seq(EqualsMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "street", "type": "string", "default": "NONE"}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("street") shouldBe "NONE"
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Integer field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "no", "type": "int"}""")
      val pactConfiguration: Map[String, Value] = Map("no" -> Value(StringValue("matching(integer, 121)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("no").toString.toInt shouldBe 121
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.no")
          rules shouldBe Seq(new NumberTypeMatcher(NumberType.INTEGER))
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "no", "type": "int", "default": 5}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("no").toString.toInt shouldBe 5
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Long field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "id", "type": "long"}""")
      val pactConfiguration: Map[String, Value] = Map("id" -> Value(StringValue("notEmpty('100')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("id").toString.toInt shouldBe 100
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.id")
          rules shouldBe Seq(NotEmptyMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "id", "type": "long", "default": 100}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("id").toString.toInt shouldBe 100
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Double field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "width", "type": "double"}""")
      val pactConfiguration: Map[String, Value] = Map("width" -> Value(StringValue("matching(decimal, 1.8)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("width").toString.toDouble shouldBe 1.8d
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.width")
          rules shouldBe Seq(new NumberTypeMatcher(NumberType.DECIMAL))
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "width", "type": "double", "default": 1.8}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("width").toString.toDouble shouldBe 1.8d
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Float field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "height", "type": "float"}""")
      val pactConfiguration: Map[String, Value] = Map("height" -> Value(StringValue("matching(decimal, 15.8)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("height").toString.toFloat shouldBe 15.8f
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.height")
          rules shouldBe Seq(new NumberTypeMatcher(NumberType.DECIMAL))
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "height", "type": "float", "default": 15.8}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("height").toString.toFloat shouldBe 15.8f
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Boolean field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "enabled", "type": "boolean"}""")
      val pactConfiguration: Map[String, Value] = Map("enabled" -> Value(StringValue("matching(boolean, true)")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("enabled").toString.toBoolean shouldBe true
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.enabled")
          rules shouldBe Seq(BooleanMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "enabled", "type": "boolean", "default": true}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("enabled").toString.toBoolean shouldBe true
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Bytes field" when {
    "value provided" should provide {
      val schema = schemaWithField("""{"name": "MAC", "type": "bytes"}""")
      val pactConfiguration: Map[String, Value] = Map("MAC" -> Value(StringValue(s"matching(equalTo, '$byteValue')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("MAC") shouldBe java.nio.ByteBuffer.wrap(byteValue.getBytes(StandardCharsets.UTF_8))
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.MAC")
          rules shouldBe Seq(EqualsMatcher.INSTANCE)
        }
        "can be encoded to avro binary and decoded back to the same bytes" in {
          val bytes = avroRecord.toByteString(schema).value.toByteArray
          val decoded = decodeRecord(schema, bytes)
          decoded.get("MAC") shouldBe java.nio.ByteBuffer.wrap(byteValue.getBytes(StandardCharsets.UTF_8))
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "MAC", "type": "bytes", "default": "\\u0000"}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("MAC") shouldBe java.nio.ByteBuffer.wrap("\\u0000".getBytes(StandardCharsets.UTF_8))
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }

  "Optional Primitive field" when {
    val schema = schemaWithField("""{"name": "MAC", "type": ["null", "bytes"]}""")
    "value provided" should provide {
      val pactConfiguration: Map[String, Value] = Map("MAC" -> Value(StringValue(s"matching(equalTo, '$byteValue')")))
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("MAC") shouldBe java.nio.ByteBuffer.wrap(byteValue.getBytes(StandardCharsets.UTF_8))
        }
        "returns matching rules using JsonPath" in {
          avroRecord.matchingRules should have size 1
          val rules = avroRecord.matchingRules.getRules("$.MAC")
          rules shouldBe Seq(EqualsMatcher.INSTANCE)
        }
      }
    }

    "value not provided but has default" should provide {
      val schema = schemaWithField("""{"name": "MAC", "type": ["null", "bytes"], "default": null}""")
      val pactConfiguration: Map[String, Value] = Map()
      val avroRecord = AvroRecord(schema, pactConfiguration).value

      "a method," which {
        "returns GenericRecord with field containing default value" in {
          val genericRecord = avroRecord.toGenericRecord(schema)
          genericRecord.get("MAC") shouldBe null
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
          genericRecord.get("MAC") shouldBe null
        }
        "returns empty matching rules using JsonPath" in {
          avroRecord.matchingRules shouldBe empty
        }
      }
    }
  }
}
