package com.github.austek.plugin.avro

import com.google.protobuf.struct.{ListValue, Struct, Value}
import io.pact.plugin.pact_plugin.*

import java.nio.file.{Files, Paths}
import scala.concurrent.Await
import scala.concurrent.duration.*

object GoldenFixtureGenerator {

  private case class Fixture(name: String, schemaFile: String, record: String, config: Map[String, Value])

  private def s(text: String): Value = Value(Value.Kind.StringValue(text))
  private def l(values: Value*): Value = Value(Value.Kind.ListValue(ListValue(values)))
  private def o(fields: (String, Value)*): Value = Value(Value.Kind.StructValue(Struct(fields.toMap)))

  private val fixtures = List(
    Fixture("item", "item.avsc", "Item", Map("name" -> s("notEmpty('Item-41')"), "id" -> s("notEmpty('41')"))),
    Fixture(
      "complex",
      "schemas.avsc",
      "Complex",
      Map(
        "id" -> s("notEmpty('100')"),
        "names" -> l(s("notEmpty('name-1')"), s("notEmpty('name-2')")),
        "enabled" -> s("matching(boolean, true)"),
        "no" -> s("matching(integer, 121)"),
        "height" -> s("matching(decimal, 15.8)"),
        "width" -> s("matching(decimal, 1.8)"),
        "ages" -> o("first" -> s("matching(integer, 2)"), "second" -> s("matching(integer, 3)")),
        "color" -> s("matching(equalTo, 'GREEN')"),
        "md5" -> s("matching(equalTo, 'abcd')"),
        "address" -> o("street" -> s("notEmpty('street name')")),
        "items" -> l(
          o("name" -> s("notEmpty('Item-1')"), "id" -> s("matching(integer, 1)")),
          o("name" -> s("notEmpty('Item-2')"), "id" -> s("matching(integer, 2)"))
        )
      )
    )
  )

  private def q(text: String): String =
    "\"" + text.flatMap {
      case '"'          => "\\\""
      case '\\'         => "\\\\"
      case c if c < ' ' => f"\\u${c.toInt}%04x"
      case c            => c.toString
    } + "\""

  private def json(value: Value): String = value.kind match {
    case Value.Kind.StringValue(t) => q(t)
    case Value.Kind.NumberValue(n) => if (n == n.toLong.toDouble) n.toLong.toString else n.toString
    case Value.Kind.BoolValue(b)   => b.toString
    case Value.Kind.ListValue(v)   => v.values.map(json).mkString("[", ",", "]")
    case Value.Kind.StructValue(v) => obj(v.fields)
    case _                         => "null"
  }

  private def obj(fields: Map[String, Value]): String =
    fields.toSeq.sortBy(_._1).map { case (k, v) => s"${q(k)}:${json(v)}" }.mkString("{", ",", "}")

  private def rulesJson(response: InteractionResponse): String =
    response.rules.toSeq
      .sortBy(_._1)
      .map { case (path, rules) =>
        val items = rules.rule.map(r => s"""{"type":${q(r.`type`)},"values":${obj(r.values.map(_.fields).getOrElse(Map.empty))}}""")
        s"${q(path)}:${items.mkString("[", ",", "]")}"
      }
      .mkString("{", ",", "}")

  private def render(fixture: Fixture, response: InteractionResponse): String = {
    val body = response.contents.get
    val hex = body.getContent.toByteArray.map("%02x".format(_)).mkString
    s"""{"name":${q(fixture.name)},"schema":${q(fixture.schemaFile)},"record":${q(fixture.record)},"config":${obj(fixture.config)},""" +
      s""""contentType":${q(body.contentType)},"bodyHex":${q(hex)},"rules":${rulesJson(response)}}"""
  }

  def main(args: Array[String]): Unit = {
    val outDir = Paths.get(args(0))
    Files.createDirectories(outDir)
    fixtures.foreach { fixture =>
      val schemaPath = Paths.get("src/test/resources", fixture.schemaFile).toAbsolutePath.toString
      val config = fixture.config ++ Map(
        "pact:avro" -> s(schemaPath),
        "pact:record-name" -> s(fixture.record),
        "pact:content-type" -> s("avro/binary")
      )
      val request = ConfigureInteractionRequest("avro/binary", Some(Struct(config)))
      val response = Await.result(new PactAvroPluginService().configureInteraction(request), 30.seconds)
      require(response.error.isEmpty, s"${fixture.name}: ${response.error}")
      Files.writeString(outDir.resolve(s"${fixture.name}.json"), render(fixture, response.interaction.head) + "\n")
    }
  }
}
