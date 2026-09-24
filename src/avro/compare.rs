use crate::avro::schema::{Kind, SchemaCtx};
use apache_avro::schema::{RecordField, Schema};
use apache_avro::types::Value;
use bytes::Bytes;
use pact_matching::matchingrules::{
    compare_lists_with_matchingrule, compare_maps_with_matchingrule, match_values,
};
use pact_matching::{CommonMismatch, MatchingContext, Mismatch as CoreMismatch};
use pact_models::matchingrules::{MatchingRule, RuleList};
use pact_models::path_exp::DocPath;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

pub type Mctx<'c> = &'c (dyn MatchingContext + Send + Sync);

#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BodyItem {
    pub key: String,
    pub mismatches: Vec<Mismatch>,
}

#[derive(Debug, Clone, PartialEq)]
struct Elem(Value);

impl Display for Elem {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", show(&self.0))
    }
}

fn unwrap_union(value: &Value) -> &Value {
    match value {
        Value::Union(_, inner) => unwrap_union(inner),
        other => other,
    }
}

fn is_null(value: &Value) -> bool {
    matches!(unwrap_union(value), Value::Null)
}

fn show(value: &Value) -> String {
    match unwrap_union(value) {
        Value::String(text) | Value::Enum(_, text) => text.clone(),
        Value::Null => "null".to_string(),
        Value::Int(number) => number.to_string(),
        Value::Long(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Boolean(flag) => flag.to_string(),
        other => format!("{other:?}"),
    }
}

fn show_list(values: &[Value]) -> String {
    let shown: Vec<String> = values.iter().map(show).collect();
    format!("[{}]", shown.join(", "))
}

fn is_scalar(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::String
            | Kind::Bytes
            | Kind::Int
            | Kind::Long
            | Kind::Float
            | Kind::Double
            | Kind::Boolean
            | Kind::Enum
            | Kind::Fixed
    )
}

fn lookup<'v>(fields: &'v [(String, Value)], name: &str) -> Option<&'v Value> {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn single(path: &DocPath, mismatch: Mismatch) -> BodyItem {
    BodyItem {
        key: path.to_string(),
        mismatches: vec![mismatch],
    }
}

fn at(
    path: &DocPath,
    expected: Option<String>,
    actual: Option<String>,
    message: String,
) -> Mismatch {
    Mismatch {
        expected,
        actual,
        message,
        path: path.to_string(),
    }
}

fn expected_null(path: &DocPath, expected: &Value, type_name: &str) -> BodyItem {
    let message = format!(
        "Expected null (Null) to be equal to '{}' ({type_name})",
        show(expected)
    );
    single(path, at(path, Some(show(expected)), None, message))
}

fn unexpected_value(path: &DocPath, actual: &Value, type_name: &str) -> BodyItem {
    let message = format!(
        "Expected null (Null) but received value '{}' ({type_name})",
        show(actual)
    );
    single(path, at(path, None, Some(show(actual)), message))
}

fn lossy(bytes: Option<Bytes>) -> Option<String> {
    bytes.map(|value| String::from_utf8_lossy(&value).to_string())
}

fn from_common(common: &CommonMismatch) -> Mismatch {
    match common.to_body_mismatch() {
        CoreMismatch::BodyMismatch {
            path,
            expected,
            actual,
            mismatch,
        } => Mismatch {
            expected: lossy(expected),
            actual: lossy(actual),
            message: mismatch,
            path,
        },
        other => at(&DocPath::root(), None, None, other.description()),
    }
}

fn list_level(path: &DocPath, mismatches: &[CommonMismatch]) -> Vec<BodyItem> {
    if mismatches.is_empty() {
        vec![]
    } else {
        vec![BodyItem {
            key: path.to_string(),
            mismatches: mismatches.iter().map(from_common).collect(),
        }]
    }
}

fn numeric_rules(rules: &RuleList) -> Option<RuleList> {
    let kept: Vec<MatchingRule> = rules
        .rules
        .iter()
        .filter(|rule| **rule != MatchingRule::NotEmpty)
        .cloned()
        .collect();
    if kept.is_empty() && !rules.rules.is_empty() {
        None
    } else {
        Some(RuleList {
            rules: kept,
            ..rules.clone()
        })
    }
}

