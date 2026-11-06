-- Transfers between the accounts of a company, and the opening balance of an account (ADR 0031). The
-- constraints repeat the rules of `domain::Transfer` and `domain::AccountOpening` that one row can show;
-- that the accounts of a transfer are active is the use case's. Balances are added up on every read:
-- nothing here stores one.

CREATE TABLE account_transfers (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL,
    from_account_id UUID        NOT NULL,
    to_account_id   UUID        NOT NULL,
    -- Unconstrained scale on purpose: more than two decimals is refused, never rounded.
    amount          NUMERIC     NOT NULL,
    occurred_on     DATE        NOT NULL,
    note            TEXT,
    -- Never edited or deleted: undoing one marks it, and the trail keeps who and when.
    reversed        BOOLEAN     NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT account_transfers_company_fk FOREIGN KEY (company_id) REFERENCES companies (id),
    CONSTRAINT account_transfers_from_fk FOREIGN KEY (company_id, from_account_id)
        REFERENCES bank_accounts (company_id, id),
    CONSTRAINT account_transfers_to_fk FOREIGN KEY (company_id, to_account_id)
        REFERENCES bank_accounts (company_id, id),
    CONSTRAINT account_transfers_two_accounts CHECK (from_account_id <> to_account_id),
    CONSTRAINT account_transfers_amount_valid CHECK (
        amount = round(amount, 2) AND amount > 0 AND amount < 10000000000000
    ),
    CONSTRAINT account_transfers_occurred_on_valid CHECK (
        occurred_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT account_transfers_note_valid CHECK (
        note IS NULL OR (
            char_length(note) BETWEEN 1 AND 200 AND note = btrim(note) AND note !~ '[[:cntrl:]]'
        )
    )
);

-- The list is newest first, then by id (ADR 0018).
CREATE INDEX account_transfers_company_day_idx ON account_transfers (company_id, occurred_on, id);

-- The balance an account held at the start of a day: one per account, replaced as a whole.
CREATE TABLE account_openings (
    account_id UUID PRIMARY KEY,
    company_id UUID    NOT NULL,
    balance    NUMERIC NOT NULL,
    on_date    DATE    NOT NULL,
    CONSTRAINT account_openings_account_fk FOREIGN KEY (company_id, account_id)
        REFERENCES bank_accounts (company_id, id),
    CONSTRAINT account_openings_balance_valid CHECK (
        balance = round(balance, 2) AND balance > -10000000000000 AND balance < 10000000000000
    ),
    CONSTRAINT account_openings_on_date_valid CHECK (
        on_date BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    )
);

CREATE INDEX account_openings_company_idx ON account_openings (company_id);
