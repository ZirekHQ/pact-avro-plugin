use crate::avro::path::PactFieldPath;
use crate::avro::schema::{Kind, SchemaCtx};
use crate::error::PluginError;
use apache_avro::schema::{EnumSchema, RecordSchema, Schema};
use apache_avro::types::Value;
use pact_models::matchingrules::MatchingRule;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Null,
    Boolean(bool),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf {
        path: PactFieldPath,
        value: Scalar,
        rules: Vec<MatchingRule>,
    },
    Array {
        path: PactFieldPath,
        items: Vec<Node>,
    },
    Map {
        path: PactFieldPath,
        entries: BTreeMap<String, Node>,
    },
    Record {
        path: PactFieldPath,
        fields: BTreeMap<String, Node>,
    },
}

type Rules = BTreeMap<String, Vec<MatchingRule>>;

impl Node {
    pub fn null_leaf(path: PactFieldPath) -> Node {
        Node::Leaf {
            path,
            value: Scalar::Null,
            rules: vec![],
        }
    }

    pub fn with_path(self, new_path: PactFieldPath) -> Node {
        match self {
            Node::Leaf { value, rules, .. } => Node::Leaf {
                path: new_path,
                value,
                rules,
            },
            Node::Array { items, .. } => Node::Array {
                path: new_path,
                items,
            },
            Node::Map { entries, .. } => Node::Map {
                path: new_path,
                entries,
            },
            record => record,
        }
    }

    pub fn rules_by_path(&self) -> Rules {
        let mut collected = Rules::new();
        self.collect_rules(&mut collected);
        collected
    }

    fn collect_rules(&self, collected: &mut Rules) {
        match self {
            Node::Leaf { path, rules, .. } => collected
                .entry(path.to_json_path())
                .or_default()
                .extend(rules.iter().cloned()),
            Node::Array { items, .. } => {
                items.iter().for_each(|item| item.collect_rules(collected))
            }
            Node::Map { entries, .. } => entries
                .values()
                .for_each(|node| node.collect_rules(collected)),
            Node::Record { fields, .. } => fields
                .values()
                .for_each(|node| node.collect_rules(collected)),
        }
    }
}

fn mismatch(schema: &Schema) -> PluginError {
    PluginError::Message(format!(
        "Configured value does not fit schema of type '{}'",
        crate::avro::schema::kind(schema).name()
    ))
}

pub fn to_value<'a>(
    ctx: &SchemaCtx<'a>,
    schema: &'a Schema,
    node: &Node,
) -> Result<Value, PluginError> {
    let schema = ctx.resolve(schema);
    match (schema, node) {
        (Schema::Union(union), _) => union_value(ctx, union.variants(), node),
        (Schema::Record(record), Node::Record { fields, .. }) => record_value(ctx, record, fields),
        (Schema::Array(array), Node::Array { items, .. }) => items
            .iter()
            .map(|item| to_value(ctx, &array.items, item))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        (Schema::Map(map), Node::Map { entries, .. }) => entries
            .iter()
            .map(|(key, item)| to_value(ctx, &map.types, item).map(|value| (key.clone(), value)))
            .collect::<Result<HashMap<_, _>, _>>()
            .map(Value::Map),
        (_, Node::Leaf { value, .. }) => leaf_value(schema, value),
        (other, _) => Err(mismatch(other)),
    }
}

fn union_value<'a>(
    ctx: &SchemaCtx<'a>,
    variants: &'a [Schema],
    node: &Node,
) -> Result<Value, PluginError> {
    let wants_null = matches!(
        node,
        Node::Leaf {
            value: Scalar::Null,
            ..
        }
    );
    variants
        .iter()
        .position(|variant| (ctx.kind_of(variant) == Kind::Null) == wants_null)
        .ok_or_else(|| {
            PluginError::Message("Union schema has no branch for the configured value".to_string())
        })
        .and_then(|index| {
            to_value(ctx, &variants[index], node)
                .map(|inner| Value::Union(index as u32, Box::new(inner)))
        })
}