fn numeric(
    rules: &RuleList,
    run: impl FnOnce(&RuleList) -> Result<(), Vec<String>>,
) -> Result<(), Vec<String>> {
    numeric_rules(rules).map_or(Ok(()), |kept| run(&kept))
}

fn match_scalar(
    path: &DocPath,
    rules: &RuleList,
    expected: &Value,
    actual: &Value,
) -> Result<(), Vec<String>> {
    match (unwrap_union(expected), unwrap_union(actual)) {
        (Value::String(e) | Value::Enum(_, e), Value::String(a) | Value::Enum(_, a)) => {
            match_values(path, rules, e.clone(), a.clone())
        }
        (Value::Int(e), Value::Int(a)) => numeric(rules, |kept| match_values(path, kept, *e, *a)),
        (Value::Long(e), Value::Long(a)) => numeric(rules, |kept| match_values(path, kept, *e, *a)),
        (Value::Float(e), Value::Float(a)) => numeric(rules, |kept| {
            match_values(path, kept, f64::from(*e), f64::from(*a))
        }),
        (Value::Double(e), Value::Double(a)) => {
            numeric(rules, |kept| match_values(path, kept, *e, *a))
        }
        (Value::Boolean(e), Value::Boolean(a)) => {
            numeric(rules, |kept| match_values(path, kept, *e, *a))
        }
        (Value::Bytes(e) | Value::Fixed(_, e), Value::Bytes(a) | Value::Fixed(_, a)) => {
            match_values(path, rules, Bytes::from(e.clone()), Bytes::from(a.clone()))
        }
        (e, a) => Err(vec![format!("Unsupported comparison of {e:?} with {a:?}")]),
    }
}

pub struct Comparator<'c, 'a> {
    schemas: &'c SchemaCtx<'a>,
    matching: Mctx<'c>,
}

impl<'c, 'a> Comparator<'c, 'a> {
    pub fn new(schemas: &'c SchemaCtx<'a>, matching: Mctx<'c>) -> Self {
        Self { schemas, matching }
    }

    pub fn compare_record(
        &self,
        record: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        match (record, expected, actual) {
            (Schema::Record(schema), Value::Record(exp), Value::Record(act)) => schema
                .fields
                .iter()
                .flat_map(|field| {
                    self.compare_field(
                        field,
                        path,
                        lookup(exp, &field.name),
                        lookup(act, &field.name),
                    )
                })
                .collect(),
            _ => vec![],
        }
    }

    fn compare_field(
        &self,
        field: &'a RecordField,
        path: &DocPath,
        expected: Option<&Value>,
        actual: Option<&Value>,
    ) -> Vec<BodyItem> {
        let Some(expected) = expected else {
            return vec![];
        };
        let null = Value::Null;
        let actual = actual.unwrap_or(&null);
        let field_path = path.join(field.name.as_str());
        self.compare_typed(&field.name, &field.schema, &field_path, expected, actual)
    }

    fn compare_typed(
        &self,
        name: &str,
        schema: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        match self.schemas.kind_of(schema) {
            Kind::Array => self.compare_array(name, schema, path, expected, actual),
            Kind::Map => self.compare_map(name, schema, path, expected, actual),
            Kind::Record => self.compare_nested(schema, path, expected, actual),
            Kind::Union => match self.schemas.nullable_branch(schema) {
                Some(branch) => self.compare_typed(
                    name,
                    branch,
                    path,
                    unwrap_union(expected),
                    unwrap_union(actual),
                ),
                None => {
                    tracing::warn!("Field.compare doesn't support non-nullable union types");
                    vec![]
                }
            },
            kind if is_scalar(kind) => {
                vec![self.compare_value(path, kind, expected, actual)]
            }
            other => {
                tracing::warn!("Field.compare doesn't support type: {}", other.name());
                vec![]
            }
        }
    }

    fn compare_nested(
        &self,
        schema: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        match (is_null(expected), is_null(actual)) {
            (true, true) => vec![],
            (true, false) => vec![unexpected_value(path, actual, "Record")],
            (false, true) => vec![expected_null(path, expected, "Record")],
            (false, false) => {
                self.compare_record(self.schemas.resolve(schema), path, expected, actual)
            }
        }
    }

