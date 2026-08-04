package com.github.austek.plugin.avro.compare

import au.com.dius.pact.core.matchers.*
import com.github.austek.plugin.avro.error.PluginError
import com.github.austek.plugin.avro.implicits.RecordImplicits.*
import com.typesafe.scalalogging.StrictLogging
import org.apache.avro.generic.GenericRecord

import scala.jdk.CollectionConverters.*

object AvroContentMatcher extends StrictLogging {

  def compare(expected: GenericRecord, actual: GenericRecord, context: MatchingContext): Either[Seq[PluginError[?]], BodyMatchResult] =
    expected.compare(List("$"), actual)(using context).map { result =>
      new BodyMatchResult(null, result.asJava)
    }
}
