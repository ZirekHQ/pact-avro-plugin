//! Spike for the Rust migration's Plan 2: proves pact_models/pact_matching
//! work as external dependencies and records the real API surface.
//!
//! Findings:
//!
//! Crate versions actually resolved and verified against (by reading the
//! pinned source under `~/.cargo/registry/src/...`, not docs.rs alone):
//! `pact_models = 1.3.14`, `pact_matching = 2.0.11` (pinned via `"1.3"` /
//! `"2.0"` in Cargo.toml).
//!
//! What differed from the plan's draft, and why:
//!
//! 1. `parse_matcher_def` and `DocPath::root().join(..)` compiled exactly as
//!    drafted, with imports exactly as drafted
//!    (`pact_models::matchingrules::expressions::parse_matcher_def`,
//!    `pact_models::matchingrules::MatchingRule`, `pact_models::path_exp::DocPath`).
//!    `MatchingRuleDefinition.rules` is actually `Vec<Either<MatchingRule,
//!    MatchingReference>>` (from the `either` crate), not `Vec<MatchingRule>`
//!    as the draft's comment implied, but that's invisible to the draft's
//!    `.is_empty()` assertion so it still compiled unmodified.
//!
//! 2. `match_values` and `MatchingContext` needed real correction, on three
//!    points:
//!    - `match_values` is NOT re-exported at the `pact_matching` crate root;
//!      it lives at `pact_matching::matchingrules::match_values` (the crate's
//!      top-level `use` of it in `lib.rs` is a private `use`, not `pub use`).
//!    - `match_values`'s second parameter is `&RuleList` (a struct holding
//!      `Vec<MatchingRule>` plus AND/OR logic), not `&MatchingRule` as
//!      drafted. Pass `&RuleList::new(MatchingRule::Equality)` for a single
//!      rule.
//!    - `pact_matching::MatchingContext` is confirmed to be a **trait**
//!      (`pub trait MatchingContext: Debug`), exactly as the brief
//!      speculated it might need to be — not a concrete type you construct
//!      directly. The concrete implementation is `CoreMatchingContext`,
//!      built from a `MatchingRuleCategory` (a path -> `RuleList` map) via
//!      `CoreMatchingContext::new(DiffConfig, &MatchingRuleCategory,
//!      &HashMap<String, PluginInteractionConfig>)` — the third argument is
//!      required because `pact_matching`'s default features include
//!      `"plugins"`, which this crate has not opted out of. The realistic
//!      call shape is: build a `MatchingRuleCategory` keyed by `DocPath`,
//!      wrap it in a `CoreMatchingContext`, call
//!      `context.select_best_matcher(&path)` (a `MatchingContext` trait
//!      method) to get the `RuleList` for that path, then pass that
//!      `RuleList` into `match_values`. This mirrors pact-jvm's
//!      `MatchingContext` + per-path rule lookup shape reasonably closely.
//!
//! Net assessment for Plan 2: both crates work as external dependencies and
//! cover matching-rule-DSL parsing, per-value matching, and path-expression
//! construction — the three capabilities the Scala plugin leans on
//! pact-jvm-core for. Nothing was found to be unusable; the deltas above are
//! naming/shape corrections, not missing capability.

use pact_models::matchingrules::expressions::parse_matcher_def;
use pact_models::matchingrules::MatchingRule;
use pact_models::path_exp::DocPath;

#[test]
fn parses_a_type_matching_rule_expression() {
    // Mirrors what RuleParser.scala does today via pact-jvm-core.
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
    // Mirrors PathExpressionImplicits.constructPath (List[String] => "$.foo.bar").
    let path = DocPath::root().join("foo").join("bar");
    assert_eq!(path.to_string(), "$.foo.bar");
}

#[test]
fn equality_matching_rule_flags_a_mismatch() {
    use pact_matching::matchingrules::match_values;
    use pact_matching::{CoreMatchingContext, DiffConfig, MatchingContext};
    use pact_models::matchingrules::{Category, MatchingRuleCategory, RuleLogic};
    use std::collections::HashMap;

    // Build a per-path matching-rule category the way a real interaction's
    // body matching rules would be structured (path -> RuleList), then wrap
    // it in a MatchingContext, exactly like pact-jvm's MatchingContext is
    // used to look up the applicable rule for a given path before matching.
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