    fn compare_element(
        &self,
        schema: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        match self.schemas.kind_of(schema) {
            Kind::Record => {
                self.compare_record(self.schemas.resolve(schema), path, expected, actual)
            }
            kind if is_scalar(kind) => vec![self.compare_value(path, kind, expected, actual)],
            other => {
                tracing::warn!("Field.compare doesn't support type: {}", other.name());
                vec![]
            }
        }
    }

    pub fn compare_value(
        &self,
        path: &DocPath,
        kind: Kind,
        expected: &Value,
        actual: &Value,
    ) -> BodyItem {
        let mismatches = if is_null(actual) && !is_null(expected) {
            vec![at(
                path,
                Some(show(expected)),
                None,
                format!(
                    "Expected null (Null) to be equal to '{}' ({})",
                    show(expected),
                    kind.name()
                ),
            )]
        } else if self.matching.matcher_is_defined(path) {
            self.rule_mismatches(path, expected, actual)
        } else if unwrap_union(expected) == unwrap_union(actual) {
            vec![]
        } else {
            vec![at(
                path,
                Some(show(expected)),
                Some(show(actual)),
                format!(
                    "Expected '{}' ({}) but received value '{}'",
                    show(expected),
                    kind.name(),
                    show(actual)
                ),
            )]
        };
        BodyItem {
            key: path.to_string(),
            mismatches,
        }
    }

    fn rule_mismatches(&self, path: &DocPath, expected: &Value, actual: &Value) -> Vec<Mismatch> {
        let rules = self.matching.select_best_matcher(path);
        match_scalar(path, &rules, expected, actual)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|message| at(path, Some(show(expected)), Some(show(actual)), message))
            .collect()
    }
}

fn elems(values: &[Value]) -> Vec<Elem> {
    values.iter().cloned().map(Elem).collect()
}

fn sorted(entries: &std::collections::HashMap<String, Value>) -> BTreeMap<String, Elem> {
    entries
        .iter()
        .map(|(key, value)| (key.clone(), Elem(value.clone())))
        .collect()
}

fn keys(entries: &BTreeMap<String, Elem>) -> BTreeSet<String> {
    entries.keys().cloned().collect()
}

impl<'c, 'a> Comparator<'c, 'a> {
    fn compare_array(
        &self,
        name: &str,
        schema: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        let Schema::Array(array) = self.schemas.resolve(schema) else {
            return vec![];
        };
        let items = self.schemas.resolve(&array.items);
        match (expected, actual) {
            (Value::Array(exp), Value::Array(act)) => {
                self.compare_lists(name, items, path, exp, act)
            }
            (Value::Array(_), _) => vec![expected_null(path, expected, "Array")],
            (_, Value::Array(_)) => vec![unexpected_value(path, actual, "Array")],
            _ => vec![],
        }
    }

    fn compare_lists(
        &self,
        name: &str,
        items: &'a Schema,
        path: &DocPath,
        exp: &[Value],
        act: &[Value],
    ) -> Vec<BodyItem> {
        if exp.is_empty() && !act.is_empty() {
            let message = format!(
                "Expected repeated field '{name}' to be empty but received {}",
                show_list(act)
            );
            return vec![single(
                path,
                at(path, Some(show_list(exp)), Some(show_list(act)), message),
            )];
        }
        if self.matching.matcher_is_defined(path) {
            self.lists_with_rules(items, path, exp, act)
        } else {
            self.lists_by_index(name, items, path, exp, act)
        }
    }

    fn lists_with_rules(
        &self,
        items: &'a Schema,
        path: &DocPath,
        exp: &[Value],
        act: &[Value],
    ) -> Vec<BodyItem> {
        let rules = self.matching.select_best_matcher(path);
        let (expected, actual) = (elems(exp), elems(act));
        let mut collected: Vec<BodyItem> = vec![];
        let level: Vec<CommonMismatch> = rules
            .rules
            .iter()
            .flat_map(|rule| {
                let mut callback = |p: &DocPath,
                                    e: &Elem,
                                    a: &Elem,
                                    _c: Mctx<'_>|
                 -> Result<(), Vec<CommonMismatch>> {
                    collected.extend(self.compare_element(items, p, &e.0, &a.0));
                    Ok(())
                };
                compare_lists_with_matchingrule(
                    rule,
                    path,
                    &expected,
                    &actual,
                    self.matching,
                    rules.cascaded,
                    &mut callback,
                )
                .err()
                .unwrap_or_default()
            })
            .collect();
        collected.extend(list_level(path, &level));
        collected
    }

