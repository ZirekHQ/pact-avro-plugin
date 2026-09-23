use crate::avro::codec::decode;
use crate::avro::compare::{BodyItem, Comparator, Mismatch};
use crate::avro::rules::rule_from_proto;
use crate::avro::schema::SchemaCtx;
use crate::constants::{CONTENT_TYPES, CONTENT_TYPES_STR, MATCHING_RULE_CATEGORY_NAME};
use crate::error::PluginError;
use crate::pact_plugin::{
    Body, CompareContentsRequest, CompareContentsResponse, ContentMismatch, ContentMismatches,
    MatchingRules,
};
use apache_avro::schema::Schema;
use pact_matching::{CoreMatchingContext, DiffConfig};
use pact_models::matchingrules::{MatchingRuleCategory, RuleLogic};
use pact_models::path_exp::DocPath;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static CONTENT_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\w+/\*?\+?\w+);\s*record=(\w+)$").expect("static regex is valid")
});

fn message(text: impl Into<String>) -> PluginError {
    PluginError::Message(text.into())
}

pub(crate) fn record_name(body: &Body, label: &str) -> Result<String, PluginError> {
    let Some(captures) = CONTENT_TYPE.captures(&body.content_type) else {
        return Err(message(format!(
            "{label} body content type didn't match expected template of 'content/type; record=NameOfRecord'"
        )));
    };
    if CONTENT_TYPES.contains(&&captures[1]) {
        Ok(captures[2].to_string())
    } else {
        Err(message(format!(
            "{label} body is not one of '{CONTENT_TYPES_STR}' content type"
        )))
    }
}

fn shared_record_name(actual: &Body, expected: &Body) -> Result<String, PluginError> {
    let actual_name = record_name(actual, "Actual")?;
    let expected_name = record_name(expected, "Expected")?;
    if actual_name == expected_name {
        Ok(expected_name)
    } else {
        Err(message(format!(
            "Record names don't match, actual: '{actual_name}' expected: '{expected_name}'"
        )))
    }
}

fn add_rules(
    category: &mut MatchingRuleCategory,
    key: &str,
    rules: &MatchingRules,
) -> Result<(), PluginError> {
    let path =
        DocPath::new(key).map_err(|error| message(format!("Invalid path '{key}': {error}")))?;
    for rule in &rules.rule {
        category.add_rule(path.clone(), rule_from_proto(rule)?, RuleLogic::And);
    }
    Ok(())
}

fn matching_context(request: &CompareContentsRequest) -> Result<CoreMatchingContext, PluginError> {
    let mut category = MatchingRuleCategory::empty(MATCHING_RULE_CATEGORY_NAME);
    for (key, rules) in &request.rules {
        add_rules(&mut category, key, rules)?;
    }
    let config = if request.allow_unexpected_keys {
        DiffConfig::AllowUnexpectedKeys
    } else {
        DiffConfig::NoUnexpectedKeys
    };
    Ok(CoreMatchingContext::new(config, &category, &HashMap::new()))
}

fn content_mismatch(mismatch: Mismatch) -> ContentMismatch {
    ContentMismatch {
        expected: mismatch.expected.map(String::into_bytes),
        actual: mismatch.actual.map(String::into_bytes),
        mismatch: mismatch.message,
        path: mismatch.path,
        diff: String::new(),
        mismatch_type: String::new(),
    }
}

fn to_response(items: Vec<BodyItem>) -> CompareContentsResponse {
    let results = items.into_iter().fold(HashMap::new(), |mut acc, item| {
        let mismatches = item.mismatches.into_iter().map(content_mismatch);
        acc.entry(item.key)
            .or_insert_with(|| ContentMismatches {
                mismatches: Vec::new(),
            })
            .mismatches
            .extend(mismatches);
        acc
    });
    CompareContentsResponse {
        error: String::new(),
        type_mismatch: None,
        results,
    }
}

fn body<'r>(body: &'r Option<Body>, text: &str) -> Result<&'r Body, PluginError> {
    body.as_ref().ok_or_else(|| message(text))
}

