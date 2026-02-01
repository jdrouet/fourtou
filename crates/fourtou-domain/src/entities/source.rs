/// Unique identifier for a data source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(String);

impl SourceId {
    /// Creates a new `SourceId` from a string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the source ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for SourceId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SourceId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_id_string_when_created_with_new() {
        let id = SourceId::new("my-source");
        assert_eq!(id.as_str(), "my-source");
    }

    #[test]
    fn should_format_as_inner_string_when_displayed() {
        let id = SourceId::new("test-source");
        assert_eq!(format!("{id}"), "test-source");
    }

    #[test]
    fn should_create_source_id_when_converted_from_str() {
        let id: SourceId = "from-str".into();
        assert_eq!(id.as_str(), "from-str");
    }

    #[test]
    fn should_create_source_id_when_converted_from_string() {
        let id: SourceId = String::from("from-string").into();
        assert_eq!(id.as_str(), "from-string");
    }

    #[test]
    fn should_be_equal_when_same_inner_value() {
        let id1 = SourceId::new("same");
        let id2 = SourceId::new("same");
        let id3 = SourceId::new("different");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn should_deduplicate_when_inserted_in_hashset() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(SourceId::new("one"));
        set.insert(SourceId::new("two"));
        set.insert(SourceId::new("one")); // duplicate

        assert_eq!(set.len(), 2);
    }
}
