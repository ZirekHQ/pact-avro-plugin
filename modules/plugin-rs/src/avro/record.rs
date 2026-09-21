use crate::avro::node::{Node, Scalar};
use crate::avro::path::PactFieldPath;
use crate::avro::rules::{parse_rules, FieldRule};
use crate::avro::schema::{Kind, SchemaCtx};
use crate::error::PluginError;
use apache_avro::schema::{RecordField, RecordSchema, Schema};
use prost_types::value::Kind as ProtoKind;
use prost_types::Value;
use serde_json::{Map as JsonMap, Number, Value as Json};
use std::collections::BTreeMap;
use std::str::FromStr;

type Errors = Vec<PluginError>;
type Built = Result<Node, Errors>;

fn collect_all<T>(items: impl Iterator<Item = Result<T, Errors>>) -> Result<Vec<T>, Errors> {
    let (oks, errs): (Vec<_>, Vec<_>) = items.partition(Result::is_ok);
    if errs.is_empty() {
        Ok(oks.into_iter().flatten().collect())
    } else {
        Err(errs.into_iter().filter_map(Result::err).flatten().collect())
    }
}

fn kind_label(kind: &ProtoKind) -> &'static str {
    match kind {
        ProtoKind::NullValue(_) => "NullValue",
        ProtoKind::NumberValue(_) => "NumberValue",
        ProtoKind::StringValue(_) => "StringValue",
        ProtoKind::BoolValue(_) => "BoolValue",
        ProtoKind::StructValue(_) => "StructValue",
        ProtoKind::ListValue(_) => "ListValue",
    }
}

fn one(error: PluginError) -> Errors {
    vec![error]
}

fn parse_number<T: FromStr>(text: &str) -> Result<T, PluginError> {
    text.parse()
        .map_err(|_| PluginError::Exception(format!("For input string: \"{text}\"")))
}

fn parse_bool(name: &str, text: &str) -> Result<bool, PluginError> {
    match text.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(PluginError::Exception(format!(
            "Invalid boolean value '{text}' for field '{name}'"
        ))),
    }
}

fn code_point_bytes(name: &str, text: &str) -> Result<Vec<u8>, PluginError> {
    text.chars()
        .map(|ch| {
            u8::try_from(u32::from(ch)).map_err(|_| {
                PluginError::Exception(format!(
                    "Invalid bytes default for field '{name}': U+{:04X} is above U+00FF",
                    u32::from(ch)
                ))
            })
        })
        .collect()
}

fn null_not_allowed(name: &str) -> PluginError {
    PluginError::Message(format!(
        "Null value is not allowed for non-nullable field '{name}'"
    ))
}

fn is_null(value: &Value) -> bool {
    matches!(value.kind, None | Some(ProtoKind::NullValue(_)))
}

fn scalar_from_text(kind: Kind, name: &str, text: &str) -> Result<Scalar, PluginError> {
    match kind {
        Kind::Boolean => parse_bool(name, text).map(Scalar::Boolean),
        Kind::Bytes | Kind::Fixed => Ok(Scalar::Bytes(text.as_bytes().to_vec())),
        Kind::Double => parse_number(text).map(Scalar::Double),
        Kind::Float => parse_number(text).map(Scalar::Float),
        Kind::Int => parse_number(text).map(Scalar::Int),
        Kind::Long => parse_number(text).map(Scalar::Long),
        Kind::Enum | Kind::String => Ok(Scalar::Text(text.to_string())),
        Kind::Null => Ok(Scalar::Null),
        other => Err(PluginError::field_unsupported_type(
            other.name(),
            name,
            text,
        )),
    }
}

fn numeric_default(kind: Kind, number: &Number) -> Option<Scalar> {
    match kind {
        Kind::Int => number
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .map(Scalar::Int),
        Kind::Long => number.as_i64().map(Scalar::Long),
        Kind::Float => number.as_f64().map(|n| Scalar::Float(n as f32)),
        Kind::Double => number.as_f64().map(Scalar::Double),
        _ => None,
    }
}

fn struct_fields(value: &Value) -> BTreeMap<String, Value> {
    match &value.kind {
        Some(ProtoKind::StructValue(inner)) => inner.fields.clone(),
        _ => BTreeMap::new(),
    }
}

