-- Fiscal documents (ADR 0073, ADR 0076): an NF-e on its way to, or past, the authority, and the files it made. The
-- constraints repeat the rules of `domain::FiscalDocument` that one row can show. That a sale has one live document is a
-- partial unique index; that a status follows another is the domain's (and the use case's, under the row's lock).
-- Our id is the gateway's reference. The lines are not here: they are in the XML the authority signed.
CREATE TABLE fiscal_documents (
    id                    UUID PRIMARY KEY,
    company_id            UUID        NOT NULL REFERENCES companies (id),
    kind                  TEXT        NOT NULL,
    sale_id               UUID        NOT NULL,
    issuer_cnpj           TEXT        NOT NULL,
    issuer_name           TEXT        NOT NULL,
    regime                TEXT        NOT NULL,
    environment           TEXT        NOT NULL,
    recipient_name        TEXT        NOT NULL,
    recipient_document    TEXT,
    nature_code           TEXT        NOT NULL,
    total                 NUMERIC     NOT NULL,
    issued_at             BIGINT      NOT NULL,
    issued_on             DATE        NOT NULL,
    submitted_by_user_id  UUID        NOT NULL,
    submitted_by_name     TEXT        NOT NULL,
    status                TEXT        NOT NULL,
    refusal_code          TEXT,
    refusal_message       TEXT,
    access_key            TEXT,
    number                BIGINT,
    series                SMALLINT,
    protocol              TEXT,
    note                  TEXT,
    CONSTRAINT fiscal_documents_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT fiscal_documents_sale_fk FOREIGN KEY (company_id, sale_id) REFERENCES sales (company_id, id),
    CONSTRAINT fiscal_documents_kind_valid CHECK (kind IN ('nfe')),
    CONSTRAINT fiscal_documents_status_valid CHECK (
        status IN ('draft', 'processing', 'authorized', 'contingency', 'rejected', 'denied', 'cancelled')
    ),
    CONSTRAINT fiscal_documents_issuer_valid CHECK (
        issuer_cnpj ~ '^[0-9A-Z]{14}$'
        AND char_length(issuer_name) BETWEEN 1 AND 150 AND issuer_name !~ '^\s|\s$' AND issuer_name !~ '[[:cntrl:]]'
    ),
    CONSTRAINT fiscal_documents_regime_valid CHECK (regime IN ('simples_nacional', 'lucro_presumido')),
    CONSTRAINT fiscal_documents_environment_valid CHECK (environment IN ('homologation', 'production')),
    CONSTRAINT fiscal_documents_recipient_valid CHECK (
        char_length(recipient_name) BETWEEN 1 AND 150 AND recipient_name !~ '^\s|\s$' AND recipient_name !~ '[[:cntrl:]]'
        AND (recipient_document IS NULL OR recipient_document ~ '^([0-9A-Z]{11}|[0-9A-Z]{14})$')
    ),
    CONSTRAINT fiscal_documents_nature_valid CHECK (
        char_length(nature_code) BETWEEN 1 AND 150 AND nature_code !~ '^\s|\s$' AND nature_code !~ '[[:cntrl:]]'
    ),
    CONSTRAINT fiscal_documents_total_valid CHECK (total > 0 AND total = round(total, 2)),
    CONSTRAINT fiscal_documents_submitted_by_valid CHECK (
        char_length(submitted_by_name) BETWEEN 1 AND 150 AND submitted_by_name !~ '[[:cntrl:]]'
    ),
    -- An authorized or cancelled document has what the authority gave; no other does. `IS NOT NULL` is explicit because
    -- a CHECK passes when its result is NULL.
    CONSTRAINT fiscal_documents_authorization_follows_status CHECK (
        (status IN ('authorized', 'cancelled')
            AND access_key IS NOT NULL AND access_key ~ '^[0-9]{44}$'
            AND number IS NOT NULL AND number BETWEEN 1 AND 999999999
            AND series IS NOT NULL AND series BETWEEN 1 AND 999)
        OR (status NOT IN ('authorized', 'cancelled')
            AND access_key IS NULL AND number IS NULL AND series IS NULL AND protocol IS NULL)
    ),
    CONSTRAINT fiscal_documents_protocol_valid CHECK (
        protocol IS NULL OR (char_length(protocol) BETWEEN 1 AND 50 AND protocol !~ '[[:cntrl:]]')
    ),
    -- The refusal of a rejection or a denial, both parts or none.
    CONSTRAINT fiscal_documents_refusal_follows_status CHECK (
        (status IN ('rejected', 'denied') AND refusal_code IS NOT NULL AND refusal_message IS NOT NULL
            AND char_length(refusal_code) <= 50 AND char_length(refusal_message) <= 500
            AND refusal_code !~ '[[:cntrl:]]' AND refusal_message !~ '[[:cntrl:]]')
        OR (status NOT IN ('rejected', 'denied') AND refusal_code IS NULL AND refusal_message IS NULL)
    ),
    CONSTRAINT fiscal_documents_note_valid CHECK (
        note IS NULL OR (char_length(note) BETWEEN 1 AND 500 AND note !~ '[[:cntrl:]]')
    )
);

