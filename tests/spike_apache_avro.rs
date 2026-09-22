use apache_avro::reader::datum::GenericDatumReader;
use apache_avro::schema::{ResolvedSchema, Schema};
use apache_avro::types::Value;
use apache_avro::writer::datum::GenericDatumWriter;

const UNION: &str = r#"[
  {"type":"record","name":"Item","namespace":"x","fields":[{"name":"id","type":"long"}]},
  {"type":"record","name":"Complex","namespace":"x","fields":[
    {"name":"items","type":{"type":"array","items":"x.Item"}},
    {"name":"zipcode","type":["bytes","null"]},
    {"name":"color","type":{"type":"enum","name":"Color","symbols":["RED","GREEN"]}}
  ]}
]"#;

fn complex(schema: &Schema) -> &Schema {
    match schema {
        Schema::Union(union) => &union.variants()[1],
        other => other,
    }
}

#[test]
fn named_references_stay_as_ref_and_resolve_through_resolved_schema() {
    let schema = Schema::parse_str(UNION).unwrap();
    let Schema::Record(record) = complex(&schema) else {
        panic!("expected record")
    };
    let Schema::Array(array) = &record.fields[0].schema else {
        panic!("expected array")
    };
    let Schema::Ref { name } = &*array.items else {
        panic!("expected ref, got {:?}", array.items)
    };
    let resolved = ResolvedSchema::try_from(&schema).unwrap();
    assert!(matches!(
        resolved.get_names().get(name),
        Some(Schema::Record(_))
    ));
}

#[test]
fn record_from_union_roundtrips_with_root_supplied_for_ref_resolution() {
    let schema = Schema::parse_str(UNION).unwrap();
    let record = complex(&schema);
    let value = Value::Record(vec![
        (
            "items".into(),
            Value::Array(vec![Value::Record(vec![("id".into(), Value::Long(7))])]),
        ),
        ("zipcode".into(), Value::Union(1, Box::new(Value::Null))),
        ("color".into(), Value::Enum(1, "GREEN".into())),
    ]);
    let bytes = GenericDatumWriter::builder(record)
        .schemata(vec![&schema])
        .and_then(|builder| builder.build())
        .and_then(|writer| writer.write_value_to_vec(value.clone()))
        .unwrap();
    let decoded = GenericDatumReader::builder(record)
        .writer_schemata(vec![&schema])
        .and_then(|builder| builder.build())
        .and_then(|reader| reader.read_value(&mut &bytes[..]))
        .unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn logical_types_parse_as_dedicated_variants() {
    let schema = Schema::parse_str(r#"{"type":"int","logicalType":"date"}"#).unwrap();
    assert!(matches!(schema, Schema::Date));
}