pub struct RecordBuilder<'c, 'a> {
    ctx: &'c SchemaCtx<'a>,
}

impl<'c, 'a> RecordBuilder<'c, 'a> {
    pub fn new(ctx: &'c SchemaCtx<'a>) -> Self {
        Self { ctx }
    }

    pub fn build(&self, record: &'a Schema, config: &BTreeMap<String, Value>) -> Built {
        self.record(&PactFieldPath::root(), record, config)
    }

    fn record(
        &self,
        path: &PactFieldPath,
        record: &'a Schema,
        config: &BTreeMap<String, Value>,
    ) -> Built {
        let Schema::Record(schema) = record else {
            return Err(one(PluginError::Message(
                "Record schema expected".to_string(),
            )));
        };
        let fields = schema.fields.iter().map(|field| {
            self.field(path, field, config.get(&field.name))
                .map(|node| (field.name.clone(), node))
        });
        collect_all(fields).map(|fields| Node::Record {
            path: path.clone(),
            fields: fields.into_iter().collect(),
        })
    }

    fn field(&self, path: &PactFieldPath, field: &'a RecordField, config: Option<&Value>) -> Built {
        match self.ctx.kind_of(&field.schema) {
            Kind::Union => self.union_field(path, field, config),
            _ => self.plain_field(path, field, config),
        }
    }