pub fn build(
    request: &CompareContentsRequest,
    schema: &Schema,
) -> Result<CompareContentsResponse, PluginError> {
    let actual_body = body(&request.actual, "Actual body required")?;
    let expected_body = body(&request.expected, "Expected body required")?;
    let name = shared_record_name(actual_body, expected_body)?;
    let ctx = SchemaCtx::new(schema)?;
    let record = ctx.find_record(&name)?;
    let actual = decode(
        &ctx,
        record,
        actual_body.content.as_deref().unwrap_or_default(),
    )?;
    let expected = decode(
        &ctx,
        record,
        expected_body.content.as_deref().unwrap_or_default(),
    )?;
    let matching = matching_context(request)?;
    let items = Comparator::new(&ctx, &matching).compare_record(
        record,
        &DocPath::root(),
        &expected,
        &actual,
    );
    Ok(to_response(items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::codec::encode;
    use crate::avro::schema::parse_str;
    use crate::pact_plugin::MatchingRule as ProtoRule;
    use apache_avro::types::Value;

    const SCHEMA: &str = r#"{"type":"record","name":"Item","fields":[
        {"name":"id","type":"long"},{"name":"name","type":"string"}]}"#;

    fn body_of(schema: &Schema, id: i64, name: &str, content_type: &str) -> Body {
        let ctx = SchemaCtx::new(schema).unwrap();
        let value = Value::Record(vec![
            ("id".into(), Value::Long(id)),
            ("name".into(), Value::String(name.into())),
        ]);
        Body {
            content_type: content_type.to_string(),
            content: Some(encode(&ctx, schema, value).unwrap()),
            content_type_hint: 0,
        }
    }

    fn rules(pairs: &[(&str, &str)]) -> HashMap<String, MatchingRules> {
        pairs
            .iter()
            .map(|(path, kind)| {
                let rule = ProtoRule {
                    r#type: (*kind).to_string(),
                    values: None,
                };
                ((*path).to_string(), MatchingRules { rule: vec![rule] })
            })
            .collect()
    }

    fn request(actual: Option<Body>, expected: Option<Body>) -> CompareContentsRequest {
        CompareContentsRequest {
            expected,
            actual,
            allow_unexpected_keys: false,
            rules: rules(&[("$.id", "equality"), ("$.name", "equality")]),
            plugin_configuration: None,
        }
    }

    #[test]
    fn matching_contents_report_an_empty_result_per_field() {
        let schema = parse_str(SCHEMA).unwrap();
        let both = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let response = build(&request(Some(both.clone()), Some(both)), &schema).unwrap();
        assert_eq!(response.error, "");
        assert_eq!(response.results.len(), 2);
        assert!(response.results["$.id"].mismatches.is_empty());
        assert!(response.results["$.name"].mismatches.is_empty());
    }

    #[test]
    fn differing_contents_report_mismatches_on_the_differing_path() {
        let schema = parse_str(SCHEMA).unwrap();
        let expected = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let actual = body_of(&schema, 1, "b", "avro/binary;record=Item");
        let response = build(&request(Some(actual), Some(expected)), &schema).unwrap();
        assert!(response.results["$.id"].mismatches.is_empty());
        let mismatch = &response.results["$.name"].mismatches[0];
        assert_eq!(mismatch.path, "$.name");
        assert_eq!(mismatch.expected.as_deref(), Some(b"a".as_slice()));
        assert_eq!(mismatch.actual.as_deref(), Some(b"b".as_slice()));
    }

    #[test]
    fn missing_bodies_are_reported_in_order() {
        let schema = parse_str(SCHEMA).unwrap();
        let any = body_of(&schema, 1, "a", "avro/binary;record=Item");
        assert_eq!(
            build(&request(None, Some(any.clone())), &schema)
                .unwrap_err()
                .to_string(),
            "Actual body required"
        );
        assert_eq!(
            build(&request(Some(any), None), &schema)
                .unwrap_err()
                .to_string(),
            "Expected body required"
        );
    }

    #[test]
    fn content_types_must_follow_the_record_template() {
        let schema = parse_str(SCHEMA).unwrap();
        let good = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let bad = body_of(&schema, 1, "a", "avro/binary");
        let error = build(&request(Some(bad), Some(good)), &schema).unwrap_err();
        assert_eq!(error.to_string(), "Actual body content type didn't match expected template of 'content/type; record=NameOfRecord'");
    }

    #[test]
    fn non_avro_content_types_are_rejected() {
        let schema = parse_str(SCHEMA).unwrap();
        let good = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let other = body_of(&schema, 1, "a", "text/plain;record=Item");
        let error = build(&request(Some(good), Some(other)), &schema).unwrap_err();
        assert_eq!(error.to_string(), "Expected body is not one of 'application/avro;avro/bytes;avro/binary;application/*+avro' content type");
    }

    #[test]
    fn record_names_must_agree() {
        let schema = parse_str(SCHEMA).unwrap();
        let one = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let other = body_of(&schema, 1, "a", "avro/binary;record=Other");
        let error = build(&request(Some(one), Some(other)), &schema).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Record names don't match, actual: 'Item' expected: 'Other'"
        );
    }

    #[test]
    fn the_literal_wildcard_avro_content_type_is_accepted() {
        let schema = parse_str(SCHEMA).unwrap();
        let both = body_of(&schema, 1, "a", "application/*+avro;record=Item");
        assert!(build(&request(Some(both.clone()), Some(both)), &schema).is_ok());
    }

    #[test]
    fn undecodable_content_reports_the_fixed_message() {
        let schema = parse_str(SCHEMA).unwrap();
        let good = body_of(&schema, 1, "a", "avro/binary;record=Item");
        let broken = Body {
            content: Some(vec![0xff]),
            ..good.clone()
        };
        let error = build(&request(Some(broken), Some(good)), &schema).unwrap_err();
        assert_eq!(error.to_string(), "Failed to deserialize avro schema");
    }

    #[test]
    fn items_sharing_a_key_merge_their_mismatches_in_order() {
        let mismatch = |message: &str| Mismatch {
            expected: None,
            actual: None,
            message: message.to_string(),
            path: "$.ages".to_string(),
        };
        let item = |messages: &[&str]| BodyItem {
            key: "$.ages".to_string(),
            mismatches: messages.iter().map(|m| mismatch(m)).collect(),
        };
        let response = to_response(vec![item(&["first"]), item(&["second", "third"])]);
        assert_eq!(response.results.len(), 1);
        let merged: Vec<&str> = response.results["$.ages"]
            .mismatches
            .iter()
            .map(|m| m.mismatch.as_str())
            .collect();
        assert_eq!(merged, ["first", "second", "third"]);
    }
}
