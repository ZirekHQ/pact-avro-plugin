use crate::error::PluginError;
use crate::pact_plugin;
use crate::proto_json::{json_object_to_struct, struct_to_json};
use pact_models::matchingrules::expressions::parse_matcher_def;
use pact_models::matchingrules::MatchingRule;
use serde_json::{Map, Value as Json};

#[derive(Debug, Clone, PartialEq)]
pub struct FieldRule {
    pub value: String,
    pub rules: Vec<MatchingRule>,
}

pub fn parse_rules(input: &str) -> Result<FieldRule, PluginError> {
    let definition = parse_matcher_def(input).map_err(|error| {
        PluginError::Message(format!(
            "'{input}' is not a valid matching rule definition - {error}"
        ))
    })?;
    let mut rules = vec![];
    let mut errors = vec![];
    for item in definition.rules {
        item.either(
            |rule| rules.push(rule),
            |reference| errors.push(format!("Rule '{reference:?}' not supported for now")),
        );
    }
    if errors.is_empty() {
        Ok(FieldRule {
            value: definition.value,
            rules,
        })
    } else {
        Err(PluginError::Messages(errors))
    }
}

fn attributes(rule: &MatchingRule) -> Map<String, Json> {
    match rule.to_json() {
        Json::Object(mut entries) => {
            entries.remove("match");
            entries
        }
        _ => Map::new(),
    }
}

pub fn rule_to_proto(rule: &MatchingRule) -> pact_plugin::MatchingRule {
    pact_plugin::MatchingRule {
        r#type: rule.name(),
        values: Some(json_object_to_struct(&attributes(rule))),
    }
}

pub fn rule_from_proto(rule: &pact_plugin::MatchingRule) -> Result<MatchingRule, PluginError> {
    let values = rule
        .values
        .as_ref()
        .map_or_else(|| Json::Object(Map::new()), struct_to_json);
    MatchingRule::create(&rule.r#type, &values).map_err(|error| {
        PluginError::Message(format!("Invalid matching rule '{}': {error}", rule.r#type))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_matcher_expression_into_value_and_rules() {
        let parsed = parse_rules("matching(equalTo, 'hello')").unwrap();
        assert_eq!(parsed.value, "hello");
        assert_eq!(parsed.rules, vec![MatchingRule::Equality]);
    }

    #[test]
    fn parses_not_empty() {
        let parsed = parse_rules("notEmpty('100')").unwrap();
        assert_eq!(
            (parsed.value.as_str(), parsed.rules),
            ("100", vec![MatchingRule::NotEmpty])
        );
    }

    #[test]
    fn plain_text_is_not_a_valid_definition() {
        let error = parse_rules("hello").unwrap_err().to_string();
        assert!(error.starts_with("'hello' is not a valid matching rule definition - "));
    }

    #[test]
    fn regex_rule_round_trips_through_the_proto_form() {
        let rule = MatchingRule::Regex("\\d+".to_string());
        let proto = rule_to_proto(&rule);
        assert_eq!(proto.r#type, "regex");
        assert_eq!(rule_from_proto(&proto).unwrap(), rule);
    }

    #[test]
    fn attribute_less_rules_have_an_empty_values_struct() {
        let proto = rule_to_proto(&MatchingRule::Equality);
        assert_eq!(proto.r#type, "equality");
        assert!(proto.values.unwrap().fields.is_empty());
    }

    #[test]
    fn a_type_name_alone_is_enough_to_rebuild_a_rule() {
        let proto = pact_plugin::MatchingRule {
            r#type: "type".to_string(),
            values: None,
        };
        assert_eq!(rule_from_proto(&proto).unwrap(), MatchingRule::Type);
    }
}
