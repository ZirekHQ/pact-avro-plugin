package com.github.austek.plugin.avro.implicits

import au.com.dius.pact.core.model.PathExpressionsKt.*

object PathExpressionImplicits {

  implicit class RichListString(list: List[String]) {
    def constructPath: String = {
      list.foldLeft("") { case (path, segment) =>
        if (path.isEmpty) {
          segment
        } else {
          constructValidPath(segment, path, true)
        }
      }
    }
  }

}