-- At most one live document per sale and kind: asking to issue twice finds the same one. A rejected, denied or
-- cancelled document frees the sale.
CREATE UNIQUE INDEX fiscal_documents_one_live_per_sale_key
    ON fiscal_documents (company_id, sale_id, kind)
    WHERE status IN ('draft', 'processing', 'authorized', 'contingency');

-- An access key names one document.
CREATE UNIQUE INDEX fiscal_documents_access_key_key
    ON fiscal_documents (company_id, access_key) WHERE access_key IS NOT NULL;

-- A fiscal document is the record of an act: it is never deleted, and once authorized its content never changes, only
-- its status (to cancelled). A trigger holds that even against a bug in the application.
CREATE FUNCTION fiscal_documents_keep_the_record() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'a fiscal document is never deleted';
    END IF;
    IF OLD.status IN ('authorized', 'cancelled') AND (
        NEW.company_id, NEW.kind, NEW.sale_id, NEW.issuer_cnpj, NEW.issuer_name, NEW.regime, NEW.environment,
        NEW.recipient_name, NEW.recipient_document, NEW.nature_code, NEW.total, NEW.issued_at, NEW.issued_on,
        NEW.submitted_by_user_id, NEW.submitted_by_name, NEW.access_key, NEW.number, NEW.series, NEW.protocol
    ) IS DISTINCT FROM (
        OLD.company_id, OLD.kind, OLD.sale_id, OLD.issuer_cnpj, OLD.issuer_name, OLD.regime, OLD.environment,
        OLD.recipient_name, OLD.recipient_document, OLD.nature_code, OLD.total, OLD.issued_at, OLD.issued_on,
        OLD.submitted_by_user_id, OLD.submitted_by_name, OLD.access_key, OLD.number, OLD.series, OLD.protocol
    ) THEN
        RAISE EXCEPTION 'the content of an authorized fiscal document never changes';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER fiscal_documents_are_a_record
    BEFORE UPDATE OR DELETE ON fiscal_documents
    FOR EACH ROW EXECUTE FUNCTION fiscal_documents_keep_the_record();

-- The files of a document: the record that says whose the bytes in the blob store are. One per document and kind.
CREATE TABLE fiscal_files (
    blob_id      UUID PRIMARY KEY,
    company_id   UUID    NOT NULL,
    document_id  UUID    NOT NULL,
    kind         TEXT    NOT NULL,
    size         BIGINT  NOT NULL,
    CONSTRAINT fiscal_files_document_fk FOREIGN KEY (company_id, document_id)
        REFERENCES fiscal_documents (company_id, id),
    CONSTRAINT fiscal_files_one_per_kind_key UNIQUE (company_id, document_id, kind),
    CONSTRAINT fiscal_files_kind_valid CHECK (kind IN ('xml', 'pdf', 'cancellation_xml')),
    CONSTRAINT fiscal_files_size_valid CHECK (size > 0)
);

CREATE FUNCTION fiscal_files_are_a_record() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    RAISE EXCEPTION 'a fiscal file is never changed or deleted';
END;
$$;

CREATE TRIGGER fiscal_files_never_change
    BEFORE UPDATE OR DELETE ON fiscal_files
    FOR EACH ROW EXECUTE FUNCTION fiscal_files_are_a_record();
