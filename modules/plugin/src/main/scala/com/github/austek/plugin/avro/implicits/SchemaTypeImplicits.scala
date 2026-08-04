package com.github.austek.plugin.avro.implicits

import org.apache.avro.Schema.Type
import org.apache.avro.Schema.Type.*
import org.apache.avro.generic.GenericRecord

import java.nio.ByteBuffer

object SchemaTypeImplicits {

  implicit class RichType(schemaType: Type) {
    def asJava: Class[?] = {
      schemaType match {
        case STRING  => classOf[String]
        case BYTES   => classOf[ByteBuffer]
        case INT     => classOf[Int]
        case LONG    => classOf[Long]
        case FLOAT   => classOf[Float]
        case DOUBLE  => classOf[Double]
        case BOOLEAN => classOf[Boolean]
        case ENUM    => classOf[String]
        case FIXED   => classOf[Array[Byte]]
        case ARRAY   => classOf[List[?]]
        case MAP     => classOf[Map[?, ?]]
        case RECORD  => classOf[GenericRecord]
        case _       => classOf[Object]
      }
    }
  }
}
