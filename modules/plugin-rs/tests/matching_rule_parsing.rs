use pact_matching::matchingrules::match_values;
use pact_matching::{CoreMatchingContext, DiffConfig, MatchingContext};
use pact_models::matchingrules::expressions::parse_matcher_def;
use pact_models::matchingrules::{Category, MatchingRule, MatchingRuleCategory, RuleLogic};
use pact_models::path_exp::DocPath;
use std::collections::HashMap;

#[test]
fn parses_a_type_matching_rule_expression() {
    let parsed = parse_matcher_def("matching(type,'Name')")
        .expect("a valid matching rule expression must parse");

    assert_eq!(parsed.value, "Name");
    assert!(
        !parsed.rules.is_empty(),
        "expected at least one parsed matching rule, got none"
    );
}

#[test]
fn rejects_an_invalid_matching_rule_expression() {
    let result = parse_matcher_def("not a valid expression(");
    assert!(result.is_err(), "malformed expressions must be rejected");
}

#[test]
fn builds_a_doc_path_matching_pact_expression_syntax() {
    let path = DocPath::root().join("foo").join("bar");
    assert_eq!(path.to_string(), "$.foo.bar");
}

#[test]
fn equality_matching_rule_flags_a_mismatch() {
    let path = DocPath::root().join("name");
    let mut matchers = MatchingRuleCategory::empty(Category::BODY);
    matchers.add_rule(path.clone(), MatchingRule::Equality, RuleLogic::And);

    let context =
        CoreMatchingContext::new(DiffConfig::NoUnexpectedKeys, &matchers, &HashMap::new());
    let rules = context.select_best_matcher(&path);
    assert!(
        !rules.is_empty(),
        "expected the context to resolve an Equality rule for '{path}'"
    );

    let result = match_values(&path, &rules, "expected", "actual");
    assert!(
        result.is_err(),
        "'expected' vs 'actual' under Equality must mismatch"
    );
}
