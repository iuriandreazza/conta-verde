use crate::name::required_text;
use crate::{AttachmentId, CompanyId, DomainError, EntryId};

/// The largest file an entry takes, in bytes ([ADR 0036]).
pub const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;
/// How many files an entry holds at once; the ones removed do not count.
pub const MAX_ENTRY_ATTACHMENTS: usize = 20;
const MAX_FILE_NAME_CHARS: usize = 200;

/// What a person attaches to an entry: the invoice, the receipt, the contract. Nothing a browser would
/// run when opened (HTML, SVG, scripts) is on the list, so a download is never a page of the app's origin.
const ALLOWED_TYPES: [&str; 8] = [
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/webp",
    "text/plain",
    "text/csv",
    "application/xml",
    "text/xml",
];

/// Unvalidated description of a file; the bytes themselves belong to the blob store.
#[derive(Debug, Clone)]
pub struct AttachmentDraft {
    pub file_name: String,
    pub content_type: String,
    pub size: u64,
}

/// A file attached to a financial entry ([ADR 0036]). Its bytes live in the blob store under its id; this
/// is the record that says whose it is and what it is. Never edited: removing it marks it, drops the
/// bytes, and the trail keeps who and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    id: AttachmentId,
    company_id: CompanyId,
    entry_id: EntryId,
    file_name: String,
    content_type: String,
    size: u64,
    removed: bool,
}

impl Attachment {
    /// A new attachment, not removed. Whether the entry is the company's is the use case's to check.
    pub fn new(
        id: AttachmentId,
        company_id: CompanyId,
        entry_id: EntryId,
        draft: AttachmentDraft,
    ) -> Result<Self, DomainError> {
        let file_name = required_text(
            &draft.file_name,
            MAX_FILE_NAME_CHARS,
            DomainError::InvalidAttachmentName,
        )?;
        // The name is shown and offered back on download, so it never carries a path or a line break.
        if file_name == "."
            || file_name == ".."
            || file_name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | '\u{2028}' | '\u{2029}'))
        {
            return Err(DomainError::InvalidAttachmentName);
        }
        // The type is what the browser said; it is trusted no further than naming what is served back
        // (always as a download, never sniffed), so only the list above is taken.
        let content_type = draft.content_type.trim().to_ascii_lowercase();
        if !ALLOWED_TYPES.contains(&content_type.as_str()) {
            return Err(DomainError::UnsupportedAttachmentType);
        }
        if draft.size == 0 || draft.size > MAX_ATTACHMENT_BYTES {
            return Err(DomainError::InvalidAttachmentSize);
        }
        Ok(Self {
            id,
            company_id,
            entry_id,
            file_name,
            content_type,
            size: draft.size,
            removed: false,
        })
    }

    /// The same attachment, removed or not: what storage hands back, and what a removal makes.
    pub fn with_removed(mut self, removed: bool) -> Self {
        self.removed = removed;
        self
    }

    pub fn id(&self) -> AttachmentId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn entry_id(&self) -> EntryId {
        self.entry_id
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn is_removed(&self) -> bool {
        self.removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(file_name: &str, content_type: &str, size: u64) -> AttachmentDraft {
        AttachmentDraft {
            file_name: file_name.into(),
            content_type: content_type.into(),
            size,
        }
    }

    fn build(draft: AttachmentDraft) -> Result<Attachment, DomainError> {
        Attachment::new(
            AttachmentId::generate(),
            CompanyId::generate(),
            EntryId::generate(),
            draft,
        )
    }

    #[test]
    fn takes_a_named_file_of_a_listed_type_up_to_the_limit() {
        let attachment = build(draft("  Nota 123.pdf ", " Application/PDF ", 2048)).unwrap();
        assert_eq!(attachment.file_name(), "Nota 123.pdf");
        assert_eq!(attachment.content_type(), "application/pdf");
        assert_eq!(attachment.size(), 2048);
        assert!(!attachment.is_removed());
        assert!(build(draft("a.png", "image/png", MAX_ATTACHMENT_BYTES)).is_ok());
        assert!(build(draft("a.png", "image/png", 1)).is_ok());
    }

    #[test]
    fn refuses_a_name_that_is_a_path_or_not_one_line() {
        for name in [
            "",
            "   ",
            ".",
            "..",
            "a/b.pdf",
            "a\\b.pdf",
            "a\nb.pdf",
            "a\0b.pdf",
            "a\u{2028}b.pdf",
            &"x".repeat(201),
        ] {
            assert_eq!(
                build(draft(name, "application/pdf", 1)).unwrap_err(),
                DomainError::InvalidAttachmentName,
                "{name:?}"
            );
        }
    }

    #[test]
    fn refuses_what_a_browser_would_run_and_what_is_empty_or_too_big() {
        for kind in ["text/html", "image/svg+xml", "application/javascript", ""] {
            assert_eq!(
                build(draft("a.html", kind, 1)).unwrap_err(),
                DomainError::UnsupportedAttachmentType,
                "{kind:?}"
            );
        }
        for size in [0, MAX_ATTACHMENT_BYTES + 1] {
            assert_eq!(
                build(draft("a.pdf", "application/pdf", size)).unwrap_err(),
                DomainError::InvalidAttachmentSize,
                "{size}"
            );
        }
    }
}
