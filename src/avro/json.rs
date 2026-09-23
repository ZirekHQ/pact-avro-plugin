use crate::avro::schema::{Kind, SchemaCtx};
use crate::error::PluginError;
use apache_avro::schema::{RecordSchema, Schema};
use apache_avro::types::Value as AvroValue;
use serde_json::Value as Json;

/// JSON numbers can't represent NaN or infinities, so a non-finite float
/// or double is carried as its canonical Rust string form (`"NaN"`,
/// `"inf"`, `"-inf"`) instead — `coerce_scalar` parses these back when
/// targeting a `Float`/`Double` field.
fn finite_or_sentinel(n: f64) -> Json {
    serde_json::Number::from_f64(n)
        .map(Json::Number)
        .unwrap_or_else(|| Json::String(n.to_string()))
}

pub fn to_json(value: &AvroValue) -> Json {
    match value {
        AvroValue::Null => Json::Null,
        AvroValue::Boolean(flag) => Json::Bool(*flag),
        AvroValue::Int(n) => Json::Number((*n).into()),
        AvroValue::Long(n) => Json::Number((*n).into()),
        AvroValue::Float(n) => finite_or_sentinel(*n as f64),
        AvroValue::Double(n) => finite_or_sentinel(*n),
        AvroValue::String(text) => Json::String(text.clone()),
        AvroValue::Enum(_, symbol) => Json::String(symbol.clone()),
        AvroValue::Bytes(bytes) | AvroValue::Fixed(_, bytes) => {
            Json::String(bytes.iter().map(|byte| *byte as char).collect())
        }
        AvroValue::Union(_, inner) => to_json(inner),
        AvroValue::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        AvroValue::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
        AvroValue::Record(fields) => Json::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
        _ => unreachable!(
            "logical-type Avro values (Date/Decimal/Uuid/...) never occur: \
             avro::schema::parse_str/parse_file strip every logicalType before parsing"
        ),
    }
}

fn mismatch(name: &str, schema: &Schema) -> PluginError {
    PluginError::Message(format!(
        "Generated value for '{name}' does not fit schema of type '{}'",
        crate::avro::schema::kind(schema).name()
    ))
}

pub fn coerce(ctx: &SchemaCtx, schema: &Schema, json: &Json) -> Result<AvroValue, PluginError> {
    coerce_named(ctx, "$", schema, json)
}

fn coerce_named(
    ctx: &SchemaCtx,
    name: &str,
    schema: &Schema,
    json: &Json,
) -> Result<AvroValue, PluginError> {
    let resolved = ctx.resolve(schema);
    match resolved {
        Schema::Union(union) => coerce_union(ctx, name, union.variants(), json),
        Schema::Record(record) => coerce_record(ctx, record, json),
        Schema::Array(array) => coerce_array(ctx, name, &array.items, json),
        Schema::Map(map) => coerce_map(ctx, name, &map.types, json),
        _ => coerce_scalar(name, resolved, json),
    }
}

fn coerce_union(
    ctx: &SchemaCtx,
    name: &str,
    variants: &[Schema],
    json: &Json,
) -> Result<AvroValue, PluginError> {
    let wants_null = matches!(json, Json::Null);
    variants
        .iter()
        .position(|variant| (ctx.kind_of(variant) == Kind::Null) == wants_null)
        .ok_or_else(|| {
            PluginError::Message(format!(
                "Union schema for '{name}' has no branch for the generated value"
            ))
        })
        .and_then(|index| {
            coerce_named(ctx, name, &variants[index], json)
                .map(|inner| AvroValue::Union(index as u32, Box::new(inner)))
        })
}

