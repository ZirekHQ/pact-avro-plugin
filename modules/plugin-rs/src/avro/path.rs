use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PactFieldPath(Vec<String>);

impl PactFieldPath {
    pub fn root() -> Self {
        Self(vec!["$".to_string()])
    }

    pub fn parse(dotted: &str) -> Self {
        Self(dotted.split('.').map(str::to_string).collect())
    }

    pub fn append(&self, segment: impl Display) -> Self {
        let mut segments = self.0.clone();
        segments.push(segment.to_string());
        Self(segments)
    }

    pub fn to_json_path(&self) -> String {
        self.0.join(".")
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
    fn append_joins_segments_with_dots_including_indexes() {
        let path = PactFieldPath::root().append("names").append(0);
        assert_eq!(path.to_json_path(), "$.names.0");
    }

    #[test]
    fn parse_splits_on_dots() {
        assert_eq!(
            PactFieldPath::parse("$.a.b"),
            PactFieldPath::root().append("a").append("b")
        );
    }

    #[test]
    fn append_does_not_mutate_the_original() {
        let root = PactFieldPath::root();
        let _child = root.append("x");
        assert_eq!(root.to_json_path(), "$");
    }
}
