use crate::error::PluginError;
use apache_avro::schema::{ResolvedSchema, Schema};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Null,
    Boolean,
    Int,
    Long,
    Float,
    Double,
    Bytes,
    String,
    Enum,
    Fixed,
    Array,
    Map,
    Record,
    Union,
    Unsupported,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Null => "NULL",
            Kind::Boolean => "BOOLEAN",
            Kind::Int => "INT",
            Kind::Long => "LONG",
            Kind::Float => "FLOAT",
            Kind::Double => "DOUBLE",
            Kind::Bytes => "BYTES",
            Kind::String => "STRING",
            Kind::Enum => "ENUM",
            Kind::Fixed => "FIXED",
            Kind::Array => "ARRAY",
            Kind::Map => "MAP",
            Kind::Record => "RECORD",
            Kind::Union => "UNION",
            Kind::Unsupported => "UNSUPPORTED",
        }
    }
}

pub fn kind(schema: &Schema) -> Kind {
    match schema {
        Schema::Null => Kind::Null,
        Schema::Boolean => Kind::Boolean,
        Schema::Int => Kind::Int,
        Schema::Long => Kind::Long,
        Schema::Float => Kind::Float,
        Schema::Double => Kind::Double,
        Schema::Bytes => Kind::Bytes,
        Schema::String => Kind::String,
        Schema::Enum(_) => Kind::Enum,
        Schema::Fixed(_) => Kind::Fixed,
        Schema::Array(_) => Kind::Array,
        Schema::Map(_) => Kind::Map,
        Schema::Record(_) => Kind::Record,
        Schema::Union(_) => Kind::Union,
        _ => Kind::Unsupported,
    }
}

fn message(text: String) -> PluginError {
    PluginError::Message(text)
}

fn absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn strip_logical_types(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| key != "logicalType")
                .map(|(key, inner)| (key, strip_logical_types(inner)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(strip_logical_types).collect()),
        other => other,
    }
}

fn parse_base_types(text: &str) -> Result<Schema, String> {
    serde_json::from_str::<Value>(text)
        .map_err(|error| error.to_string())
        .and_then(|json| {
            Schema::parse_str(&strip_logical_types(json).to_string())
                .map_err(|error| error.to_string())
        })
}

fn read_and_parse(path: &Path) -> Result<Schema, String> {
    std::fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| parse_base_types(&text))
}

pub fn parse_file(path: &Path) -> Result<Schema, PluginError> {
    read_and_parse(path).map_err(|error| {
        tracing::error!(
            "Failed to read or parse avro schema file {}: {error}",
            absolute(path)
        );
        message(format!(
            "Failed to parse avro schema from file: {}",
            absolute(path)
        ))
    })
}

pub fn parse_str(text: &str) -> Result<Schema, PluginError> {
    parse_base_types(text).map_err(|error| {
        tracing::error!("Failed to parse avro schema from string: {error}");
        message("Failed to parse avro schema from string".to_string())
    })
}

fn record_named(schema: &Schema, name: &str) -> bool {
    matches!(schema, Schema::Record(record) if record.name.name() == name)
}

pub struct SchemaCtx<'a> {
    root: &'a Schema,
    resolved: ResolvedSchema<'a>,
}

impl<'a> SchemaCtx<'a> {
    pub fn new(root: &'a Schema) -> Result<Self, PluginError> {
        ResolvedSchema::try_from(root)
            .map(|resolved| Self { root, resolved })
            .map_err(|error| PluginError::Exception(error.to_string()))
    }