fn record_value<'a>(
    ctx: &SchemaCtx<'a>,
    record: &'a RecordSchema,
    fields: &BTreeMap<String, Node>,
) -> Result<Value, PluginError> {
    record
        .fields
        .iter()
        .map(|field| {
            fields
                .get(&field.name)
                .ok_or_else(|| {
                    PluginError::Exception(format!(
                        "Couldn't find configuration for field: {}",
                        field.name
                    ))
                })
                .and_then(|node| to_value(ctx, &field.schema, node))
                .map(|value| (field.name.clone(), value))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Record)
}

fn enum_value(schema: &EnumSchema, symbol: &str) -> Result<Value, PluginError> {
    schema
        .symbols
        .iter()
        .position(|candidate| candidate == symbol)
        .map(|index| Value::Enum(index as u32, symbol.to_string()))
        .ok_or_else(|| PluginError::Exception(format!("Not an enum: {symbol}")))
}

fn leaf_value(schema: &Schema, scalar: &Scalar) -> Result<Value, PluginError> {
    match (schema, scalar) {
        (Schema::Null, Scalar::Null) => Ok(Value::Null),
        (Schema::Boolean, Scalar::Boolean(flag)) => Ok(Value::Boolean(*flag)),
        (Schema::Int, Scalar::Int(number)) => Ok(Value::Int(*number)),
        (Schema::Long, Scalar::Long(number)) => Ok(Value::Long(*number)),
        (Schema::Float, Scalar::Float(number)) => Ok(Value::Float(*number)),
        (Schema::Double, Scalar::Double(number)) => Ok(Value::Double(*number)),
        (Schema::String, Scalar::Text(text)) => Ok(Value::String(text.clone())),
        (Schema::Bytes, Scalar::Bytes(bytes)) => Ok(Value::Bytes(bytes.clone())),
        (Schema::Fixed(fixed), Scalar::Bytes(bytes)) => Ok(Value::Fixed(fixed.size, bytes.clone())),
        (Schema::Enum(symbols), Scalar::Text(symbol)) => enum_value(symbols, symbol),
        (other, _) => Err(mismatch(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::codec::{decode, encode};
    use crate::avro::schema::parse_str;

    fn path(dotted: &str) -> PactFieldPath {
        PactFieldPath::parse(dotted)
    }

    fn leaf(dotted: &str, value: Scalar, rules: Vec<MatchingRule>) -> Node {
        Node::Leaf {
            path: path(dotted),
            value,
            rules,
        }
    }

    fn record_node(fields: Vec<(&str, Node)>) -> Node {
        Node::Record {
            path: PactFieldPath::root(),
            fields: fields
                .into_iter()
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn every_leaf_path_is_a_rule_key_even_without_rules() {
        let node = record_node(vec![
            (
                "a",
                leaf(
                    "$.a",
                    Scalar::Text("x".into()),
                    vec![MatchingRule::Equality],
                ),
            ),
            ("b", leaf("$.b", Scalar::Null, vec![])),
        ]);
        let rules = node.rules_by_path();
        assert_eq!(rules.get("$.a"), Some(&vec![MatchingRule::Equality]));
        assert_eq!(rules.get("$.b"), Some(&vec![]));
    }

    #[test]
    fn duplicate_paths_accumulate_rules() {
        let node = Node::Array {
            path: path("$.names"),
            items: vec![
                leaf(
                    "$.names.0",
                    Scalar::Text("a".into()),
                    vec![MatchingRule::NotEmpty],
                ),
                leaf(
                    "$.names.0",
                    Scalar::Text("b".into()),
                    vec![MatchingRule::NotEmpty],
                ),
            ],
        };
        assert_eq!(node.rules_by_path().get("$.names.0").map(Vec::len), Some(2));
    }

    #[test]
    fn with_path_replaces_leaf_paths_but_not_record_paths() {
        let moved = leaf("$.a", Scalar::Null, vec![]).with_path(path("$.a.0"));
        assert_eq!(moved, leaf("$.a.0", Scalar::Null, vec![]));
        let record = record_node(vec![]);
        assert_eq!(record.clone().with_path(path("$.x")), record);
    }

    #[test]
    fn complex_shapes_encode_and_decode() {
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
        let item = record_node(vec![("id", leaf("$.items.0.id", Scalar::Long(7), vec![]))]);
        let node = record_node(vec![
            (
                "items",
                Node::Array {
                    path: path("$.items"),
                    items: vec![item],
                },
            ),
            ("zipcode", leaf("$.zipcode", Scalar::Null, vec![])),
            (
                "color",
                leaf("$.color", Scalar::Text("GREEN".into()), vec![]),
            ),
            (
                "md5",
                leaf("$.md5", Scalar::Bytes(b"abcd".to_vec()), vec![]),
            ),
            (
                "ages",
                Node::Map {
                    path: path("$.ages"),
                    entries: [(
                        "first".to_string(),
                        leaf("$.ages.first", Scalar::Int(2), vec![]),
                    )]
                    .into(),
                },
            ),
        ]);
        let value = to_value(&ctx, record, &node).unwrap();
        let bytes = encode(&ctx, record, value.clone()).unwrap();
        assert_eq!(decode(&ctx, record, &bytes).unwrap(), value);
    }

    #[test]
    fn nullable_union_uses_the_null_branch_index_wherever_it_sits() {
        let schema = parse_str(
            r#"{"type":"record","name":"R","fields":[{"name":"z","type":["bytes","null"]}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let node = record_node(vec![("z", leaf("$.z", Scalar::Null, vec![]))]);
        let Value::Record(fields) = to_value(&ctx, &schema, &node).unwrap() else {
            panic!("record expected")
        };
        assert_eq!(fields[0].1, Value::Union(1, Box::new(Value::Null)));
    }

    #[test]
    fn unknown_enum_symbol_is_an_error() {
        let schema = parse_str(
            r#"{"type":"record","name":"R","fields":[{"name":"c","type":{"type":"enum","name":"C","symbols":["A"]}}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let node = record_node(vec![("c", leaf("$.c", Scalar::Text("Z".into()), vec![]))]);
        assert_eq!(
            to_value(&ctx, &schema, &node).unwrap_err().to_string(),
            "Not an enum: Z"
        );
    }
}
