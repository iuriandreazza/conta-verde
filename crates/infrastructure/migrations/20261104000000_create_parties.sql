-- company_id has no foreign key yet: the companies table arrives with authentication and tenancy (see ADR 0004).
CREATE TABLE parties (
    id          UUID PRIMARY KEY,
    company_id  UUID        NOT NULL,
    name        TEXT        NOT NULL,
    document    TEXT,
    email       TEXT,
    is_customer BOOLEAN     NOT NULL,
    is_supplier BOOLEAN     NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT parties_has_role CHECK (is_customer OR is_supplier)
);

CREATE UNIQUE INDEX parties_company_document_key
    ON parties (company_id, document)
    WHERE document IS NOT NULL;

CREATE INDEX parties_company_name_idx ON parties (company_id, lower(name), id);