fn coerce_record(
    ctx: &SchemaCtx,
    record: &RecordSchema,
    json: &Json,
) -> Result<AvroValue, PluginError> {
    let Json::Object(entries) = json else {
        return Err(PluginError::Message(format!(
            "Expected an object for record '{}'",
            record.name.name()
        )));
    };
    record
        .fields
        .iter()
        .map(|field| {
            entries
                .get(&field.name)
                .ok_or_else(|| {
                    PluginError::Message(format!(
                        "Generated record is missing field '{}'",
                        field.name
                    ))
                })
                .and_then(|value| coerce_named(ctx, &field.name, &field.schema, value))
                .map(|value| (field.name.clone(), value))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(AvroValue::Record)
}

fn coerce_array(
    ctx: &SchemaCtx,
    name: &str,
    items_schema: &Schema,
    json: &Json,
) -> Result<AvroValue, PluginError> {
    let Json::Array(items) = json else {
        return Err(PluginError::Message(format!(
            "Expected an array for '{name}'"
        )));
    };
    items
        .iter()
        .map(|item| coerce_named(ctx, name, items_schema, item))
        .collect::<Result<Vec<_>, _>>()
        .map(AvroValue::Array)
}

fn coerce_map(
    ctx: &SchemaCtx,
    name: &str,
    values_schema: &Schema,
    json: &Json,
) -> Result<AvroValue, PluginError> {
    let Json::Object(entries) = json else {
        return Err(PluginError::Message(format!(
            "Expected an object for '{name}'"
        )));
    };
    entries
        .iter()
        .map(|(key, value)| {
            coerce_named(ctx, name, values_schema, value).map(|value| (key.clone(), value))
        })
        .collect::<Result<std::collections::HashMap<_, _>, _>>()
        .map(AvroValue::Map)
}

/// Reads a JSON number as `i64`, falling back to a finite, integral `f64`
/// representation (e.g. `999.0`, or an integral value at or above `9.0e15`
/// that `serde_json` stores as a float).
fn integral_i64(n: &serde_json::Number) -> Option<i64> {
    n.as_i64().or_else(|| {
        n.as_f64()
            .filter(|f| f.fract() == 0.0 && *f >= i64::MIN as f64 && *f < i64::MAX as f64)
            .map(|f| f as i64)
    })
}

fn coerce_scalar(name: &str, schema: &Schema, json: &Json) -> Result<AvroValue, PluginError> {
    match (schema, json) {
        (Schema::Null, Json::Null) => Ok(AvroValue::Null),
        (Schema::Boolean, Json::Bool(flag)) => Ok(AvroValue::Boolean(*flag)),
        (Schema::Int, Json::Number(n)) => integral_i64(n)
            .and_then(|n| i32::try_from(n).ok())
            .map(AvroValue::Int)
            .ok_or_else(|| mismatch(name, schema)),
        (Schema::Long, Json::Number(n)) => integral_i64(n)
            .map(AvroValue::Long)
            .ok_or_else(|| mismatch(name, schema)),
        (Schema::Float, Json::Number(n)) => n
            .as_f64()
            .filter(|n| *n >= f32::MIN as f64 && *n <= f32::MAX as f64)
            .map(|n| AvroValue::Float(n as f32))
            .ok_or_else(|| mismatch(name, schema)),
        (Schema::Float, Json::String(text)) => text
            .parse::<f32>()
            .map(AvroValue::Float)
            .map_err(|_| mismatch(name, schema)),
        (Schema::Double, Json::Number(n)) => n
            .as_f64()
            .map(AvroValue::Double)
            .ok_or_else(|| mismatch(name, schema)),
        (Schema::Double, Json::String(text)) => text
            .parse::<f64>()
            .map(AvroValue::Double)
            .map_err(|_| mismatch(name, schema)),
        (Schema::String, Json::String(text)) => Ok(AvroValue::String(text.clone())),
        (Schema::Bytes, Json::String(text)) => crate::avro::record::code_point_bytes(name, text)
            .map(AvroValue::Bytes)
            .map_err(|_| mismatch(name, schema)),
        (Schema::Fixed(fixed), Json::String(text)) => {
            let bytes = crate::avro::record::code_point_bytes(name, text)
                .map_err(|_| mismatch(name, schema))?;
            if bytes.len() == fixed.size {
                Ok(AvroValue::Fixed(bytes.len(), bytes))
            } else {
                Err(mismatch(name, schema))
            }
        }
        (Schema::Enum(symbols), Json::String(symbol)) => symbols
            .symbols
            .iter()
            .position(|candidate| candidate == symbol)
            .map(|index| AvroValue::Enum(index as u32, symbol.clone()))
            .ok_or_else(|| mismatch(name, schema)),
        _ => Err(mismatch(name, schema)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::codec::{decode, encode};
    use crate::avro::schema::parse_str;

    #[test]
    fn integral_floats_at_or_above_i64_float_precision_coerce_to_int_and_long() {
        let big = serde_json::json!(9.0e15);
        assert_eq!(
            coerce_scalar("n", &Schema::Long, &big).unwrap(),
            AvroValue::Long(9_000_000_000_000_000)
        );

        let small = serde_json::json!(999.0);
        assert_eq!(
            coerce_scalar("n", &Schema::Int, &small).unwrap(),
            AvroValue::Int(999)
        );
    }

    #[test]
    fn non_integral_floats_are_rejected_for_int_and_long() {
        assert!(coerce_scalar("n", &Schema::Long, &serde_json::json!(1.5)).is_err());
    }

    #[test]
    fn finite_numbers_outside_the_f32_range_are_rejected() {
        assert!(coerce_scalar("n", &Schema::Float, &serde_json::json!(1e100)).is_err());
    }

    #[test]
    fn bytes_and_fixed_fields_with_non_ascii_bytes_round_trip_exactly() {
        let schema = parse_str(
            r#"{"type":"record","name":"Item","fields":[
                 {"name":"blob","type":"bytes"},
                 {"name":"hash","type":{"type":"fixed","name":"MD5","size":3}}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let value = AvroValue::Record(vec![
            ("blob".into(), AvroValue::Bytes(vec![0xC3, 0xFF, 0x00])),
            ("hash".into(), AvroValue::Fixed(3, vec![0xAB, 0xCD, 0x01])),
        ]);
        let json = to_json(&value);
        let restored = coerce(&ctx, &schema, &json).unwrap();
        assert_eq!(restored, value);
    }

    #[test]
    fn a_scalar_record_round_trips_through_json() {
        let schema = parse_str(
            r#"{"type":"record","name":"Item","fields":[
                 {"name":"id","type":"long"},{"name":"name","type":"string"},
                 {"name":"active","type":"boolean"}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let value = AvroValue::Record(vec![
            ("id".into(), AvroValue::Long(7)),
            ("name".into(), AvroValue::String("hi".into())),
            ("active".into(), AvroValue::Boolean(true)),
        ]);
        let json = to_json(&value);
        let restored = coerce(&ctx, &schema, &json).unwrap();
        assert_eq!(restored, value);
    }

    #[test]
    fn nested_records_arrays_maps_and_unions_round_trip() {
        let schema = parse_str(
            r#"[{"type":"record","name":"Item","namespace":"x","fields":[{"name":"id","type":"long"}]},
                {"type":"record","name":"Complex","namespace":"x","fields":[
                  {"name":"items","type":{"type":"array","items":"x.Item"}},
                  {"name":"zipcode","type":["bytes","null"]},
                  {"name":"color","type":{"type":"enum","name":"Color","symbols":["RED","GREEN"]}},
                  {"name":"md5","type":{"type":"fixed","name":"MD5","size":4}},
                  {"name":"ages","type":{"type":"map","values":"int"}}]}]"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let record = ctx.find_record("Complex").unwrap();
        let value = AvroValue::Record(vec![
            (
                "items".into(),
                AvroValue::Array(vec![AvroValue::Record(vec![(
                    "id".into(),
                    AvroValue::Long(7),
                )])]),
            ),
            (
                "zipcode".into(),
                AvroValue::Union(1, Box::new(AvroValue::Null)),
            ),
            ("color".into(), AvroValue::Enum(1, "GREEN".into())),
            ("md5".into(), AvroValue::Fixed(4, b"abcd".to_vec())),
            (
                "ages".into(),
                AvroValue::Map([("first".to_string(), AvroValue::Int(2))].into()),
            ),
        ]);
        let json = to_json(&value);
        let restored = coerce(&ctx, record, &json).unwrap();
        assert_eq!(restored, value);
    }

    #[test]
    fn nan_and_infinite_floats_and_doubles_round_trip_exactly() {
        let schema = parse_str(
            r#"{"type":"record","name":"Item","fields":[
                 {"name":"f","type":"float"},{"name":"d","type":"double"}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        for value in [
            AvroValue::Record(vec![
                ("f".into(), AvroValue::Float(f32::NAN)),
                ("d".into(), AvroValue::Double(f64::INFINITY)),
            ]),
            AvroValue::Record(vec![
                ("f".into(), AvroValue::Float(f32::NEG_INFINITY)),
                ("d".into(), AvroValue::Double(f64::NAN)),
            ]),
        ] {
            let json = to_json(&value);
            let restored = coerce(&ctx, &schema, &json).unwrap();
            let (AvroValue::Record(expected), AvroValue::Record(actual)) = (&value, &restored)
            else {
                panic!("record expected")
            };
            for ((_, expected), (_, actual)) in expected.iter().zip(actual.iter()) {
                match (expected, actual) {
                    (AvroValue::Float(e), AvroValue::Float(a)) => {
                        assert!(e.to_bits() == a.to_bits() || e == a, "{e} != {a}")
                    }
                    (AvroValue::Double(e), AvroValue::Double(a)) => {
                        assert!(e.to_bits() == a.to_bits() || e == a, "{e} != {a}")
                    }
                    _ => panic!("unexpected variant"),
                }
            }
        }
    }

    #[test]
    fn a_json_value_that_does_not_fit_the_schema_is_a_clear_error() {
        let schema =
            parse_str(r#"{"type":"record","name":"Item","fields":[{"name":"id","type":"long"}]}"#)
                .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let error = coerce(&ctx, &schema, &serde_json::json!({"id": "not a number"})).unwrap_err();
        assert!(error.to_string().contains("id"), "{error}");
    }

    #[test]
    fn round_tripping_via_encode_and_decode_matches_the_original_bytes() {
        let schema = parse_str(
            r#"{"type":"record","name":"Item","fields":[
                 {"name":"id","type":"long"},{"name":"name","type":"string"}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let value = AvroValue::Record(vec![
            ("id".into(), AvroValue::Long(3)),
            ("name".into(), AvroValue::String("hello".into())),
        ]);
        let bytes = encode(&ctx, &schema, value.clone()).unwrap();
        let decoded = decode(&ctx, &schema, &bytes).unwrap();
        let json = to_json(&decoded);
        let re_encoded = encode(&ctx, &schema, coerce(&ctx, &schema, &json).unwrap()).unwrap();
        assert_eq!(re_encoded, bytes);
    }
}
