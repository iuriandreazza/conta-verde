use conta_verde_domain::{
    Actor, Attachment, AttachmentDraft, AttachmentId, AuditAction, CompanyId, EntryId,
    MAX_ENTRY_ATTACHMENTS, attachment_changes, attachment_creation_changes,
};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, AttachmentRepository, BlobStore, FinancialEntryRepository, RepositoryError,
};

/// An attachment with its bytes, for a download.
#[derive(Debug, Clone)]
pub struct AttachedFile {
    pub attachment: Attachment,
    pub bytes: Vec<u8>,
}

/// Files attached to the entries of a company: the invoice, the receipt ([ADR 0036]).
pub struct AttachmentService<R, B, E> {
    repository: R,
    blobs: B,
    entries: E,
}

impl<R, B, E> AttachmentService<R, B, E>
where
    R: AttachmentRepository,
    B: BlobStore,
    E: FinancialEntryRepository,
{
    pub fn new(repository: R, blobs: B, entries: E) -> Self {
        Self {
            repository,
            blobs,
            entries,
        }
    }

    /// Validates the file and that the entry is the company's, keeps the bytes, then stores the
    /// record with its creation in the audit trail. The bytes go first: a record never points at a file
    /// that is not there, and a failed record write takes the bytes back.
    pub async fn attach(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<Attachment, ApplicationError> {
        let attachment = Attachment::new(
            AttachmentId::generate(),
            company_id,
            entry_id,
            AttachmentDraft {
                file_name: file_name.to_owned(),
                content_type: content_type.to_owned(),
                size: bytes.len() as u64,
            },
        )?;
        self.ensure_entry(company_id, entry_id).await?;
        if self
            .repository
            .list_by_entry(company_id, entry_id)
            .await?
            .len()
            >= MAX_ENTRY_ATTACHMENTS
        {
            return Err(ApplicationError::TooManyAttachments);
        }
        self.blobs.put(company_id, attachment.id(), bytes).await?;
        let audit = audit_entry(
            &attachment,
            actor,
            AuditAction::Created,
            attachment_creation_changes(&attachment),
        );
        if let Err(error) = self.repository.save(&attachment, &audit).await {
            // The write failed, so nothing refers to the bytes: a leftover file would only be clutter,
            // and the error to report is the write's.
            let _ = self.blobs.delete(company_id, attachment.id()).await;
            return Err(error.into());
        }
        Ok(attachment)
    }

    /// The attachments of the entry, in the order they were attached.
    pub async fn list(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Attachment>, ApplicationError> {
        self.ensure_entry(company_id, entry_id).await?;
        Ok(self.repository.list_by_entry(company_id, entry_id).await?)
    }

    /// The attachment and its bytes. One that is removed, or not of this entry, is not found.
    pub async fn download(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> Result<AttachedFile, ApplicationError> {
        let attachment = self.find(company_id, entry_id, id).await?;
        let bytes = self.blobs.get(company_id, id).await?.ok_or_else(|| {
            RepositoryError::Storage(format!("the bytes of attachment {id:?} are missing"))
        })?;
        Ok(AttachedFile { attachment, bytes })
    }

    /// Removes an attachment: it is marked removed and stays in the trail, and its bytes are dropped.
    pub async fn remove(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> Result<(), ApplicationError> {
        let stored = self.find(company_id, entry_id, id).await?;
        let removed = stored.clone().with_removed(true);
        let audit = audit_entry(
            &removed,
            actor,
            AuditAction::Updated,
            attachment_changes(&stored, &removed),
        );
        self.repository
            .remove(&removed, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::AttachmentNotFound,
                other => other.into(),
            })?;
        // The record is what says the file exists, and it is gone: bytes that could not be dropped are
        // unreachable clutter, not something to fail a removal that has happened.
        let _ = self.blobs.delete(company_id, id).await;
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> Result<Attachment, ApplicationError> {
        self.repository
            .find(company_id, entry_id, id)
            .await?
            .filter(|attachment| !attachment.is_removed())
            .ok_or(ApplicationError::AttachmentNotFound)
    }

    async fn ensure_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<(), ApplicationError> {
        self.entries
            .find(company_id, entry_id)
            .await?
            .map(|_| ())
            .ok_or(ApplicationError::EntryNotFound)
    }
}