    pub fn root(&self) -> &'a Schema {
        self.root
    }

    pub fn resolve(&self, schema: &'a Schema) -> &'a Schema {
        match schema {
            Schema::Ref { name } => self
                .resolved
                .get_names()
                .get(name)
                .copied()
                .unwrap_or(schema),
            other => other,
        }
    }

    pub fn kind_of(&self, schema: &'a Schema) -> Kind {
        kind(self.resolve(schema))
    }

    pub fn find_record(&self, record_name: &str) -> Result<&'a Schema, PluginError> {
        match self.root {
            Schema::Record(_) if record_named(self.root, record_name) => Ok(self.root),
            Schema::Record(_) => Err(message(format!(
                "Record '{record_name}' was not found in avro Schema provided"
            ))),
            Schema::Union(union) => union
                .variants()
                .iter()
                .map(|variant| self.resolve(variant))
                .find(|variant| record_named(variant, record_name))
                .ok_or_else(|| {
                    message(format!(
                        "Avro union schema didn't contain record: '{record_name}'"
                    ))
                }),
            other => Err(message(format!(
                "Schema provided is of type: '{}', but expected to be union/record",
                kind(other).name()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str =
        r#"{"type":"record","name":"Item","namespace":"x","fields":[{"name":"id","type":"long"}]}"#;
    const UNION: &str = r#"[
      {"type":"record","name":"Item","namespace":"x","fields":[{"name":"id","type":"long"}]},
      {"type":"record","name":"Other","namespace":"x","fields":[{"name":"item","type":"x.Item"}]}
    ]"#;

    fn parsed(text: &str) -> Schema {
        parse_str(text).unwrap()
    }

    #[test]
    fn finds_a_top_level_record_by_name() {
        let schema = parsed(ITEM);
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert!(matches!(ctx.find_record("Item"), Ok(Schema::Record(_))));
    }

    #[test]
    fn rejects_a_top_level_record_with_another_name() {
        let schema = parsed(ITEM);
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert_eq!(
            ctx.find_record("Nope").unwrap_err().to_string(),
            "Record 'Nope' was not found in avro Schema provided"
        );
    }

    #[test]
    fn finds_a_record_inside_a_union() {
        let schema = parsed(UNION);
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert!(ctx.find_record("Other").is_ok());
    }

    #[test]
    fn reports_a_record_missing_from_a_union() {
        let schema = parsed(UNION);
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert_eq!(
            ctx.find_record("Nope").unwrap_err().to_string(),
            "Avro union schema didn't contain record: 'Nope'"
        );
    }

    #[test]
    fn rejects_schemas_that_are_neither_record_nor_union() {
        let schema = parsed(r#""string""#);
        let ctx = SchemaCtx::new(&schema).unwrap();
        assert_eq!(
            ctx.find_record("Item").unwrap_err().to_string(),
            "Schema provided is of type: 'STRING', but expected to be union/record"
        );
    }

    #[test]
    fn resolves_named_references_to_their_record() {
        let schema = parsed(UNION);
        let ctx = SchemaCtx::new(&schema).unwrap();
        let Schema::Record(other) = ctx.find_record("Other").unwrap() else {
            panic!("record expected")
        };
        assert_eq!(ctx.kind_of(&other.fields[0].schema), Kind::Record);
    }

    #[test]
    fn invalid_schema_text_reports_a_fixed_message() {
        assert_eq!(
            parse_str("not json").unwrap_err().to_string(),
            "Failed to parse avro schema from string"
        );
    }

    #[test]
    fn missing_schema_file_reports_its_absolute_path() {
        let message = parse_file(Path::new("non-existing.avsc"))
            .unwrap_err()
            .to_string();
        assert!(message.starts_with("Failed to parse avro schema from file:"));
        assert!(message.ends_with("non-existing.avsc"));
    }

    #[test]
    fn date_over_int_is_an_int() {
        let schema = parsed(r#"{"type":"int","logicalType":"date"}"#);
        assert_eq!(kind(&schema), Kind::Int);
    }

    #[test]
    fn uuid_logical_type_over_string_is_a_string() {
        let schema = parsed(r#"{"type":"string","logicalType":"uuid"}"#);
        assert_eq!(kind(&schema), Kind::String);
    }

    #[test]
    fn timestamp_micros_over_long_is_a_long() {
        let schema = parsed(r#"{"type":"long","logicalType":"timestamp-micros"}"#);
        assert_eq!(kind(&schema), Kind::Long);
    }

    #[test]
    fn decimal_over_bytes_is_bytes() {
        let schema = parsed(r#"{"type":"bytes","logicalType":"decimal","precision":4,"scale":2}"#);
        assert_eq!(kind(&schema), Kind::Bytes);
    }

    #[test]
    fn invalid_json_with_a_logical_type_still_reports_the_fixed_message() {
        assert_eq!(
            parse_str(r#"{"type":"string","logicalType":"uuid""#)
                .unwrap_err()
                .to_string(),
            "Failed to parse avro schema from string"
        );
    }
}
