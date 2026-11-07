-- The log of documents e-mailed on request (ADR 0064): which, to whom, by whom, when. Never edited; a row is deleted only to
-- take back a send that went nowhere. Not audited: the document did not change. `document_id` is a purchase order or a
-- settlement by `kind`, so the database cannot hold a foreign key to it: the use case finds the document in the company
-- before it records the send. `sent_at` is Unix milliseconds from the caller's clock, so a test can move it.
CREATE TABLE document_mails (
    id         UUID PRIMARY KEY,
    company_id UUID   NOT NULL REFERENCES companies (id),
    kind       TEXT   NOT NULL CHECK (kind IN ('purchase_order', 'payment_receipt')),
    document_id UUID  NOT NULL,
    to_address TEXT   NOT NULL CHECK (to_address <> '' AND strpos(to_address, E'\n') = 0),
    sent_by    UUID   NOT NULL REFERENCES users (id),
    sent_at    BIGINT NOT NULL
);

CREATE INDEX document_mails_company_day_idx ON document_mails (company_id, sent_at DESC);
CREATE INDEX document_mails_document_idx ON document_mails (company_id, kind, document_id, sent_at DESC);
