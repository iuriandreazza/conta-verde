use crate::DomainError;

pub const MAX_TAGS: usize = 10;
const MAX_TAG_CHARS: usize = 30;

/// A free label on a party, like `vip` or `atacado`. Trimmed and in lower case, so `VIP` and `vip`
/// are the same tag and a filter finds either; a comma is refused because the audit trail writes
/// the tags of a party as one comma-separated line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tag(String);

impl Tag {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let tag = raw.trim().to_lowercase();
        let chars = tag.chars().count();
        if chars == 0
            || chars > MAX_TAG_CHARS
            || tag.contains(',')
            || tag.chars().any(char::is_control)
        {
            return Err(DomainError::InvalidTag);
        }
        Ok(Self(tag))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The tags of a party: each one checked, repeated ones kept once, in alphabetical order, so the
/// same tags always read the same however they were typed.
pub(crate) fn clean_tags(raw: &[String]) -> Result<Vec<Tag>, DomainError> {
    let mut tags = raw
        .iter()
        .map(|tag| Tag::parse(tag))
        .collect::<Result<Vec<_>, _>>()?;
    tags.sort();
    tags.dedup();
    if tags.len() > MAX_TAGS {
        return Err(DomainError::TooManyTags);
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_lowers_the_case() {
        assert_eq!(Tag::parse("  VIP ").unwrap().as_str(), "vip");
        assert_eq!(
            Tag::parse("Distribuição ÁGUA").unwrap().as_str(),
            "distribuição água"
        );
    }

    #[test]
    fn refuses_what_a_label_cannot_be() {
        for raw in ["", "   ", "a,b", "a\nb", "a\0b", &"a".repeat(31)] {
            assert_eq!(Tag::parse(raw), Err(DomainError::InvalidTag), "{raw:?}");
        }
        assert!(Tag::parse(&"a".repeat(30)).is_ok());
    }

    #[test]
    fn a_party_keeps_each_tag_once_in_alphabetical_order() {
        let tags = clean_tags(&["Vip".into(), "atacado".into(), "VIP".into()]).unwrap();
        let names: Vec<&str> = tags.iter().map(Tag::as_str).collect();
        assert_eq!(names, ["atacado", "vip"]);
    }

    #[test]
    fn a_party_has_at_most_ten_tags_counting_each_once() {
        let many = |count: usize| (0..count).map(|n| format!("t{n}")).collect::<Vec<_>>();
        assert!(clean_tags(&many(10)).is_ok());
        assert_eq!(clean_tags(&many(11)), Err(DomainError::TooManyTags));
        let mut repeated = many(10);
        repeated.push("T0".into());
        assert_eq!(clean_tags(&repeated).unwrap().len(), 10);
    }
}
