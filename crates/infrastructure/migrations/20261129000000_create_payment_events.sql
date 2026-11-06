-- Payment webhooks and what they settle (ADR 0046). The constraints repeat the rules of the domain
-- types that one row can show; that a charge settles at most once and that a charge has at most one fee
-- entry are the unique indexes below, which is what makes a notice that arrives twice safe.

-- The receipt of a paid charge: the day, what was paid, the provider's fee, and why the installment was
-- not settled when it was not. A charge is paid exactly when it has them.
ALTER TABLE charges
    ADD COLUMN paid_on     DATE,
    ADD COLUMN paid_amount NUMERIC,
    ADD COLUMN fee         NUMERIC,
    ADD COLUMN note        TEXT;

ALTER TABLE charges
    ADD CONSTRAINT charges_receipt_fits_status CHECK (
        (status = 'paid') = (paid_on IS NOT NULL AND paid_amount IS NOT NULL AND fee IS NOT NULL)
        AND (status = 'paid' OR (paid_on IS NULL AND paid_amount IS NULL AND fee IS NULL
                                 AND note IS NULL))
    ),
    ADD CONSTRAINT charges_receipt_valid CHECK (
        (paid_on IS NULL OR paid_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31')
        AND (paid_amount IS NULL OR (paid_amount = round(paid_amount, 2) AND paid_amount > 0
                                     AND paid_amount < 10000000000000))
        AND (fee IS NULL OR (fee = round(fee, 2) AND fee >= 0 AND fee < 10000000000000))
        AND (note IS NULL OR (char_length(note) BETWEEN 1 AND 200 AND note = btrim(note)
                              AND note !~ '[[:cntrl:]]'))
    );

-- The settlement a charge's payment made: a charge settles at most once, even if it is reversed later.
ALTER TABLE entry_settlements ADD COLUMN charge_id UUID;
ALTER TABLE entry_settlements
    ADD CONSTRAINT entry_settlements_charge_fk FOREIGN KEY (company_id, charge_id)
        REFERENCES charges (company_id, id);
CREATE UNIQUE INDEX entry_settlements_charge_key
    ON entry_settlements (charge_id) WHERE charge_id IS NOT NULL;

-- The payable entry of the fee a provider kept: one per charge. The origin stays at most one.
ALTER TABLE financial_entries ADD COLUMN charge_fee_id UUID;
ALTER TABLE financial_entries
    DROP CONSTRAINT financial_entries_one_origin,
    ADD CONSTRAINT financial_entries_charge_fee_fk FOREIGN KEY (company_id, charge_fee_id)
        REFERENCES charges (company_id, id),
    ADD CONSTRAINT financial_entries_one_origin CHECK (
        num_nonnulls(recurrence_id, sale_id, sale_return_id, charge_fee_id) <= 1
    );
CREATE UNIQUE INDEX financial_entries_charge_fee_key
    ON financial_entries (charge_fee_id) WHERE charge_fee_id IS NOT NULL;

-- How a company collects: one row, where the money lands and what the fee is entered under.
CREATE TABLE collection_settings (
    company_id      UUID PRIMARY KEY REFERENCES companies (id),
    bank_account_id UUID NOT NULL,
    fee_category_id UUID,
    CONSTRAINT collection_settings_bank_account_fk FOREIGN KEY (company_id, bank_account_id)
        REFERENCES bank_accounts (company_id, id),
    -- A foreign key with a null column is not checked, which is what an optional reference wants.
    CONSTRAINT collection_settings_fee_category_fk FOREIGN KEY (company_id, fee_category_id)
        REFERENCES categories (company_id, id)
);

-- What providers told us, kept as it came, once per provider's id within a company.
CREATE TABLE payment_events (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL,
    charge_id  UUID        NOT NULL,
    provider   TEXT        NOT NULL,
    event_id   TEXT        NOT NULL,
    kind       TEXT        NOT NULL,
    outcome    TEXT        NOT NULL DEFAULT 'pending',
    note       TEXT,
    raw        BYTEA       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT payment_events_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT payment_events_charge_fk FOREIGN KEY (company_id, charge_id)
        REFERENCES charges (company_id, id),
    CONSTRAINT payment_events_provider_event_key UNIQUE (company_id, provider, event_id),
    CONSTRAINT payment_events_provider_valid CHECK (provider ~ '^[a-z][a-z0-9_]{0,31}$'),
    CONSTRAINT payment_events_event_id_valid CHECK (
        char_length(event_id) BETWEEN 1 AND 200
        AND event_id = btrim(event_id)
        AND event_id !~ '[[:cntrl:]]'
    ),
    CONSTRAINT payment_events_kind_valid CHECK (kind IN ('paid', 'cancelled', 'expired')),
    CONSTRAINT payment_events_outcome_valid CHECK (outcome IN ('pending', 'applied', 'not_applied')),
    CONSTRAINT payment_events_note_valid CHECK (
        note IS NULL OR (char_length(note) BETWEEN 1 AND 200 AND note !~ '[[:cntrl:]]')
    ),
    CONSTRAINT payment_events_raw_valid CHECK (octet_length(raw) <= 65536)
);

CREATE INDEX payment_events_charge_idx ON payment_events (company_id, charge_id, created_at);
