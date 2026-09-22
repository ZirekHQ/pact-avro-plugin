use pact_models::path_exp::DocPath;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Segment {
    Field(String),
    Index(usize),
}

impl Segment {
    fn render(&self) -> String {
        match self {
            Segment::Index(index) => format!(".{index}"),
            Segment::Field(name) => DocPath::root()
                .join_field(name.as_str())
                .to_string()
                .trim_start_matches('$')
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PactFieldPath(Vec<Segment>);

impl PactFieldPath {
    pub fn root() -> Self {
        Self(vec![])
    }

    pub fn parse(dotted: &str) -> Self {
        Self(
            dotted
                .split('.')
                .filter(|part| *part != "$")
                .map(|part| {
                    part.parse()
                        .map(Segment::Index)
                        .unwrap_or_else(|_| Segment::Field(part.to_string()))
                })
                .collect(),
        )
    }

    pub fn field(&self, name: impl Into<String>) -> Self {
        self.push(Segment::Field(name.into()))
    }

    pub fn index(&self, index: usize) -> Self {
        self.push(Segment::Index(index))
    }

    fn push(&self, segment: Segment) -> Self {
        let mut segments = self.0.clone();
        segments.push(segment);
        Self(segments)
    }

    pub fn to_json_path(&self) -> String {
        self.0.iter().fold("$".to_string(), |mut path, segment| {
            path.push_str(&segment.render());
            path
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_renders_as_dollar() {
        assert_eq!(PactFieldPath::root().to_json_path(), "$");
    }

    #[test]
    fn fields_and_indexes_join_with_dots() {
        let path = PactFieldPath::root().field("names").index(0);
        assert_eq!(path.to_json_path(), "$.names.0");
    }

    #[test]
    fn keys_with_path_syntax_are_bracket_quoted() {
        let path = PactFieldPath::root().field("ages").field("a.b");
        assert_eq!(path.to_json_path(), "$.ages['a.b']");
    }

    #[test]
    fn quotes_and_backslashes_in_keys_are_escaped() {
        let path = PactFieldPath::root().field("it's\\");
        assert_eq!(path.to_json_path(), "$['it\\'s\\\\']");
    }

    #[test]
    fn numeric_map_keys_stay_distinct_from_indexes() {
        let path = PactFieldPath::root().field("ages").field("0");
        assert_eq!(path.to_json_path(), "$.ages['0']");
    }

    #[test]
    fn rendered_paths_parse_back_as_pact_paths() {
        let path = PactFieldPath::root()
            .field("ages")
            .field("a.b")
            .to_json_path();
        assert!(DocPath::new(path).is_ok());
    }

    #[test]
    fn parse_splits_on_dots() {
        assert_eq!(
            PactFieldPath::parse("$.a.b"),
            PactFieldPath::root().field("a").field("b")
        );
    }

    #[test]
    fn append_does_not_mutate_the_original() {
        let root = PactFieldPath::root();
        let _child = root.field("x");
        assert_eq!(root.to_json_path(), "$");
    }
}
