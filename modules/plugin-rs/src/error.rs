use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("{0}")]
    Message(String),

    /// Individual messages are logged by the caller; this fixed summary is
    /// what actually surfaces to the gRPC caller.
    #[error("Multiple errors detected and logged, please check logs")]
    Messages(Vec<String>),

    #[error("{0}")]
    Exception(String),
}

impl PluginError {
    pub fn field_unsupported_type(field_type: &str, field_name: &str, field_value: &str) -> Self {
        PluginError::Message(format!(
            "Type '{field_type}' is not supported for field: '{field_name}' with value: '{field_value}'"
        ))
    }

    pub fn field_not_nullable(field_name: &str, field_value: &str) -> Self {
        PluginError::Message(format!(
            "'UNION' type is only supported to make field nullable, field: '{field_name}' with value: '{field_value}'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_variant_displays_its_text_verbatim() {
        let err = PluginError::Message("Config item with key 'pact:avro' is required".to_string());
        assert_eq!(
            err.to_string(),
            "Config item with key 'pact:avro' is required"
        );
    }

    #[test]
    fn messages_variant_joins_with_multiple_errors_note() {
        let err = PluginError::Messages(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            err.to_string(),
            "Multiple errors detected and logged, please check logs"
        );
    }
}