    fn lists_by_index(
        &self,
        name: &str,
        items: &'a Schema,
        path: &DocPath,
        exp: &[Value],
        act: &[Value],
    ) -> Vec<BodyItem> {
        let mut found: Vec<BodyItem> = exp
            .iter()
            .zip(act.iter())
            .enumerate()
            .flat_map(|(index, (e, a))| self.compare_element(items, &path.join_index(index), e, a))
            .collect();
        if exp.len() != act.len() {
            let message = format!(
                "Expected repeated field '{name}' to have {} values but received {} values",
                exp.len(),
                act.len()
            );
            found.push(single(
                path,
                at(path, Some(show_list(exp)), Some(show_list(act)), message),
            ));
        }
        found
    }

    fn compare_map(
        &self,
        name: &str,
        schema: &'a Schema,
        path: &DocPath,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        let Schema::Map(map) = self.schemas.resolve(schema) else {
            return vec![];
        };
        let values = self.schemas.resolve(&map.types);
        match (expected, actual) {
            (Value::Map(exp), Value::Map(act)) => {
                self.compare_entries(name, values, path, &sorted(exp), &sorted(act))
            }
            (Value::Map(_), _) => vec![expected_null(path, expected, "Map")],
            (_, Value::Map(_)) => vec![unexpected_value(path, actual, "Map")],
            _ => vec![],
        }
    }

    fn compare_entries(
        &self,
        name: &str,
        values: &'a Schema,
        path: &DocPath,
        exp: &BTreeMap<String, Elem>,
        act: &BTreeMap<String, Elem>,
    ) -> Vec<BodyItem> {
        if exp.is_empty() && !act.is_empty() {
            let message = format!(
                "Expected Map field '{name}' to be empty but received {}",
                show_entries(act)
            );
            return vec![single(
                path,
                at(path, None, Some(show_entries(act)), message),
            )];
        }
        if self.matching.matcher_is_defined(path) {
            self.entries_with_rules(values, path, exp, act)
        } else {
            self.entries_plain(name, values, path, exp, act)
        }
    }

    fn entries_with_rules(
        &self,
        values: &'a Schema,
        path: &DocPath,
        exp: &BTreeMap<String, Elem>,
        act: &BTreeMap<String, Elem>,
    ) -> Vec<BodyItem> {
        let rules = self.matching.select_best_matcher(path);
        let mut collected: Vec<BodyItem> = vec![];
        let level: Vec<CommonMismatch> = rules
            .rules
            .iter()
            .flat_map(|rule| {
                let mut callback = |p: &DocPath,
                                    e: &Elem,
                                    a: &Elem,
                                    _c: Mctx<'_>|
                 -> Result<(), Vec<CommonMismatch>> {
                    collected.extend(self.compare_element(values, p, &e.0, &a.0));
                    Ok(())
                };
                compare_maps_with_matchingrule(
                    rule,
                    rules.cascaded,
                    path,
                    exp,
                    act,
                    self.matching,
                    &mut callback,
                )
                .err()
                .unwrap_or_default()
            })
            .collect();
        collected.extend(list_level(path, &level));
        collected
    }

    fn entries_plain(
        &self,
        name: &str,
        values: &'a Schema,
        path: &DocPath,
        exp: &BTreeMap<String, Elem>,
        act: &BTreeMap<String, Elem>,
    ) -> Vec<BodyItem> {
        let key_items = self
            .matching
            .match_keys(path, &keys(exp), &keys(act))
            .err()
            .map(|found| list_level(path, &found))
            .unwrap_or_default();
        let entry_items = exp.iter().flat_map(|(key, e)| match act.get(key) {
            Some(a) => self.compare_element(values, &path.join(key.as_str()), &e.0, &a.0),
            None => vec![missing_entry(path, name, key, &e.0)],
        });
        key_items.into_iter().chain(entry_items).collect()
    }
}

fn show_entries(entries: &BTreeMap<String, Elem>) -> String {
    let shown: Vec<String> = entries
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    format!("{{{}}}", shown.join(", "))
}

