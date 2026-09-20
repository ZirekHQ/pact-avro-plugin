use crate::avro::schema::SchemaCtx;
use crate::error::PluginError;
use apache_avro::reader::datum::GenericDatumReader;
use apache_avro::schema::Schema;
use apache_avro::types::Value;
use apache_avro::writer::datum::GenericDatumWriter;

pub fn encode<'a>(
    ctx: &SchemaCtx<'a>,
    record: &'a Schema,
    value: Value,
) -> Result<Vec<u8>, PluginError> {
    GenericDatumWriter::builder(record)
        .schemata(vec![ctx.root()])
        .and_then(|builder| builder.build())
        .and_then(|writer| writer.write_value_to_vec(value))
        .map_err(|error| PluginError::Exception(error.to_string()))
}

pub fn decode<'a>(
    ctx: &SchemaCtx<'a>,
    record: &'a Schema,
    bytes: &[u8],
) -> Result<Value, PluginError> {
    GenericDatumReader::builder(record)
        .writer_schemata(vec![ctx.root()])
        .and_then(|builder| builder.build())
        .and_then(|reader| reader.read_value(&mut &bytes[..]))
        .map_err(|error| {
            tracing::error!("Failed to deserialize avro schema: {error}");
            PluginError::Message("Failed to deserialize avro schema".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_str;

    fn item_schema() -> Schema {
        parse_str(
            r#"{"type":"record","name":"Item","fields":[
                 {"name":"name","type":"string"},{"name":"id","type":"long"}]}"#,
        )
        .unwrap()
    }

    fn item(name: &str, id: i64) -> Value {
        Value::Record(vec![
            ("name".into(), Value::String(name.into())),
            ("id".into(), Value::Long(id)),
        ])
    }

    #[test]
    fn encodes_to_the_avro_binary_layout() {
        let schema = item_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let bytes = encode(&ctx, &schema, item("hello", 3)).unwrap();
        assert_eq!(bytes, [0x0a, b'h', b'e', b'l', b'l', b'o', 0x06]);
    }

    #[test]
    fn decode_reverses_encode() {
        let schema = item_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let bytes = encode(&ctx, &schema, item("hello", 3)).unwrap();
        assert_eq!(decode(&ctx, &schema, &bytes).unwrap(), item("hello", 3));
    }

    #[test]
    fn decode_of_garbage_reports_the_fixed_message() {
        let schema = item_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert_eq!(
            decode(&ctx, &schema, &[0xff]).unwrap_err().to_string(),
            "Failed to deserialize avro schema"
        );
    }
}
