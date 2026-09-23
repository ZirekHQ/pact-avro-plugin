use crate::avro::codec::{decode, encode};
use crate::avro::json::{coerce, to_json};
use crate::avro::rules::generator_from_proto;
use crate::avro::schema::SchemaCtx;
use crate::compare::record_name;
use crate::error::PluginError;
use crate::pact_plugin::{
    generate_content_request, Body, GenerateContentRequest, GenerateContentResponse,
};
use crate::proto_json::struct_to_json;
use apache_avro::schema::Schema;
use pact_models::generators::{
    ContentTypeHandler, Generator, GeneratorTestMode, JsonHandler, NoopVariantMatcher,
};
use pact_models::path_exp::DocPath;
use std::collections::HashMap;

/// `PactFieldPath` renders an array index the same way as a field name
/// (`.0`), so `DocPath::new` parses it as a field lookup that never
/// resolves against a JSON array. A field named "0" is never rendered this
/// way — `PactFieldPath` bracket-quotes numeric-looking field names
/// (`['0']`) specifically to keep them distinct — so every unquoted
/// `.<digits>` segment is unambiguously an array index. Bracket-quoted
/// segments (e.g. `['a.0']`) are copied verbatim: a dotted digit run inside
/// them is part of the field name, not an index.
fn bracket_indexes(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let mut result = String::with_capacity(path.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && chars.get(i + 1) == Some(&'\'') {
            let start = i;
            i += 2;
            while i < chars.len() {
                let is_escape = chars[i] == '\\';
                let is_quote = chars[i] == '\'';
                i += if is_escape { 2 } else { 1 };
                if is_quote {
                    break;
                }
            }
            if chars.get(i) == Some(&']') {
                i += 1;
            }
            result.extend(&chars[start..i.min(chars.len())]);
        } else if chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
            let start = i + 1;
            let mut end = start;
            while chars.get(end).is_some_and(char::is_ascii_digit) {
                end += 1;
            }
            result.push('[');
            result.extend(&chars[start..end]);
            result.push(']');
            i = end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn generator_map(
    proto: &HashMap<String, crate::pact_plugin::Generator>,
) -> HashMap<DocPath, Generator> {
    proto
        .iter()
        .filter_map(|(path, proto_generator)| {
            let doc_path = match DocPath::new(bracket_indexes(path)) {
                Ok(doc_path) => doc_path,
                Err(error) => {
                    tracing::warn!(
                        "Generator path '{path}' could not be parsed ({error}); \
                         leaving the field unchanged"
                    );
                    return None;
                }
            };
            let generator = generator_from_proto(proto_generator);
            match generator {
                Generator::MockServerURL(..)
                | Generator::ArrayContains(..)
                | Generator::Plugin { .. } => {
                    tracing::warn!(
                        "Generator '{}' at path '{path}' is not supported by this plugin; \
                         leaving the field unchanged",
                        generator.name()
                    );
                    None
                }
                _ => Some((doc_path, generator)),
            }
        })
        .collect()
}

fn test_mode(request: &GenerateContentRequest) -> GeneratorTestMode {
    if request.test_mode == generate_content_request::TestMode::Consumer as i32 {
        GeneratorTestMode::Consumer
    } else {
        GeneratorTestMode::Provider
    }
}

/// Flattens `test_context`'s own top-level fields into the context map,
/// rather than nesting the whole thing under a new key: pact core's
/// verification path already sends `test_context` with its own
/// `providerState` key alongside other data (port, etc.), while other
/// callers send the provider state values directly at the top level.
/// `pact_models`'s own `ProviderStateGenerator` handles both shapes: it
/// reads `context["providerState"]` when present, and falls back to
/// `context` itself otherwise.
fn context(test_context: Option<&serde_json::Value>) -> HashMap<&str, serde_json::Value> {
    match test_context {
        Some(serde_json::Value::Object(map)) => {
            map.iter().map(|(k, v)| (k.as_str(), v.clone())).collect()
        }
        _ => HashMap::new(),
    }
}

pub fn build(
    request: &GenerateContentRequest,
    schema: &Schema,
) -> Result<GenerateContentResponse, PluginError> {
    let body = request
        .contents
        .as_ref()
        .ok_or_else(|| PluginError::Message("Contents required".to_string()))?;
    let name = record_name(body, "Contents")?;
    let ctx = SchemaCtx::new(schema)?;
    let record = ctx.find_record(&name)?;
    let decoded = decode(&ctx, record, body.content.as_deref().unwrap_or_default())?;

    let generators = generator_map(&request.generators);
    let mode = test_mode(request);
    let test_context_json = request.test_context.as_ref().map(struct_to_json);
    let context = context(test_context_json.as_ref());
    let mut handler = JsonHandler {
        value: to_json(&decoded),
    };
    handler
        .process_body(
            &generators,
            &mode,
            &context,
            &(Box::new(NoopVariantMatcher)
                as Box<dyn pact_models::generators::VariantMatcher + Send + Sync>),
        )
        .map_err(PluginError::Message)?;

    let generated = coerce(&ctx, record, &handler.value)?;
    let bytes = encode(&ctx, record, generated)?;

    Ok(GenerateContentResponse {
        contents: Some(Body {
            content_type: body.content_type.clone(),
            content: Some(bytes),
            content_type_hint: body.content_type_hint,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avro::schema::parse_str;

    const SCHEMA: &str = r#"{"type":"record","name":"Item","fields":[
        {"name":"id","type":"long"},{"name":"name","type":"string"}]}"#;

    fn body_of(schema: &Schema, id: i64, name: &str) -> Body {
        let ctx = SchemaCtx::new(schema).unwrap();
        let value = apache_avro::types::Value::Record(vec![
            ("id".into(), apache_avro::types::Value::Long(id)),
            (
                "name".into(),
                apache_avro::types::Value::String(name.into()),
            ),
        ]);
        Body {
            content_type: "avro/binary;record=Item".to_string(),
            content: Some(encode(&ctx, schema, value).unwrap()),
            content_type_hint: 0,
        }
    }

    #[test]
    fn random_int_generates_a_value_in_range() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "RandomInt".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"min": 100, "max": 200})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        let generated_id = decoded_id(&schema, response.contents.as_ref().unwrap());
        assert!((100..=200).contains(&generated_id), "{generated_id}");
    }

    fn decoded_id(schema: &Schema, body: &Body) -> i64 {
        let ctx = SchemaCtx::new(schema).unwrap();
        let apache_avro::types::Value::Record(fields) =
            decode(&ctx, schema, body.content.as_deref().unwrap()).unwrap()
        else {
            panic!("record expected")
        };
        let apache_avro::types::Value::Long(id) = fields[0].1 else {
            panic!("long expected")
        };
        id
    }

    fn generator(r#type: &str) -> crate::pact_plugin::Generator {
        crate::pact_plugin::Generator {
            r#type: r#type.to_string(),
            values: None,
        }
    }

    #[test]
    fn bracket_indexes_converts_unquoted_numeric_segments() {
        assert_eq!(bracket_indexes("$.names.0"), "$.names[0]");
        assert_eq!(bracket_indexes("$.names.12"), "$.names[12]");
    }

    #[test]
    fn bracket_indexes_preserves_dots_inside_quoted_map_keys() {
        assert_eq!(bracket_indexes("$.ages['a.0']"), "$.ages['a.0']");
        assert_eq!(bracket_indexes("$.ages['a.0'].1"), "$.ages['a.0'][1]");
    }

    #[derive(Clone, Default)]
    struct SharedBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Runs `body` under a tracing subscriber that writes every event into
    /// an in-memory buffer, returning the buffer's contents as text.
    fn captured_tracing_output(body: impl FnOnce()) -> String {
        let buffer = SharedBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, body);
        let bytes = buffer.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn an_unparseable_generator_path_is_logged_and_skipped() {
        let mut proto = HashMap::new();
        proto.insert("$.a[".to_string(), generator("RandomInt"));

        let output = captured_tracing_output(|| {
            let map = generator_map(&proto);
            assert!(map.is_empty());
        });

        assert!(output.contains("$.a["), "{output}");
    }

    /// Shaped like pact core's own verification path sends it: the
    /// provider state nested under its own `providerState` key, alongside
    /// other top-level context data (a mock server port, etc.) rather than
    /// being the provider state map itself.
    fn pact_core_shaped_test_context() -> prost_types::Struct {
        crate::proto_json::json_object_to_struct(
            serde_json::json!({"providerState": {"orderId": 999}, "port": 8080})
                .as_object()
                .unwrap(),
        )
    }

    #[test]
    fn uuid_generates_a_value_matching_its_format() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert("$.name".to_string(), generator("Uuid"));
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let apache_avro::types::Value::Record(fields) = decode(
            &ctx,
            &schema,
            response.contents.unwrap().content.as_deref().unwrap(),
        )
        .unwrap() else {
            panic!("record expected")
        };
        let apache_avro::types::Value::String(name) = &fields[1].1 else {
            panic!("string expected")
        };
        assert!(uuid::Uuid::parse_str(name).is_ok(), "{name}");
    }

    #[test]
    fn provider_state_generator_reads_the_value_from_test_context() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "ProviderState".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"expression": "orderId"})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let test_context = crate::proto_json::json_object_to_struct(
            serde_json::json!({"orderId": 999}).as_object().unwrap(),
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            test_context: Some(test_context),
            test_mode: generate_content_request::TestMode::Provider as i32,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        assert_eq!(
            decoded_id(&schema, response.contents.as_ref().unwrap()),
            999
        );
    }

    #[test]
    fn provider_state_generator_reads_the_value_from_a_pre_nested_test_context() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "ProviderState".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"expression": "orderId"})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let test_context = pact_core_shaped_test_context();
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            test_context: Some(test_context),
            test_mode: generate_content_request::TestMode::Provider as i32,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        assert_eq!(
            decoded_id(&schema, response.contents.as_ref().unwrap()),
            999
        );
    }

    #[test]
    fn a_provider_state_generator_with_no_context_leaves_the_field_unchanged() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "ProviderState".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"expression": "orderId"})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            test_mode: generate_content_request::TestMode::Provider as i32,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        assert_eq!(decoded_id(&schema, response.contents.as_ref().unwrap()), 1);
    }

    #[test]
    fn a_content_type_missing_the_record_suffix_is_a_clear_error() {
        let schema = parse_str(SCHEMA).unwrap();
        let mut body = body_of(&schema, 1, "a");
        body.content_type = "avro/binary".to_string();
        let request = GenerateContentRequest {
            contents: Some(body),
            generators: HashMap::new(),
            plugin_configuration: None,
            ..Default::default()
        };
        assert_eq!(
            build(&request, &schema).unwrap_err().to_string(),
            "Contents body content type didn't match expected template of \
             'content/type; record=NameOfRecord'"
        );
    }

    #[test]
    fn an_out_of_scope_generator_is_skipped_and_other_fields_still_generate() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert("$.name".to_string(), generator("MockServerURL"));
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "RandomInt".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"min": 500, "max": 500})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let apache_avro::types::Value::Record(fields) = decode(
            &ctx,
            &schema,
            response
                .contents
                .as_ref()
                .unwrap()
                .content
                .as_deref()
                .unwrap(),
        )
        .unwrap() else {
            panic!("record expected")
        };
        assert_eq!(
            fields[1].1,
            apache_avro::types::Value::String("a".to_string())
        );
        assert_eq!(
            decoded_id(&schema, response.contents.as_ref().unwrap()),
            500
        );
    }

    #[test]
    fn a_generator_path_missing_from_the_body_does_not_fail_the_call() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.missing".to_string(),
            crate::pact_plugin::Generator {
                r#type: "RandomInt".to_string(),
                values: None,
            },
        );
        generators.insert(
            "$.id".to_string(),
            crate::pact_plugin::Generator {
                r#type: "RandomInt".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"min": 500, "max": 500})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        assert_eq!(
            decoded_id(&schema, response.contents.as_ref().unwrap()),
            500,
            "the generator on the existing '$.id' path should still have applied"
        );
    }

    #[test]
    fn missing_contents_is_a_clear_error() {
        let schema = parse_str(SCHEMA).unwrap();
        let request = GenerateContentRequest {
            contents: None,
            generators: HashMap::new(),
            plugin_configuration: None,
            ..Default::default()
        };
        assert_eq!(
            build(&request, &schema).unwrap_err().to_string(),
            "Contents required"
        );
    }

    #[test]
    fn a_generator_on_an_array_element_applies() {
        let schema = parse_str(
            r#"{"type":"record","name":"Ids","fields":[
                {"name":"ids","type":{"type":"array","items":"long"}}]}"#,
        )
        .unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let value = apache_avro::types::Value::Record(vec![(
            "ids".into(),
            apache_avro::types::Value::Array(vec![
                apache_avro::types::Value::Long(1),
                apache_avro::types::Value::Long(2),
            ]),
        )]);
        let body = Body {
            content_type: "avro/binary;record=Ids".to_string(),
            content: Some(encode(&ctx, &schema, value).unwrap()),
            content_type_hint: 0,
        };
        let mut generators = HashMap::new();
        generators.insert(
            "$.ids.0".to_string(),
            crate::pact_plugin::Generator {
                r#type: "RandomInt".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"min": 77, "max": 77})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        let apache_avro::types::Value::Record(fields) = decode(
            &ctx,
            &schema,
            response
                .contents
                .as_ref()
                .unwrap()
                .content
                .as_deref()
                .unwrap(),
        )
        .unwrap() else {
            panic!("record expected")
        };
        let apache_avro::types::Value::Array(ids) = &fields[0].1 else {
            panic!("array expected")
        };
        assert_eq!(
            ids,
            &vec![
                apache_avro::types::Value::Long(77),
                apache_avro::types::Value::Long(2)
            ]
        );
    }

    /// `avro::schema` strips every `logicalType` before parsing (see its
    /// own tests), so a date/timestamp field is always a plain `int`/`long`
    /// by the time this module sees it — never the string these generators
    /// produce. They only work against a `string`-typed field (e.g. one a
    /// consumer configured to hold an ISO date as free text). Generating a
    /// date/time value onto a schema's `int`/`long` field errors cleanly
    /// via `coerce`'s type mismatch, the same as any other type mismatch.
    #[test]
    fn date_generates_a_value_into_a_string_typed_field() {
        let schema = parse_str(SCHEMA).unwrap();
        let body = body_of(&schema, 1, "a");
        let mut generators = HashMap::new();
        generators.insert(
            "$.name".to_string(),
            crate::pact_plugin::Generator {
                r#type: "Date".to_string(),
                values: Some(crate::proto_json::json_object_to_struct(
                    serde_json::json!({"format": "yyyy-MM-dd"})
                        .as_object()
                        .unwrap(),
                )),
            },
        );
        let request = GenerateContentRequest {
            contents: Some(body),
            generators,
            plugin_configuration: None,
            ..Default::default()
        };
        let response = build(&request, &schema).unwrap();
        let ctx = SchemaCtx::new(&schema).unwrap();
        let apache_avro::types::Value::Record(fields) = decode(
            &ctx,
            &schema,
            response.contents.unwrap().content.as_deref().unwrap(),
        )
        .unwrap() else {
            panic!("record expected")
        };
        let apache_avro::types::Value::String(name) = &fields[1].1 else {
            panic!("string expected")
        };
        assert_eq!(name.len(), "yyyy-MM-dd".len(), "{name}");
    }
}
