-- Received invoices (ADR 0083): the NF-e that other companies issued against this one, as the gateway lists them, so that the
-- company can declare its word to the authority (the recipient's manifestation). The rows are a **mirror** of the gateway's list
-- (not audited, like a payment notice): what the person declares is the business act, and it is audited.
CREATE TABLE received_invoices (
    id                      UUID PRIMARY KEY,
    company_id              UUID        NOT NULL REFERENCES companies (id),
    access_key              TEXT        NOT NULL,
    issuer_name             TEXT        NOT NULL,
    issuer_document         TEXT        NOT NULL,
    total                   NUMERIC     NOT NULL,
    issued_on               DATE        NOT NULL,
    situation               TEXT        NOT NULL,
    manifestation           TEXT        NOT NULL,
    manifestation_reason    TEXT,
    gateway_version         BIGINT      NOT NULL,
    CONSTRAINT received_invoices_company_id_id_key UNIQUE (company_id, id),
    -- An invoice is the company's once: the gateway listing it again changes nothing but what it says now.
    CONSTRAINT received_invoices_key_key UNIQUE (company_id, access_key),
    CONSTRAINT received_invoices_access_key_valid CHECK (access_key ~ '^[0-9]{6}[0-9A-Z]{14}[0-9]{24}$'),
    CONSTRAINT received_invoices_issuer_valid CHECK (
        char_length(issuer_name) BETWEEN 1 AND 150 AND issuer_name !~ '^\s|\s$' AND issuer_name !~ '[[:cntrl:]]'
        AND issuer_document ~ '^([0-9A-Z]{11}|[0-9A-Z]{14})$'
    ),
    CONSTRAINT received_invoices_total_valid CHECK (total >= 0 AND total = round(total, 2)),
    CONSTRAINT received_invoices_situation_valid CHECK (situation IN ('authorized', 'cancelled', 'denied')),
    CONSTRAINT received_invoices_manifestation_valid CHECK (
        manifestation IN ('none', 'awareness', 'confirmation', 'unknown', 'not_performed')
    ),
    -- Only "operation not performed" has a reason (15 to 255 characters on one line); the company may not know it, because the
    -- word may have been declared outside this system and the gateway's list does not give it.
    CONSTRAINT received_invoices_reason_follows_word CHECK (
        manifestation_reason IS NULL
        OR (manifestation = 'not_performed' AND char_length(manifestation_reason) BETWEEN 15 AND 255
            AND manifestation_reason !~ '[[:cntrl:]]')
    ),
    CONSTRAINT received_invoices_version_valid CHECK (gateway_version >= 0)
);

CREATE INDEX received_invoices_list_idx ON received_invoices (company_id, issued_on DESC, id DESC);
