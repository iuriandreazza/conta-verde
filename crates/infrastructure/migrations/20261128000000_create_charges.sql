-- Charges of the installments of receivables (ADR 0045): a request to be paid, made at a payment
-- provider. The constraints repeat the rules of `domain::Charge` that one row can show. That an
-- installment has at most one open charge is the adapter's inside the write, backed by the unique
-- index below; that the installment exists and has a balance spans rows and is the use case's.

CREATE TABLE charges (
    id                UUID PRIMARY KEY,
    company_id        UUID        NOT NULL,
    entry_id          UUID        NOT NULL,
    -- The number of the installment. No foreign key to the installments: they are replaced as a
    -- whole when an entry is edited, like the settlements' (ADR 0030).
    installment       SMALLINT    NOT NULL,
    method            TEXT        NOT NULL,
    status            TEXT        NOT NULL,
    -- What was left of the installment when the charge was made. Unconstrained scale on purpose, like
    -- the installments: more than two decimals is refused, never rounded.
    amount            NUMERIC     NOT NULL,
    due_on            DATE        NOT NULL,
    provider          TEXT        NOT NULL,
    external_id       TEXT        NOT NULL,
    digitable_line    TEXT,
    pix_payload       TEXT,
    payment_url       TEXT,
    -- Who issued it: what it settles is settled in their name (ADR 0044).
    issued_by_user_id UUID        NOT NULL,
    issued_by_name    TEXT        NOT NULL,
    -- The order they were issued in, which a list keeps.
    seq               BIGSERIAL   NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT charges_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT charges_company_fk FOREIGN KEY (company_id) REFERENCES companies (id),
    CONSTRAINT charges_entry_fk FOREIGN KEY (company_id, entry_id)
        REFERENCES financial_entries (company_id, id),
    CONSTRAINT charges_installment_valid CHECK (installment BETWEEN 1 AND 120),
    CONSTRAINT charges_method_valid CHECK (method IN ('boleto', 'pix', 'card_link')),
    CONSTRAINT charges_status_valid CHECK (status IN ('open', 'paid', 'cancelled', 'expired')),
    CONSTRAINT charges_amount_valid CHECK (
        amount = round(amount, 2) AND amount > 0 AND amount < 10000000000000
    ),
    CONSTRAINT charges_due_on_valid CHECK (due_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'),
    CONSTRAINT charges_provider_valid CHECK (provider ~ '^[a-z][a-z0-9_]{0,31}$'),
    CONSTRAINT charges_external_id_valid CHECK (
        char_length(external_id) BETWEEN 1 AND 200
        AND external_id = btrim(external_id)
        AND external_id !~ '[[:cntrl:]]'
    ),
    CONSTRAINT charges_digitable_line_valid CHECK (
        digitable_line IS NULL
        OR (char_length(digitable_line) <= 100 AND digitable_line ~ '^[0-9 .]+$')
    ),
    CONSTRAINT charges_pix_payload_valid CHECK (
        pix_payload IS NULL
        OR (char_length(pix_payload) <= 1000 AND pix_payload ~ '^[!-~]+$')
    ),
    CONSTRAINT charges_payment_url_valid CHECK (
        payment_url IS NULL
        OR (char_length(payment_url) <= 2000 AND payment_url ~ '^https://[^[:space:][:cntrl:]]+$')
    ),
    -- Each method has what it needs and nothing of another's.
    CONSTRAINT charges_payment_fits_method CHECK (
        (method = 'boleto' AND digitable_line IS NOT NULL AND pix_payload IS NULL)
        OR (method = 'pix' AND pix_payload IS NOT NULL AND digitable_line IS NULL)
        OR (method = 'card_link' AND payment_url IS NOT NULL
            AND digitable_line IS NULL AND pix_payload IS NULL)
    )
);

-- At most one open charge per installment.
CREATE UNIQUE INDEX charges_one_open_per_installment
    ON charges (company_id, entry_id, installment) WHERE status = 'open';
-- The provider's id of a charge is found again by the webhook (ADR 0046), within the company.
CREATE UNIQUE INDEX charges_provider_external_key ON charges (company_id, provider, external_id);
CREATE INDEX charges_entry_idx ON charges (company_id, entry_id, seq);