    fn nullable_branch(&self, schema: &'a Schema) -> Option<&'a Schema> {
        let Schema::Union(union) = self.ctx.resolve(schema) else {
            return None;
        };
        let variants = union.variants();
        let has_null = variants.iter().any(|v| self.ctx.kind_of(v) == Kind::Null);
        let non_null = variants.iter().find(|v| self.ctx.kind_of(v) != Kind::Null);
        non_null.filter(|_| variants.len() == 2 && has_null)
    }

    fn union_field(
        &self,
        path: &PactFieldPath,
        field: &'a RecordField,
        config: Option<&Value>,
    ) -> Built {
        match (self.nullable_branch(&field.schema), config) {
            (Some(_), Some(value)) if is_null(value) => {
                Ok(Node::null_leaf(path.field(&field.name)))
            }
            (Some(branch), Some(value)) => self.configured(path, &field.name, branch, value),
            (Some(branch), None) => self.default_value(path, field, branch),
            (None, value) => Err(one(PluginError::field_not_nullable(
                &field.name,
                &format!("{value:?}"),
            ))),
        }
    }

    fn plain_field(
        &self,
        path: &PactFieldPath,
        field: &'a RecordField,
        config: Option<&Value>,
    ) -> Built {
        match config {
            Some(value) => self.configured(path, &field.name, &field.schema, value),
            None if field.default.is_some() => self.default_value(path, field, &field.schema),
            None => Err(one(PluginError::Exception(format!(
                "Couldn't find configuration for field: {}",
                field.name
            )))),
        }
    }

    fn configured(
        &self,
        path: &PactFieldPath,
        name: &str,
        schema: &'a Schema,
        config: &Value,
    ) -> Built {
        match self.ctx.kind_of(schema) {
            Kind::String
            | Kind::Int
            | Kind::Long
            | Kind::Float
            | Kind::Double
            | Kind::Boolean
            | Kind::Enum
            | Kind::Fixed
            | Kind::Bytes
            | Kind::Null => self.value(path, name, schema, config, true),
            Kind::Record => self.record(
                &path.field(name),
                self.ctx.resolve(schema),
                &struct_fields(config),
            ),
            Kind::Array => self.array(path, name, self.ctx.resolve(schema), config),
            Kind::Map => self.map(path, name, self.ctx.resolve(schema), config),
            other => Err(one(PluginError::field_unsupported_type(
                other.name(),
                name,
                &format!("{config:?}"),
            ))),
        }
    }

    fn value_schema(&self, schema: &'a Schema) -> &'a Schema {
        match self.ctx.resolve(schema) {
            Schema::Array(array) => self.ctx.resolve(&array.items),
            Schema::Map(map) => self.ctx.resolve(&map.types),
            other => other,
        }
    }

    fn value(
        &self,
        path: &PactFieldPath,
        name: &str,
        schema: &'a Schema,
        config: &Value,
        append: bool,
    ) -> Built {
        let leaf_path = if append {
            path.field(name)
        } else {
            path.clone()
        };
        let value_schema = self.value_schema(schema);
        match &config.kind {
            None | Some(ProtoKind::NullValue(_)) => Ok(Node::null_leaf(leaf_path)),
            Some(ProtoKind::StringValue(text)) => self
                .text_leaf(leaf_path, name, value_schema, text)
                .map_err(one),
            Some(ProtoKind::StructValue(inner))
                if self.ctx.kind_of(value_schema) == Kind::Record =>
            {
                self.record(&leaf_path, value_schema, &inner.fields)
            }
            Some(other) => Err(one(PluginError::Message(format!(
                "{} kind value for field is not supported",
                kind_label(other)
            )))),
        }
    }

    fn text_leaf(
        &self,
        path: PactFieldPath,
        name: &str,
        schema: &'a Schema,
        text: &str,
    ) -> Result<Node, PluginError> {
        let FieldRule { value, rules } = parse_rules(text)?;
        let scalar = scalar_from_text(self.ctx.kind_of(schema), name, &value)?;
        let rules = if scalar == Scalar::Null {
            vec![]
        } else {
            rules
        };
        Ok(Node::Leaf {
            path,
            value: scalar,
            rules,
        })
    }

    fn element_kind(&self, schema: &'a Schema) -> Kind {
        match self.ctx.resolve(schema) {
            Schema::Array(array) => self.ctx.kind_of(&array.items),
            _ => Kind::Unsupported,
        }
    }

    fn array(&self, path: &PactFieldPath, name: &str, schema: &'a Schema, config: &Value) -> Built {
        let base = path.field(name);
        match &config.kind {
            None | Some(ProtoKind::NullValue(_)) => Err(one(null_not_allowed(name))),
            Some(ProtoKind::ListValue(list)) => {
                let items =
                    list.values.iter().enumerate().map(|(index, item)| {
                        self.array_item(path, &base, name, schema, index, item)
                    });
                collect_all(items).map(|items| Node::Array {
                    path: base.clone(),
                    items,
                })
            }
            Some(other) => Err(one(PluginError::Message(format!(
                "Expected list value for field '{name}' but got '{}'",
                kind_label(other)
            )))),
        }
    }

    fn array_item(
        &self,
        root: &PactFieldPath,
        base: &PactFieldPath,
        name: &str,
        schema: &'a Schema,
        index: usize,
        item: &Value,
    ) -> Built {
        if self.element_kind(schema) == Kind::Record {
            self.value(&base.index(index), name, schema, item, false)
        } else {
            self.value(root, name, schema, item, true)
                .map(|node| node.with_path(base.index(index)))
        }
    }

    fn map(&self, path: &PactFieldPath, name: &str, schema: &'a Schema, config: &Value) -> Built {
        let base = path.field(name);
        match &config.kind {
            None | Some(ProtoKind::NullValue(_)) => Err(one(null_not_allowed(name))),
            Some(ProtoKind::StructValue(inner)) => {
                let entries = inner.fields.iter().map(|(key, item)| {
                    self.value(&base, key, schema, item, true)
                        .map(|node| (key.clone(), node))
                });
                collect_all(entries).map(|entries| Node::Map {
                    path: base.clone(),
                    entries: entries.into_iter().collect(),
                })
            }
            Some(other) => Err(one(PluginError::Message(format!(
                "Expected map value for field '{name}' but got '{}'",
                kind_label(other)
            )))),
        }
    }

    fn default_value(
        &self,
        path: &PactFieldPath,
        field: &'a RecordField,
        schema: &'a Schema,
    ) -> Built {
        match &field.default {
            None => Ok(Node::null_leaf(path.field(&field.name))),
            Some(default) => {
                self.default_node(path.field(&field.name), &field.name, schema, default)
            }
        }
    }

    fn default_node(
        &self,
        path: PactFieldPath,
        name: &str,
        schema: &'a Schema,
        default: &Json,
    ) -> Built {
        let schema = self.ctx.resolve(schema);
        match (schema, default) {
            (_, Json::Null) => Ok(Node::null_leaf(path)),
            (Schema::Union(_), _) => self
                .nullable_branch(schema)
                .ok_or_else(|| one(unsupported_default(Kind::Union, name, default)))
                .and_then(|branch| self.default_node(path, name, branch, default)),
            (Schema::Record(record), Json::Object(entries)) => {
                self.record_default(path, record, entries)
            }
            (Schema::Array(array), Json::Array(items)) => {
                self.array_default(path, name, &array.items, items)
            }
            (Schema::Map(map), Json::Object(entries)) => {
                self.map_default(path, &map.types, entries)
            }
            _ => self.default_leaf(path, name, schema, default).map_err(one),
        }
    }

    fn record_default(
        &self,
        path: PactFieldPath,
        record: &'a RecordSchema,
        entries: &JsonMap<String, Json>,
    ) -> Built {
        let fields = record.fields.iter().map(|field| {
            entries
                .get(&field.name)
                .or(field.default.as_ref())
                .ok_or_else(|| missing_default(&field.name))
                .and_then(|default| {
                    let field_path = path.field(&field.name);
                    self.default_node(field_path, &field.name, &field.schema, default)
                })
                .map(|node| (field.name.clone(), node))
        });
        collect_all(fields).map(|fields| Node::Record {
            path: path.clone(),
            fields: fields.into_iter().collect(),
        })
    }

    fn array_default(
        &self,
        path: PactFieldPath,
        name: &str,
        items_schema: &'a Schema,
        items: &[Json],
    ) -> Built {
        let nodes = items
            .iter()
            .enumerate()
            .map(|(index, item)| self.default_node(path.index(index), name, items_schema, item));
        collect_all(nodes).map(|items| Node::Array {
            path: path.clone(),
            items,
        })
    }

    fn map_default(
        &self,
        path: PactFieldPath,
        values_schema: &'a Schema,
        entries: &JsonMap<String, Json>,
    ) -> Built {
        let nodes = entries.iter().map(|(key, item)| {
            self.default_node(path.field(key), key, values_schema, item)
                .map(|node| (key.clone(), node))
        });
        collect_all(nodes).map(|entries| Node::Map {
            path: path.clone(),
            entries: entries.into_iter().collect(),
        })
    }

    fn default_leaf(
        &self,
        path: PactFieldPath,
        name: &str,
        schema: &'a Schema,
        default: &Json,
    ) -> Result<Node, PluginError> {
        let kind = self.ctx.kind_of(schema);
        let scalar = match (kind, default) {
            (Kind::Bytes | Kind::Fixed, Json::String(text)) => {
                code_point_bytes(name, text).map(Scalar::Bytes)
            }
            (_, Json::String(text)) => scalar_from_text(kind, name, text),
            (Kind::Boolean, Json::Bool(flag)) => Ok(Scalar::Boolean(*flag)),
            (_, Json::Number(number)) => numeric_default(kind, number)
                .ok_or_else(|| unsupported_default(kind, name, default)),
            (other, value) => Err(unsupported_default(other, name, value)),
        }?;
        Ok(Node::Leaf {
            path,
            value: scalar,
            rules: vec![],
        })
    }
}

fn unsupported_default(kind: Kind, name: &str, default: &Json) -> PluginError {
    PluginError::field_unsupported_type(kind.name(), name, &default.to_string())
}

fn missing_default(name: &str) -> Errors {
    one(PluginError::Exception(format!(
        "Couldn't find default for field: {name}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_str;
    use crate::proto_json::json_to_value;
    use pact_models::matchingrules::MatchingRule;
    use serde_json::json;

    fn schema_with_field(field: &str) -> Schema {
        parse_str(&format!(
            r#"{{"namespace":"com.example","type":"record","name":"Parent","fields":[{field}]}}"#
        ))
        .unwrap()
    }

    fn build(schema: &Schema, config: serde_json::Value) -> Built {
        let ctx = SchemaCtx::new(schema).unwrap();
        let record = ctx.find_record("Parent").unwrap();
        let fields = config
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), json_to_value(v)))
            .collect();
        RecordBuilder::new(&ctx).build(record, &fields)
    }

    fn only_leaf(node: Node, name: &str) -> (Scalar, Vec<MatchingRule>) {
        let Node::Record { fields, .. } = node else {
            panic!("record expected")
        };
        match fields.get(name) {
            Some(Node::Leaf { value, rules, .. }) => (value.clone(), rules.clone()),
            other => panic!("leaf expected, got {other:?}"),
        }
    }

    fn scalar_case(field: &str, expression: &str) -> (Scalar, Vec<MatchingRule>) {
        let schema = schema_with_field(field);
        let node = build(&schema, json!({"f": expression})).unwrap();
        only_leaf(node, "f")
    }

    #[test]
    fn primitive_fields_parse_value_and_rule() {
        let cases = [
            (
                r#"{"name":"f","type":"string"}"#,
                "matching(equalTo, 'hello')",
                Scalar::Text("hello".into()),
                MatchingRule::Equality,
            ),
            (
                r#"{"name":"f","type":"int"}"#,
                "matching(integer, 121)",
                Scalar::Int(121),
                MatchingRule::Integer,
            ),
            (
                r#"{"name":"f","type":"long"}"#,
                "notEmpty('100')",
                Scalar::Long(100),
                MatchingRule::NotEmpty,
            ),
            (
                r#"{"name":"f","type":"float"}"#,
                "matching(decimal, 15.8)",
                Scalar::Float(15.8),
                MatchingRule::Decimal,
            ),
            (
                r#"{"name":"f","type":"double"}"#,
                "matching(decimal, 1.8)",
                Scalar::Double(1.8),
                MatchingRule::Decimal,
            ),
            (
                r#"{"name":"f","type":"boolean"}"#,
                "matching(boolean, true)",
                Scalar::Boolean(true),
                MatchingRule::Boolean,
            ),
            (
                r#"{"name":"f","type":{"type":"enum","name":"C","symbols":["A","GREEN"]}}"#,
                "matching(equalTo, 'GREEN')",
                Scalar::Text("GREEN".into()),
                MatchingRule::Equality,
            ),
            (
                r#"{"name":"f","type":"bytes"}"#,
                "matching(equalTo, 'abc')",
                Scalar::Bytes(b"abc".to_vec()),
                MatchingRule::Equality,
            ),
            (
                r#"{"name":"f","type":{"type":"fixed","name":"F","size":4}}"#,
                "matching(equalTo, 'abcd')",
                Scalar::Bytes(b"abcd".to_vec()),
                MatchingRule::Equality,
            ),
        ];
        for (field, expression, scalar, rule) in cases {
            assert_eq!(
                scalar_case(field, expression),
                (scalar, vec![rule]),
                "{field}"
            );
        }
    }

    #[test]
    fn invalid_number_text_reports_the_java_style_message() {
        let schema = schema_with_field(r#"{"name":"f","type":"int"}"#);
        let errors = build(&schema, json!({"f": "matching(type, 'abc')"})).unwrap_err();
        assert_eq!(errors[0].to_string(), "For input string: \"abc\"");
    }

    #[test]
    fn plain_text_is_rejected_as_a_matching_rule_definition() {
        let schema = schema_with_field(r#"{"name":"f","type":"string"}"#);
        let errors = build(&schema, json!({"f": "hello"})).unwrap_err();
        assert!(errors[0]
            .to_string()
            .starts_with("'hello' is not a valid matching rule definition"));
    }

    #[test]
    fn missing_configuration_without_a_default_is_an_error() {
        let schema = schema_with_field(r#"{"name":"f","type":"string"}"#);
        let errors = build(&schema, json!({})).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "Couldn't find configuration for field: f"
        );
    }

    #[test]
    fn string_default_is_used_without_rules() {
        let schema = schema_with_field(r#"{"name":"f","type":"string","default":"NONE"}"#);
        let leaf = only_leaf(build(&schema, json!({})).unwrap(), "f");
        assert_eq!(leaf, (Scalar::Text("NONE".into()), vec![]));
    }

    #[test]
    fn nullable_union_defaults_to_null_and_accepts_configuration() {
        let schema = schema_with_field(r#"{"name":"f","type":["null","int"],"default":null}"#);
        assert_eq!(
            only_leaf(build(&schema, json!({})).unwrap(), "f"),
            (Scalar::Null, vec![])
        );
        let configured = only_leaf(
            build(&schema, json!({"f": "matching(integer, 5)"})).unwrap(),
            "f",
        );
        assert_eq!(configured, (Scalar::Int(5), vec![MatchingRule::Integer]));
    }

    #[test]
    fn unions_that_are_not_nullable_shortcuts_are_rejected() {
        let schema = schema_with_field(r#"{"name":"f","type":["string","int"]}"#);
        let errors = build(&schema, json!({"f": "matching(type, 'x')"})).unwrap_err();
        assert!(errors[0]
            .to_string()
            .starts_with("'UNION' type is only supported to make field nullable, field: 'f'"));
    }

    #[test]
    fn array_leaves_get_indexed_rule_paths() {
        let schema =
            schema_with_field(r#"{"name":"names","type":{"type":"array","items":"string"}}"#);
        let node = build(
            &schema,
            json!({"names": ["notEmpty('a')", "notEmpty('b')"]}),
        )
        .unwrap();
        let paths: Vec<String> = node.rules_by_path().keys().cloned().collect();
        assert_eq!(paths, ["$.names.0", "$.names.1"]);
    }

    #[test]
    fn array_of_records_nests_the_index_before_the_field() {
        let schema = schema_with_field(
            r#"{"name":"addresses","type":{"type":"array","items":{"type":"record","name":"A","fields":[{"name":"street","type":"string"}]}}}"#,
        );
        let node = build(
            &schema,
            json!({"addresses": [{"street": "matching(equalTo, 'x')"}]}),
        )
        .unwrap();
        assert_eq!(
            node.rules_by_path().keys().collect::<Vec<_>>(),
            ["$.addresses.0.street"]
        );
    }

    #[test]
    fn map_entries_use_their_key_as_the_last_path_segment() {
        let schema = schema_with_field(r#"{"name":"ages","type":{"type":"map","values":"int"}}"#);
        let node = build(
            &schema,
            json!({"ages": {"first": "matching(integer, 2)", "second": "matching(integer, 3)"}}),
        )
        .unwrap();
        assert_eq!(
            node.rules_by_path().keys().collect::<Vec<_>>(),
            ["$.ages.first", "$.ages.second"]
        );
    }

    #[test]
    fn nested_records_extend_the_path() {
        let schema = schema_with_field(
            r#"{"name":"address","type":{"type":"record","name":"M","fields":[{"name":"street","type":"string"}]}}"#,
        );
        let node = build(&schema, json!({"address": {"street": "notEmpty('s')"}})).unwrap();
        assert_eq!(
            node.rules_by_path().keys().collect::<Vec<_>>(),
            ["$.address.street"]
        );
    }

    #[test]
    fn errors_from_every_field_are_collected() {
        let schema =
            schema_with_field(r#"{"name":"a","type":"string"},{"name":"b","type":"string"}"#);
        let errors = build(&schema, json!({})).unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn numbers_are_not_valid_config_values() {
        let schema = schema_with_field(r#"{"name":"f","type":"int"}"#);
        let errors = build(&schema, json!({"f": 5})).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "NumberValue kind value for field is not supported"
        );
    }
    fn field_node(node: Node, name: &str) -> Node {
        let Node::Record { mut fields, .. } = node else {
            panic!("record expected")
        };
        fields.remove(name).expect("field present")
    }

    fn leaf_scalar(node: &Node) -> Scalar {
        match node {
            Node::Leaf { value, .. } => value.clone(),
            other => panic!("leaf expected, got {other:?}"),
        }
    }

    #[test]
    fn boolean_accepts_true_and_false_in_any_case() {
        for (text, expected) in [("TRUE", true), ("false", false), ("True", true)] {
            let expression = format!("notEmpty('{text}')");
            let (scalar, _) = scalar_case(r#"{"name":"f","type":"boolean"}"#, &expression);
            assert_eq!(scalar, Scalar::Boolean(expected), "{text}");
        }
    }

    #[test]
    fn boolean_rejects_other_text() {
        let schema = schema_with_field(r#"{"name":"f","type":"boolean"}"#);
        let errors = build(&schema, json!({"f": "matching(type, 'abc')"})).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "Invalid boolean value 'abc' for field 'f'"
        );
    }

    #[test]
    fn explicit_null_for_a_nullable_collection_stays_null() {
        for field in [
            r#"{"name":"f","type":["null",{"type":"array","items":"string"}],"default":null}"#,
            r#"{"name":"f","type":["null",{"type":"map","values":"string"}],"default":null}"#,
        ] {
            let schema = schema_with_field(field);
            let node = field_node(build(&schema, json!({"f": null})).unwrap(), "f");
            assert_eq!(leaf_scalar(&node), Scalar::Null, "{field}");
        }
    }

    #[test]
    fn explicit_null_for_a_non_nullable_collection_is_rejected() {
        for field in [
            r#"{"name":"f","type":{"type":"array","items":"string"}}"#,
            r#"{"name":"f","type":{"type":"map","values":"string"}}"#,
        ] {
            let schema = schema_with_field(field);
            let errors = build(&schema, json!({"f": null})).unwrap_err();
            assert_eq!(
                errors[0].to_string(),
                "Null value is not allowed for non-nullable field 'f'",
                "{field}"
            );
        }
    }

    #[test]
    fn map_keys_with_dots_are_bracket_quoted_in_rule_paths() {
        let schema = schema_with_field(r#"{"name":"ages","type":{"type":"map","values":"int"}}"#);
        let node = build(&schema, json!({"ages": {"a.b": "matching(integer, 2)"}})).unwrap();
        assert_eq!(
            node.rules_by_path().keys().collect::<Vec<_>>(),
            ["$.ages['a.b']"]
        );
    }

    #[test]
    fn array_default_becomes_indexed_leaves() {
        let schema = schema_with_field(
            r#"{"name":"f","type":{"type":"array","items":"int"},"default":[1,2]}"#,
        );
        let Node::Array { items, .. } = field_node(build(&schema, json!({})).unwrap(), "f") else {
            panic!("array expected")
        };
        let values: Vec<Scalar> = items.iter().map(leaf_scalar).collect();
        assert_eq!(values, [Scalar::Int(1), Scalar::Int(2)]);
    }

    #[test]
    fn map_default_becomes_keyed_leaves() {
        let schema = schema_with_field(
            r#"{"name":"f","type":{"type":"map","values":"string"},"default":{"k":"v"}}"#,
        );
        let Node::Map { entries, .. } = field_node(build(&schema, json!({})).unwrap(), "f") else {
            panic!("map expected")
        };
        assert_eq!(leaf_scalar(&entries["k"]), Scalar::Text("v".into()));
    }

    #[test]
    fn record_default_fills_missing_members_from_their_own_defaults() {
        let schema = schema_with_field(
            r#"{"name":"f","type":{"type":"record","name":"R","fields":[
                 {"name":"a","type":"int"},{"name":"b","type":"string","default":"x"}]},
               "default":{"a":7}}"#,
        );
        let Node::Record { fields, .. } = field_node(build(&schema, json!({})).unwrap(), "f")
        else {
            panic!("record expected")
        };
        assert_eq!(leaf_scalar(&fields["a"]), Scalar::Int(7));
        assert_eq!(leaf_scalar(&fields["b"]), Scalar::Text("x".into()));
    }

    #[test]
    fn bytes_and_fixed_defaults_map_code_points_to_bytes() {
        for field in [
            r#"{"name":"f","type":"bytes","default":"\u00ff\u0001"}"#,
            r#"{"name":"f","type":{"type":"fixed","name":"F","size":2},"default":"\u00ff\u0001"}"#,
        ] {
            let schema = schema_with_field(field);
            let leaf = only_leaf(build(&schema, json!({})).unwrap(), "f");
            assert_eq!(leaf, (Scalar::Bytes(vec![0xff, 0x01]), vec![]), "{field}");
        }
    }

    #[test]
    fn bytes_default_above_u00ff_is_rejected() {
        let schema = schema_with_field(r#"{"name":"f","type":"bytes","default":"\u0100"}"#);
        let errors = build(&schema, json!({})).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            "Invalid bytes default for field 'f': U+0100 is above U+00FF"
        );
    }
}
