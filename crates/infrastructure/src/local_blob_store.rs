use std::io::ErrorKind;
use std::path::PathBuf;

use conta_verde_application::{BlobStore, RepositoryError};
use conta_verde_domain::{BlobId, CompanyId};

/// Keeps the bytes of attachments as files in a directory ([ADR 0036]): `<root>/<company>/<attachment>`.
/// Both names are ids the program made, never anything a person typed, so a path cannot be steered out of
/// the directory.
#[derive(Debug, Clone)]
pub struct LocalBlobStore {
    root: PathBuf,
}

impl LocalBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn company_dir(&self, company_id: CompanyId) -> PathBuf {
        self.root.join(company_id.as_uuid().to_string())
    }

    fn file(&self, company_id: CompanyId, id: BlobId) -> PathBuf {
        self.company_dir(company_id).join(id.as_uuid().to_string())
    }
}

fn io_error(error: std::io::Error) -> RepositoryError {
    RepositoryError::Storage(format!("attachment file: {error}"))
}

impl BlobStore for LocalBlobStore {
    async fn put(
        &self,
        company_id: CompanyId,
        id: BlobId,
        bytes: &[u8],
    ) -> Result<(), RepositoryError> {
        tokio::fs::create_dir_all(self.company_dir(company_id))
            .await
            .map_err(io_error)?;
        // Written aside and renamed, so a reader never sees half a file and a crash leaves no file
        // under the name the record points at.
        let target = self.file(company_id, id);
        let partial = target.with_extension("partial");
        tokio::fs::write(&partial, bytes).await.map_err(io_error)?;
        tokio::fs::rename(&partial, &target).await.map_err(io_error)
    }

    async fn get(
        &self,
        company_id: CompanyId,
        id: BlobId,
    ) -> Result<Option<Vec<u8>>, RepositoryError> {
        match tokio::fs::read(self.file(company_id, id)).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error)),
        }
    }

    async fn delete(&self, company_id: CompanyId, id: BlobId) -> Result<(), RepositoryError> {
        match tokio::fs::remove_file(self.file(company_id, id)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn keeps_files_apart_by_company_like_every_blob_store() {
        let root = std::env::temp_dir().join(format!("conta-verde-blobs-{}", uuid::Uuid::new_v4()));
        let store = LocalBlobStore::new(&root);
        contract::keeps_blobs(&store, CompanyId::generate(), CompanyId::generate()).await;
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
    }

    #[tokio::test]
    async fn keeps_an_attachments_file_where_it_always_was() {
        let root = std::env::temp_dir().join(format!("conta-verde-blobs-{}", uuid::Uuid::new_v4()));
        let store = LocalBlobStore::new(&root);
        let company = CompanyId::generate();
        let attachment = conta_verde_domain::AttachmentId::generate();
        store
            .put(company, attachment.into(), b"bytes")
            .await
            .unwrap();
        let path = root
            .join(company.as_uuid().to_string())
            .join(attachment.as_uuid().to_string());
        assert_eq!(std::fs::read(path).unwrap(), b"bytes");
        assert_eq!(
            store.get(company, BlobId::from(attachment)).await.unwrap(),
            Some(b"bytes".to_vec())
        );
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
    }
}
