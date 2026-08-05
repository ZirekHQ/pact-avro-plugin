package com.github.austek.plugin.avro.implicits

import au.com.dius.pact.core.matchers.*
import com.github.austek.plugin.avro.error.{PluginError, PluginErrorException}
import com.github.austek.plugin.avro.implicits.PathExpressionImplicits.*
import com.github.austek.plugin.avro.matchers
import com.typesafe.scalalogging.StrictLogging
import org.apache.avro.generic.{GenericDatumWriter, GenericRecord}
import org.apache.avro.io.EncoderFactory

import java.io.ByteArrayOutputStream
import java.nio.charset.StandardCharsets
import scala.jdk.CollectionConverters.*
import scala.util.*

object RecordImplicits extends StrictLogging {

  implicit class RichAvroRecord(record: GenericRecord) {

    def toJson: Either[PluginErrorException, String] = {
      Using(new ByteArrayOutputStream()) { outputStream =>
        val writer = new GenericDatumWriter[GenericRecord](record.getSchema)
        val encoder = EncoderFactory.get().jsonEncoder(record.getSchema, outputStream, true)
        writer.write(record, encoder)
        encoder.flush()

        outputStream.toString(StandardCharsets.UTF_8)
      } match {
        case Success(value)     => Right(value)
        case Failure(exception) => Left(PluginErrorException(exception))
      }
    }

    def diff(other: GenericRecord): Either[PluginErrorException, String] = {
      for {
        recordJson <- record.toJson
        otherJson <- other.toJson
        result <- Right(DiffUtilsKt.generateDiff(recordJson, otherJson).asScala.mkString("\n"))
      } yield result
    }

    def toByteArray: Array[Byte] = {
      val outputStream = new ByteArrayOutputStream()
      val writer = new GenericDatumWriter[GenericRecord](record.getSchema)
      val encoder = EncoderFactory.get().binaryEncoder(outputStream, null)
      writer.write(record, encoder)
      encoder.flush()
      outputStream.toByteArray
    }

    // Deliberately untyped: casting to a concrete type would unbox a missing field's null to a primitive zero, or fail on Avro's Utf8 vs String.
    def valueOf(name: String): Any = record.get(name)

    def compare(
      path: List[String],
      other: GenericRecord
    )(implicit context: MatchingContext): Either[Seq[PluginError[?]], List[BodyItemMatchResult]] = {
      import SchemaFieldImplicits.*

      logger.debug(s">>> Record.compare($path, $record, $other)")
      if (record.getSchema.getName == other.getSchema.getName) {
        record.getSchema.getFields.asScala
          .map { field =>
            val fieldPath = path :+ field.name
            Try(other.getSchema.getField(field.name())) match {
              case Success(otherField) => field.compare(fieldPath, otherField, record, other)
              case Failure(_) =>
                matchers.BodyItemMatchResult
                  .mismatch(
                    record,
                    other,
                    diff => {
                      List(
                        new BodyItemMatchResult(
                          path.head,
                          List(
                            new BodyMismatch(
                              field.name,
                              null,
                              s"record field '${field.name}' but was missing",
                              fieldPath.constructPath,
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
          .partitionMap(identity) match {
          case (errors, _) if errors.nonEmpty => Left(errors.toSeq.flatten)
          case (_, result)                    => Right(result.flatten.toList)
        }
      } else {
        matchers.BodyItemMatchResult
          .mismatch(
            record,
            other,
            diff => {
              List(
                new BodyItemMatchResult(
                  path.head,
                  List(
                    new BodyMismatch(
                      record,
                      other,
                      s"Expected record '${record.getSchema.getName}' but got '${other.getSchema.getName}'",
                      "/",
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
  }

}
