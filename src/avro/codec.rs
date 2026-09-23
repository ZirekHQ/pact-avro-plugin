use crate::avro::map_order::sort_map_entries;
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
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            sort_map_entries(ctx, record, &bytes)
                .map_err(|error| format!("sorting map entries failed: {error}"))
        })
        .map_err(PluginError::Exception)
}

pub fn decode<'a>(
    ctx: &SchemaCtx<'a>,
    record: &'a Schema,
    bytes: &[u8],
) -> Result<Value, PluginError> {
    let mut input = bytes;
    GenericDatumReader::builder(record)
        .writer_schemata(vec![ctx.root()])
        .and_then(|builder| builder.build())
        .and_then(|reader| reader.read_value(&mut input))
        .map_err(|error| error.to_string())
        .and_then(|value| match input.len() {
            0 => Ok(value),
            extra => Err(format!("{extra} trailing bytes after avro datum")),
        })
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

    const KEYS: [&str; 8] = ["h", "c", "a", "g", "b", "f", "d", "e"];

    fn int_map() -> Value {
        Value::Map(
            KEYS.iter()
                .enumerate()
                .map(|(i, key)| (key.to_string(), Value::Int(i as i32)))
                .collect(),
        )
    }

    fn int_map_bytes() -> Vec<u8> {
        let mut sorted: Vec<(usize, &str)> = KEYS.iter().copied().enumerate().collect();
        sorted.sort_by_key(|(_, key)| *key);
        let mut bytes = vec![(sorted.len() * 2) as u8];
        sorted.iter().for_each(|(i, key)| {
            bytes.extend([0x02, key.as_bytes()[0], (*i * 2) as u8]);
        });
        bytes.push(0);
        bytes
    }

    fn assert_encodes_to(schema_json: &str, build: impl Fn() -> Value, expected: &[u8]) {
        let schema = parse_str(schema_json).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        (0..50).for_each(|_| {
            let value = Value::Record(vec![("m".into(), build())]);
            assert_eq!(encode(&ctx, &schema, value).unwrap(), expected);
        });
    }

    fn record_with(field_type: &str) -> String {
        format!(r#"{{"type":"record","name":"Bag","fields":[{{"name":"m","type":{field_type}}}]}}"#)
    }

    #[test]
    fn map_entries_encode_sorted_by_key() {
        assert_encodes_to(
            &record_with(r#"{"type":"map","values":"int"}"#),
            int_map,
            &int_map_bytes(),
        );
    }

    #[test]
    fn map_keys_sort_by_utf8_bytes_not_utf16_units_or_length() {
        let long_key = "z".repeat(64);
        let unsorted = ["𝄞", "\u{FF5E}", "é", long_key.as_str(), "ab", "a", ""];
        let sorted: [(&[u8], &str, u8); 7] = [
            (&[0x00], "", 0x0c),
            (&[0x02], "a", 0x0a),
            (&[0x04], "ab", 0x08),
            (&[0x80, 0x01], long_key.as_str(), 0x06),
            (&[0x04], "é", 0x04),
            (&[0x06], "\u{FF5E}", 0x02),
            (&[0x08], "𝄞", 0x00),
        ];
        let mut expected = vec![0x0e];
        sorted.iter().for_each(|(length, key, value)| {
            expected.extend(*length);
            expected.extend(key.as_bytes());
            expected.push(*value);
        });
        expected.push(0);
        assert_encodes_to(
            &record_with(r#"{"type":"map","values":"int"}"#),
            || {
                Value::Map(
                    unsorted
                        .iter()
                        .enumerate()
                        .map(|(i, key)| (key.to_string(), Value::Int(i as i32)))
                        .collect(),
                )
            },
            &expected,
        );
    }

    #[test]
    fn maps_nested_in_arrays_encode_sorted_by_key() {
        let mut expected = vec![0x04];
        expected.extend(int_map_bytes());
        expected.extend(int_map_bytes());
        expected.push(0);
        assert_encodes_to(
            &record_with(r#"{"type":"array","items":{"type":"map","values":"int"}}"#),
            || Value::Array(vec![int_map(), int_map()]),
            &expected,
        );
    }

    #[test]
    fn maps_nested_in_map_values_encode_sorted_by_key() {
        let mut expected = vec![0x02, 0x02, b'k'];
        expected.extend(int_map_bytes());
        expected.push(0);
        assert_encodes_to(
            &record_with(r#"{"type":"map","values":{"type":"map","values":"int"}}"#),
            || Value::Map([("k".to_string(), int_map())].into()),
            &expected,
        );
    }

    #[test]
    fn maps_inside_unions_encode_sorted_by_key() {
        let mut expected = vec![0x02];
        expected.extend(int_map_bytes());
        assert_encodes_to(
            &record_with(r#"["null",{"type":"map","values":"int"}]"#),
            || Value::Union(1, Box::new(int_map())),
            &expected,
        );
    }

    #[test]
    fn maps_behind_named_references_encode_sorted_by_key() {
        let schema = parse_str(
            r#"{"type":"record","name":"Bag","fields":[
                 {"name":"first","type":{"type":"record","name":"Inner","fields":[
                   {"name":"m","type":{"type":"map","values":"int"}}]}},
                 {"name":"second","type":"Inner"}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let inner = || Value::Record(vec![("m".into(), int_map())]);
        let expected = [int_map_bytes(), int_map_bytes()].concat();
        (0..50).for_each(|_| {
            let value = Value::Record(vec![("first".into(), inner()), ("second".into(), inner())]);
            assert_eq!(encode(&ctx, &schema, value).unwrap(), expected);
        });
    }

    #[test]
    fn empty_and_single_entry_maps_keep_the_avro_layout() {
        let schema = parse_str(&record_with(r#"{"type":"map","values":"int"}"#)).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let bytes = |entries: Vec<(String, Value)>| {
            encode(
                &ctx,
                &schema,
                Value::Record(vec![(
                    "m".into(),
                    Value::Map(entries.into_iter().collect()),
                )]),
            )
            .unwrap()
        };
        assert_eq!(bytes(vec![]), [0x00]);
        assert_eq!(
            bytes(vec![("a".into(), Value::Int(1))]),
            [0x02, 0x02, b'a', 0x02, 0x00]
        );
    }

    #[test]
    fn decode_reverses_encode() {
        let schema = item_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let bytes = encode(&ctx, &schema, item("hello", 3)).unwrap();
        assert_eq!(decode(&ctx, &schema, &bytes).unwrap(), item("hello", 3));
    }

    #[test]
    fn decode_rejects_bytes_after_the_datum() {
        let schema = item_schema();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let mut bytes = encode(&ctx, &schema, item("hello", 3)).unwrap();
        bytes.push(0x00);
        assert_eq!(
            decode(&ctx, &schema, &bytes).unwrap_err().to_string(),
            "Failed to deserialize avro schema"
        );
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
