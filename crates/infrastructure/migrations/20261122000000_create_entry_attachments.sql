-- Records of the files attached to financial entries (ADR 0036); the bytes are not here, they are in the
-- blob store under the company and the attachment id. The constraints repeat the rules of
-- `domain::Attachment` that one row can show; that an entry holds at most 20 files at once spans rows
-- and is the use case's.

CREATE TABLE entry_attachments (
    id           UUID PRIMARY KEY,
    company_id   UUID        NOT NULL,
    entry_id     UUID        NOT NULL,
    file_name    TEXT        NOT NULL,
    content_type TEXT        NOT NULL,
    size_bytes   BIGINT      NOT NULL,
    -- Never edited or deleted: removing one marks it, and the trail keeps who and when.
    removed      BOOLEAN     NOT NULL DEFAULT false,
    -- The order they were attached in, which a list keeps.
    seq          BIGSERIAL   NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT entry_attachments_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT entry_attachments_company_fk FOREIGN KEY (company_id) REFERENCES companies (id),
    CONSTRAINT entry_attachments_entry_fk FOREIGN KEY (company_id, entry_id)
        REFERENCES financial_entries (company_id, id),
    CONSTRAINT entry_attachments_file_name_valid CHECK (
        char_length(file_name) BETWEEN 1 AND 200
        AND file_name = btrim(file_name)
        AND file_name !~ '[[:cntrl:]/\\]'
        AND file_name NOT IN ('.', '..')
    ),
    CONSTRAINT entry_attachments_content_type_valid CHECK (
        content_type IN (
            'application/pdf', 'image/png', 'image/jpeg', 'image/webp',
            'text/plain', 'text/csv', 'application/xml', 'text/xml'
        )
    ),
    CONSTRAINT entry_attachments_size_valid CHECK (size_bytes BETWEEN 1 AND 10485760)
);

CREATE INDEX entry_attachments_entry_idx ON entry_attachments (company_id, entry_id, seq);