fn missing_entry(path: &DocPath, name: &str, key: &str, expected: &Value) -> BodyItem {
    let message = format!("Expected map field '{name}' to have entry '{key}', but was missing");
    single(path, at(path, Some(show(expected)), None, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_str;
    use pact_matching::{CoreMatchingContext, DiffConfig};
    use pact_models::matchingrules::{MatchingRule, MatchingRuleCategory, RuleLogic};
    use std::collections::HashMap;

    fn schema_with_field(field: &str) -> Schema {
        parse_str(&format!(
            r#"{{"namespace":"com.example","type":"record","name":"Parent","fields":[{field}]}}"#
        ))
        .unwrap()
    }

    fn context(rules: Vec<(&str, MatchingRule)>, config: DiffConfig) -> CoreMatchingContext {
        let mut category = MatchingRuleCategory::empty("body");
        for (path, rule) in rules {
            category.add_rule(DocPath::new(path).unwrap(), rule, RuleLogic::And);
        }
        CoreMatchingContext::new(config, &category, &HashMap::new())
    }

    fn record(fields: Vec<(&str, Value)>) -> Value {
        Value::Record(
            fields
                .into_iter()
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
        )
    }

    fn run(
        schema: &Schema,
        ctx: &CoreMatchingContext,
        expected: &Value,
        actual: &Value,
    ) -> Vec<BodyItem> {
        let schemas = SchemaCtx::new(schema).unwrap();
        let parent = schemas.find_record("Parent").unwrap();
        Comparator::new(&schemas, ctx).compare_record(parent, &DocPath::root(), expected, actual)
    }

    fn keys_of(items: &[BodyItem]) -> Vec<&str> {
        items.iter().map(|item| item.key.as_str()).collect()
    }

    fn failing(items: &[BodyItem]) -> Vec<&BodyItem> {
        items
            .iter()
            .filter(|item| !item.mismatches.is_empty())
            .collect()
    }

    #[test]
    fn equal_scalars_produce_an_empty_item_for_their_path() {
        let schema = schema_with_field(r#"{"name":"street","type":"string"}"#);
        let ctx = context(
            vec![("$.street", MatchingRule::Equality)],
            DiffConfig::NoUnexpectedKeys,
        );
        let value = record(vec![("street", Value::String("hello".into()))]);
        assert_eq!(
            run(&schema, &ctx, &value, &value),
            vec![BodyItem {
                key: "$.street".into(),
                mismatches: vec![]
            }]
        );
    }

    #[test]
    fn not_empty_accepts_any_numeric_value() {
        let schema = schema_with_field(r#"{"name":"id","type":"long"}"#);
        let ctx = context(
            vec![("$.id", MatchingRule::NotEmpty)],
            DiffConfig::NoUnexpectedKeys,
        );
        let expected = record(vec![("id", Value::Long(100))]);
        let actual = record(vec![("id", Value::Long(7))]);
        assert!(failing(&run(&schema, &ctx, &expected, &actual)).is_empty());
    }

    #[test]
    fn scalar_types_compare_with_equality_rules() {
        let cases = [
            (
                r#"{"name":"f","type":"string"}"#,
                Value::String("a".into()),
                Value::String("b".into()),
            ),
            (r#"{"name":"f","type":"int"}"#, Value::Int(1), Value::Int(2)),
            (
                r#"{"name":"f","type":"long"}"#,
                Value::Long(1),
                Value::Long(2),
            ),
            (
                r#"{"name":"f","type":"float"}"#,
                Value::Float(1.5),
                Value::Float(2.5),
            ),
            (
                r#"{"name":"f","type":"double"}"#,
                Value::Double(1.5),
                Value::Double(2.5),
            ),
            (
                r#"{"name":"f","type":"boolean"}"#,
                Value::Boolean(true),
                Value::Boolean(false),
            ),
            (
                r#"{"name":"f","type":{"type":"enum","name":"C","symbols":["A","B"]}}"#,
                Value::Enum(0, "A".into()),
                Value::Enum(1, "B".into()),
            ),
            (
                r#"{"name":"f","type":"bytes"}"#,
                Value::Bytes(vec![1]),
                Value::Bytes(vec![2]),
            ),
            (
                r#"{"name":"f","type":{"type":"fixed","name":"F","size":1}}"#,
                Value::Fixed(1, vec![1]),
                Value::Fixed(1, vec![2]),
            ),
        ];
        for (field, expected, other) in cases {
            let schema = schema_with_field(field);
            let ctx = context(
                vec![("$.f", MatchingRule::Equality)],
                DiffConfig::NoUnexpectedKeys,
            );
            let same = run(
                &schema,
                &ctx,
                &record(vec![("f", expected.clone())]),
                &record(vec![("f", expected.clone())]),
            );
            assert!(failing(&same).is_empty(), "{field}: equal values must pass");
            let differ = run(
                &schema,
                &ctx,
                &record(vec![("f", expected)]),
                &record(vec![("f", other)]),
            );
            assert_eq!(
                failing(&differ).len(),
                1,
                "{field}: unequal values must fail"
            );
            assert_eq!(failing(&differ)[0].key, "$.f");
        }
    }

    #[test]
    fn without_rules_unequal_scalars_use_the_equality_message() {
        let schema = schema_with_field(r#"{"name":"street","type":"string"}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("street", Value::String("hello".into()))]),
            &record(vec![("street", Value::String("other".into()))]),
        );
        assert_eq!(
            items[0].mismatches[0].message,
            "Expected 'hello' (STRING) but received value 'other'"
        );
    }

    #[test]
    fn a_missing_actual_value_is_a_null_mismatch() {
        let schema = schema_with_field(r#"{"name":"street","type":"string"}"#);
        let ctx = context(
            vec![("$.street", MatchingRule::Equality)],
            DiffConfig::NoUnexpectedKeys,
        );
        let items = run(
            &schema,
            &ctx,
            &record(vec![("street", Value::String("hello".into()))]),
            &record(vec![]),
        );
        assert_eq!(
            items[0].mismatches[0].message,
            "Expected null (Null) to be equal to 'hello' (STRING)"
        );
        assert_eq!(items[0].mismatches[0].actual, None);
    }

    #[test]
    fn nullable_union_field_reports_mismatch_when_values_differ() {
        let schema = schema_with_field(r#"{"name":"z","type":["null","int"]}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let expected = record(vec![("z", Value::Union(1, Box::new(Value::Int(1))))]);
        let actual = record(vec![("z", Value::Union(1, Box::new(Value::Int(2))))]);
        assert_eq!(failing(&run(&schema, &ctx, &expected, &actual)).len(), 1);
    }

    #[test]
    fn nullable_union_field_is_silent_when_values_match() {
        let schema = schema_with_field(r#"{"name":"z","type":["null","int"]}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let value = record(vec![("z", Value::Union(1, Box::new(Value::Int(1))))]);
        assert!(failing(&run(&schema, &ctx, &value, &value)).is_empty());
    }

    #[test]
    fn nullable_union_record_branch_is_silent_when_both_null() {
        let schema = schema_with_field(
            r#"{"name":"address","type":["null",{"type":"record","name":"M","fields":[{"name":"street","type":"string"}]}]}"#,
        );
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let value = record(vec![("address", Value::Union(0, Box::new(Value::Null)))]);
        assert!(failing(&run(&schema, &ctx, &value, &value)).is_empty());
    }

    #[test]
    fn nullable_union_record_branch_reports_mismatch_when_expected_is_null_but_actual_has_a_value()
    {
        let schema = schema_with_field(
            r#"{"name":"address","type":["null",{"type":"record","name":"M","fields":[{"name":"street","type":"string"}]}]}"#,
        );
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let expected = record(vec![("address", Value::Union(0, Box::new(Value::Null)))]);
        let actual = record(vec![(
            "address",
            Value::Union(
                1,
                Box::new(record(vec![("street", Value::String("x".into()))])),
            ),
        )]);
        assert_eq!(failing(&run(&schema, &ctx, &expected, &actual)).len(), 1);
    }

    #[test]
    fn nullable_union_array_branch_is_silent_when_both_null() {
        let schema = schema_with_field(
            r#"{"name":"names","type":["null",{"type":"array","items":"string"}]}"#,
        );
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let value = record(vec![("names", Value::Union(0, Box::new(Value::Null)))]);
        assert!(failing(&run(&schema, &ctx, &value, &value)).is_empty());
    }

    #[test]
    fn nullable_union_array_branch_reports_mismatch_when_expected_is_null_but_actual_has_a_value() {
        let schema = schema_with_field(
            r#"{"name":"names","type":["null",{"type":"array","items":"string"}]}"#,
        );
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let expected = record(vec![("names", Value::Union(0, Box::new(Value::Null)))]);
        let actual = record(vec![(
            "names",
            Value::Union(1, Box::new(Value::Array(vec![Value::String("a".into())]))),
        )]);
        assert_eq!(failing(&run(&schema, &ctx, &expected, &actual)).len(), 1);
    }

    #[test]
    fn nullable_union_map_branch_is_silent_when_both_null() {
        let schema =
            schema_with_field(r#"{"name":"ages","type":["null",{"type":"map","values":"int"}]}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let value = record(vec![("ages", Value::Union(0, Box::new(Value::Null)))]);
        assert!(failing(&run(&schema, &ctx, &value, &value)).is_empty());
    }

    #[test]
    fn nullable_union_map_branch_reports_mismatch_when_expected_is_null_but_actual_has_a_value() {
        let schema =
            schema_with_field(r#"{"name":"ages","type":["null",{"type":"map","values":"int"}]}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let expected = record(vec![("ages", Value::Union(0, Box::new(Value::Null)))]);
        let actual = record(vec![(
            "ages",
            Value::Union(1, Box::new(ages(&[("first", 1)]))),
        )]);
        assert_eq!(failing(&run(&schema, &ctx, &expected, &actual)).len(), 1);
    }

    #[test]
    fn nested_record_fields_use_dotted_paths() {
        let schema = schema_with_field(
            r#"{"name":"address","type":{"type":"record","name":"M","fields":[{"name":"street","type":"string"}]}}"#,
        );
        let ctx = context(
            vec![("$.address.street", MatchingRule::Equality)],
            DiffConfig::NoUnexpectedKeys,
        );
        let make = |s: &str| {
            record(vec![(
                "address",
                record(vec![("street", Value::String(s.into()))]),
            )])
        };
        assert_eq!(
            keys_of(&run(&schema, &ctx, &make("a"), &make("a"))),
            ["$.address.street"]
        );
        assert_eq!(
            failing(&run(&schema, &ctx, &make("a"), &make("b"))).len(),
            1
        );
    }

    fn strings(values: &[&str]) -> Value {
        Value::Array(values.iter().map(|v| Value::String((*v).into())).collect())
    }

    #[test]
    fn arrays_compare_by_index_without_rules() {
        let schema =
            schema_with_field(r#"{"name":"names","type":{"type":"array","items":"string"}}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("names", strings(&["a", "b"]))]),
            &record(vec![("names", strings(&["a", "x"]))]),
        );
        assert_eq!(keys_of(&items), ["$.names[0]", "$.names[1]"]);
        assert_eq!(failing(&items)[0].key, "$.names[1]");
    }

    #[test]
    fn arrays_of_different_length_report_a_size_mismatch() {
        let schema =
            schema_with_field(r#"{"name":"names","type":{"type":"array","items":"string"}}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("names", strings(&["a"]))]),
            &record(vec![("names", strings(&["a", "b"]))]),
        );
        let last = items.last().unwrap();
        assert_eq!(last.key, "$.names");
        assert_eq!(
            last.mismatches[0].message,
            "Expected repeated field 'names' to have 1 values but received 2 values"
        );
    }

    #[test]
    fn an_expected_empty_array_rejects_a_non_empty_actual() {
        let schema =
            schema_with_field(r#"{"name":"names","type":{"type":"array","items":"string"}}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("names", strings(&[]))]),
            &record(vec![("names", strings(&["a"]))]),
        );
        assert_eq!(
            items[0].mismatches[0].message,
            "Expected repeated field 'names' to be empty but received [a]"
        );
    }

    #[test]
    fn array_rules_apply_to_every_actual_element() {
        let schema =
            schema_with_field(r#"{"name":"names","type":{"type":"array","items":"string"}}"#);
        let ctx = context(
            vec![
                ("$.names", MatchingRule::MinType(1)),
                ("$.names[*]", MatchingRule::Type),
            ],
            DiffConfig::NoUnexpectedKeys,
        );
        let items = run(
            &schema,
            &ctx,
            &record(vec![("names", strings(&["a"]))]),
            &record(vec![("names", strings(&["x", "y"]))]),
        );
        assert_eq!(keys_of(&items), ["$.names[0]", "$.names[1]"]);
        assert!(failing(&items).is_empty());
    }

    #[test]
    fn array_of_records_keys_include_the_index_and_field() {
        let schema = schema_with_field(
            r#"{"name":"addresses","type":{"type":"array","items":{"type":"record","name":"A","fields":[{"name":"street","type":"string"}]}}}"#,
        );
        let ctx = context(
            vec![("$.addresses[0].street", MatchingRule::Equality)],
            DiffConfig::NoUnexpectedKeys,
        );
        let make = |s: &str| {
            record(vec![(
                "addresses",
                Value::Array(vec![record(vec![("street", Value::String(s.into()))])]),
            )])
        };
        assert_eq!(
            keys_of(&run(&schema, &ctx, &make("a"), &make("a"))),
            ["$.addresses[0].street"]
        );
        assert_eq!(
            failing(&run(&schema, &ctx, &make("a"), &make("b"))).len(),
            1
        );
    }

    fn ages(entries: &[(&str, i32)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| ((*k).to_string(), Value::Int(*v)))
                .collect(),
        )
    }

    #[test]
    fn maps_compare_matching_keys_by_value() {
        let schema = schema_with_field(r#"{"name":"ages","type":{"type":"map","values":"int"}}"#);
        let ctx = context(vec![], DiffConfig::NoUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("ages", ages(&[("first", 2), ("second", 3)]))]),
            &record(vec![("ages", ages(&[("first", 2), ("second", 9)]))]),
        );
        assert_eq!(keys_of(&items), ["$.ages.first", "$.ages.second"]);
        assert_eq!(failing(&items)[0].key, "$.ages.second");
    }

    #[test]
    fn a_missing_map_entry_is_reported_on_the_map_path() {
        let schema = schema_with_field(r#"{"name":"ages","type":{"type":"map","values":"int"}}"#);
        let ctx = context(vec![], DiffConfig::AllowUnexpectedKeys);
        let items = run(
            &schema,
            &ctx,
            &record(vec![("ages", ages(&[("first", 2), ("second", 3)]))]),
            &record(vec![("ages", ages(&[("first", 2)]))]),
        );
        let missing = failing(&items);
        assert!(missing.iter().all(|item| item.key == "$.ages"));
        assert!(missing
            .iter()
            .flat_map(|item| &item.mismatches)
            .any(|m| m.message
                == "Expected map field 'ages' to have entry 'second', but was missing"));
    }

    #[test]
    fn unexpected_map_keys_fail_unless_allowed() {
        let schema = schema_with_field(r#"{"name":"ages","type":{"type":"map","values":"int"}}"#);
        let expected = record(vec![("ages", ages(&[("first", 2)]))]);
        let actual = record(vec![("ages", ages(&[("first", 2), ("extra", 1)]))]);
        let strict = context(vec![], DiffConfig::NoUnexpectedKeys);
        assert_eq!(failing(&run(&schema, &strict, &expected, &actual)).len(), 1);
        let lenient = context(vec![], DiffConfig::AllowUnexpectedKeys);
        assert!(failing(&run(&schema, &lenient, &expected, &actual)).is_empty());
    }

    #[test]
    fn map_of_records_keys_include_the_entry_key() {
        let schema = schema_with_field(
            r#"{"name":"addresses","type":{"type":"map","values":{"type":"record","name":"A","fields":[{"name":"street","type":"string"}]}}}"#,
        );
        let ctx = context(
            vec![("$.addresses.first.street", MatchingRule::Equality)],
            DiffConfig::NoUnexpectedKeys,
        );
        let make = |s: &str| {
            record(vec![(
                "addresses",
                Value::Map(
                    [(
                        "first".to_string(),
                        record(vec![("street", Value::String(s.into()))]),
                    )]
                    .into(),
                ),
            )])
        };
        assert_eq!(
            keys_of(&run(&schema, &ctx, &make("a"), &make("a"))),
            ["$.addresses.first.street"]
        );
        assert_eq!(
            failing(&run(&schema, &ctx, &make("a"), &make("b"))).len(),
            1
        );
    }
}
