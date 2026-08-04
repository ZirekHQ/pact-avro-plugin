package com.github.austek.plugin.avro.implicits

import au.com.dius.pact.core.matchers.{BodyItemMatchResult => AvroBodyItemMatchResult, _}
import au.com.dius.pact.core.model.PathExpressionsKt.*
import com.github.austek.plugin.avro.error.{PluginError, PluginErrorException, PluginErrorMessage, PluginErrorMessages}
import com.github.austek.plugin.avro.implicits.PathExpressionImplicits.*
import com.github.austek.plugin.avro.implicits.RecordImplicits.*
import com.github.austek.plugin.avro.implicits.SchemaTypeImplicits.*
import com.github.austek.plugin.avro.matchers.{BodyItemMatchResult, BodyMismatch}
import com.typesafe.scalalogging.StrictLogging
import org.apache.avro.Schema
import org.apache.avro.Schema.Type.*
import org.apache.avro.generic.GenericData.{EnumSymbol, Fixed}
import org.apache.avro.generic.GenericRecord
import org.apache.avro.util.Utf8

import java.nio.ByteBuffer
import java.util
import scala.jdk.CollectionConverters.*

object SchemaFieldImplicits extends StrictLogging {

  implicit class SchemaField(field: Schema.Field) {

    def compare(path: List[String], other: Schema.Field, expected: GenericRecord, actual: GenericRecord)(implicit
      context: MatchingContext
    ): Either[Seq[PluginError[?]], List[AvroBodyItemMatchResult]] = {
      (field.schema().getType, other.schema().getType) match {
        case (expectedType, actualType) if expectedType == actualType =>
          expectedType match {
            case STRING =>
              Right(compareValue(path, field, expected.valueOf[String](field.name()), actual.valueOf[String](field.name()), () => "", context))
            case BYTES =>
              Right(
                compareValue(path, field, expected.valueOf[ByteBuffer](field.name()), actual.valueOf[ByteBuffer](field.name()), () => "", context)
              )
            case INT     => Right(compareValue(path, field, expected.valueOf[Int](field.name()), actual.valueOf[Int](field.name()), () => "", context))
            case LONG    => Right(compareValue(path, field, expected.valueOf[Long](field.name()), actual.valueOf[Long](field.name()), () => "", context))
            case FLOAT   => Right(compareValue(path, field, expected.valueOf[Float](field.name()), actual.valueOf[Float](field.name()), () => "", context))
            case DOUBLE  => Right(compareValue(path, field, expected.valueOf[Double](field.name()), actual.valueOf[Double](field.name()), () => "", context))
            case BOOLEAN => Right(compareValue(path, field, expected.valueOf[Boolean](field.name()), actual.valueOf[Boolean](field.name()), () => "", context))
            case ENUM =>
              Right(compareValue(path, field, expected.valueOf[EnumSymbol](field.name()), actual.valueOf[EnumSymbol](field.name()), () => "", context))
            case FIXED => Right(compareValue(path, field, expected.valueOf[Fixed](field.name()), actual.valueOf[Fixed](field.name()), () => "", context))
            case ARRAY => Right(compareArrayField(path, expected, actual, context))
            case MAP   => Right(compareMapField(path, expected, actual, context))
            case RECORD =>
              val fieldName = path.last
              expected.get(fieldName).asInstanceOf[GenericRecord].compare(path, actual.get(fieldName).asInstanceOf[GenericRecord])
            case t =>
              logger.warn(s"Field.compare doesn't support type: $t")
              Right(List.empty)
          }
        case _ =>
          BodyItemMatchResult
            .mismatch(
              field,
              other,
              diff => {
                List(
                  new AvroBodyItemMatchResult(
                    path.constructPath,
                    List(
                      BodyMismatch(
                        field,
                        other,
                        s"Expected field '${field.name()}' to be type '${field.schema.getType}' but got '${other.schema().getType}'",
                        path.constructPath,
                        diff
                      )
                    ).asJava
                  )
                )
              }
            )
            .left
            .map(e => Seq(e))
      }
    }

    private def compareArrayField(
      path: List[String],
      expected: GenericRecord,
      actual: GenericRecord,
      context: MatchingContext
    ): List[AvroBodyItemMatchResult] = {
      val clazz: Class[?] = field.schema().getElementType.getType.asJava
      val expectedList = expected.get(field.name()).asInstanceOf[util.List[clazz.type]]
      logger.debug(s">>> compareArrayField($path, $field, $expectedList)")
      Option(actual.get(field.name()).asInstanceOf[util.List[clazz.type]]) match {
        case Some(actualList) => compareArrayFieldValues[clazz.type](field, path, context, expectedList, actualList)
        case None             => expectedNullMismatch(path, expectedList, "Array")
      }
    }

    private def compareArrayFieldValues[T](
      field: Schema.Field,
      path: List[String],
      context: MatchingContext,
      expectedList: util.List[T],
      actualList: util.List[T]
    ): List[AvroBodyItemMatchResult] = {
      if (expectedList.isEmpty && !actualList.isEmpty) {
        List(
          BodyItemMatchResult(
            path.constructPath,
            List(
              BodyMismatch(
                expectedList,
                actualList,
                s"Expected repeated field '${field.name}' to be empty but received $actualList",
                path.constructPath,
                null
              )
            )
          )
        )
      } else if (context.matcherDefined(path.asJava)) {
        val ruleGroup = context.selectBestMatcher(path.asJava)
        logger.debug(s"compareArrayField: Matcher defined for path $path")
        ruleGroup.getRules.asScala.flatMap { matcher =>
          Matchers.INSTANCE
            .compareLists[T](
              path.asJava,
              matcher,
              expectedList,
              actualList,
              context,
              () => "",
              ruleGroup.getCascaded,
              (p, expectedValue, actualValue, c) => compareValue(p.asScala.toList, field, expectedValue, actualValue, () => "", c).asJava
            )
            .asScala
        }.toList
      } else
        List(
          Matchers.INSTANCE
            .compareListContent[T](
              expectedList,
              actualList,
              path.asJava,
              context,
              () => "",
              (p, expectedValue, actualValue, c) => {
                expectedValue match {
                  case v: GenericRecord =>
                    v.compare(p.asScala.toList, actualValue.asInstanceOf[GenericRecord])(using c) match {
                      case Left(_)      => List.empty.asJava
                      case Right(value) => value.asJava
                    }
                  case _ =>
                    compareValue(p.asScala.toList, field, expectedValue, actualValue, () => "", c).asJava
                }
              }
            )
            .asScala
            .toList
        ).flatten ++
          (if (expectedList.size != actualList.size) {
             List(
               BodyItemMatchResult(
                 path.constructPath,
                 List(
                   BodyMismatch(
                     expectedList,
                     actualList,
                     s"Expected repeated field '${field.name}' to have ${expectedList.size} values but received ${actualList.size} values",
                     path.constructPath,
                     null
                   )
                 )
               )
             )
           } else Nil)
    }

    private def compareMapField(
      path: List[String],
      expected: GenericRecord,
      actual: GenericRecord,
      context: MatchingContext
    ): List[AvroBodyItemMatchResult] = {
      val clazz: Class[?] = field.schema().getValueType.getType.asJava
      val expectedEntries = expected.get(field.name()).asInstanceOf[util.Map[String, clazz.type]]
      logger.debug(s">>> compareArrayField($path, $field, $expectedEntries)")
      Option(actual.get(field.name()).asInstanceOf[util.Map[String, clazz.type]]) match {
        case Some(actualList) if expectedEntries.isEmpty && !actualList.isEmpty =>
          compareMapFieldValues[clazz.type](field, path, context, expectedEntries, actualList)
        case Some(actualList) => compareMapFieldValues[clazz.type](field, path, context, expectedEntries, actualList)
        case None             => expectedNullMismatch(path, expectedEntries, "Map")
      }
    }

    private def compareMapFieldValues[V](
      field: Schema.Field,
      path: List[String],
      context: MatchingContext,
      expectedEntries: util.Map[String, V],
      actualEntries: util.Map[String, V]
    ): List[AvroBodyItemMatchResult] = {
      if (expectedEntries.isEmpty && !actualEntries.isEmpty) {
        List(
          BodyItemMatchResult(
            path.constructPath,
            List(
              BodyMismatch(
                expectedEntries,
                actualEntries,
                s"Expected Map field '${field.name}' to be empty but received $actualEntries",
                path.constructPath,
                null
              )
            )
          )
        )
      } else if (context.matcherDefined(path.asJava)) {
        logger.debug(s"compareMapField: matcher defined for path $path")
        context
          .selectBestMatcher(path.asJava)
          .getRules
          .asScala
          .flatMap { matcher =>
            Matchers.INSTANCE
              .compareMaps[V](
                path.asJava,
                matcher,
                expectedEntries,
                actualEntries,
                context,
                () => "",
                (p, expected, actual, c) => compareValue(p.asScala.toList, field, expected, actual, () => "", c).asJava
              )
              .asScala
          }
          .toList
      } else {
        logger.debug(s"compareMapField: no matcher defined for path ${path.constructPath}")
        logger.debug(s"                   expected keys ${expectedEntries.keySet()}")
        logger.debug(s"                   actual keys ${actualEntries.keySet()}")
        context.matchKeys(path.asJava, expectedEntries, actualEntries, () => "").asScala.toList ++
          expectedEntries.asScala.flatMap {
            case (key, value: GenericRecord) if actualEntries.containsKey(key) =>
              value.compare(path :+ key, actualEntries.get(key).asInstanceOf[GenericRecord])(using context) match {
                case Right(result) => result
                case Left(errors) =>
                  errors.foreach {
                    case PluginErrorMessage(value)       => logger.error(value)
                    case PluginErrorMessages(values)     => values.foreach(v => logger.error(v))
                    case PluginErrorException(exception) => logger.error("Failed to compare map field", exception)
                  }
                  Nil
              }
            case (key, value) if actualEntries.containsKey(key) =>
              compareValue(path :+ key, field, value, actualEntries.get(key), () => "", context)
            case (key, value) =>
              List(
                BodyItemMatchResult(
                  path.constructPath,
                  List(
                    BodyMismatch
                      .expectedNullMismatch[V](value, s"Expected map field '${field.name}' to have entry '$key', but was missing", path.constructPath, null)
                  )
                )
              )

          }
      }
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
  }

  private def expectedNullMismatch[T](path: List[String], expected: T, valueType: String): List[AvroBodyItemMatchResult] =
    List(
      BodyItemMatchResult(
        path.constructPath,
        List(BodyMismatch.expectedNullMismatch[T](expected, s"Expected null (Null) to be equal to '$expected' ($valueType)", path.constructPath))
      )
    )
}
