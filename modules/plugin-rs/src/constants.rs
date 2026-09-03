// Mirrors AvroPactConstants.scala
pub const RECORD_NAME: &str = "record-name";

// Mirrors AvroPluginConstants.scala
pub const AVRO_SCHEMA: &str = "avroSchema";
pub const RECORD: &str = "record";
pub const SCHEMA_KEY: &str = "schemaKey";
pub const MATCHING_RULE_CATEGORY_NAME: &str = "body";

// Mirrors ContentTypeConstants.scala
pub const CONTENT_TYPE_APPLICATION_AVRO: &str = "application/avro";
pub const CONTENT_TYPE_AVRO_BYTES: &str = "avro/bytes";
pub const CONTENT_TYPE_AVRO_BINARY: &str = "avro/binary";
pub const CONTENT_TYPE_AVRO_WILDCARD: &str = "application/*+avro";
pub const CONTENT_TYPES: [&str; 4] = [
    CONTENT_TYPE_APPLICATION_AVRO,
    CONTENT_TYPE_AVRO_BYTES,
    CONTENT_TYPE_AVRO_BINARY,
    CONTENT_TYPE_AVRO_WILDCARD,
];
pub const CONTENT_TYPES_STR: &str = "application/avro;avro/bytes;avro/binary;application/*+avro";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_str_joins_all_content_types_with_semicolons() {
        assert_eq!(CONTENT_TYPES_STR, CONTENT_TYPES.join(";"));
    }
}
