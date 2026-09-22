use prost_types::{value::Kind, ListValue, Struct, Value};
use serde_json::{Map, Number, Value as Json};

pub fn text_value(text: &str) -> Value {
    Value {
        kind: Some(Kind::StringValue(text.to_string())),
    }
}

pub fn json_to_value(json: &Json) -> Value {
    let kind = match json {
        Json::Null => Kind::NullValue(0),
        Json::Bool(flag) => Kind::BoolValue(*flag),
        Json::Number(number) => Kind::NumberValue(number.as_f64().unwrap_or_default()),
        Json::String(text) => Kind::StringValue(text.clone()),
        Json::Array(items) => Kind::ListValue(ListValue {
            values: items.iter().map(json_to_value).collect(),
        }),
        Json::Object(entries) => Kind::StructValue(json_object_to_struct(entries)),
    };
    Value { kind: Some(kind) }
}

pub fn json_object_to_struct(entries: &Map<String, Json>) -> Struct {
    Struct {
        fields: entries
            .iter()
            .map(|(key, value)| (key.clone(), json_to_value(value)))
            .collect(),
    }
}

pub fn value_to_json(value: &Value) -> Json {
    match &value.kind {
        None | Some(Kind::NullValue(_)) => Json::Null,
        Some(Kind::BoolValue(flag)) => Json::Bool(*flag),
        Some(Kind::NumberValue(number)) => number_to_json(*number),
        Some(Kind::StringValue(text)) => Json::String(text.clone()),
        Some(Kind::ListValue(list)) => Json::Array(list.values.iter().map(value_to_json).collect()),
        Some(Kind::StructValue(inner)) => struct_to_json(inner),
    }
}

pub fn struct_to_json(source: &Struct) -> Json {
    Json::Object(
        source
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), value_to_json(value)))
            .collect(),
    )
}

fn number_to_json(number: f64) -> Json {
    if number.fract() == 0.0 && number.abs() < 9.0e15 {
        Json::Number(Number::from(number as i64))
    } else {
        Number::from_f64(number).map_or(Json::Null, Json::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_round_trips_through_a_struct() {
        let source = json!({"a": "x", "b": [true, null], "c": {"d": 1.5}});
        let restored = value_to_json(&json_to_value(&source));
        assert_eq!(restored, source);
    }

    #[test]
    fn whole_numbers_come_back_as_integers_so_pact_rules_can_read_them() {
        let restored = value_to_json(&json_to_value(&json!({"min": 2})));
        assert_eq!(restored, json!({"min": 2}));
        assert!(restored["min"].as_u64().is_some());
    }

    #[test]
    fn text_value_wraps_a_string() {
        assert_eq!(value_to_json(&text_value("hi")), json!("hi"));
    }
}
