-- Payment methods of a company: how it pays and is paid. The constraints repeat the rules of
-- `domain::PaymentMethod`. The kind keys are stored and do not change.
CREATE TABLE payment_methods (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    kind       TEXT        NOT NULL,
    name       TEXT        NOT NULL,
    active     BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT payment_methods_kind_valid CHECK (kind IN (
        'cash', 'pix', 'bank_slip', 'credit_card', 'debit_card', 'bank_transfer', 'check', 'other'
    )),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT payment_methods_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200)
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX payment_methods_company_name_key ON payment_methods (company_id, lower(name));
