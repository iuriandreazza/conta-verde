-- Lines of bank statements and their reconciliation (ADR 0034). The constraints repeat the rules of
-- `domain::StatementLine` that one row can show; that the settlement or the transfer a line is matched to
-- is in the same account, is not reversed and has the same amount spans rows, and is the use case's.

-- The targets of the foreign keys below: a matched line is in the company of what it is matched to.
ALTER TABLE entry_settlements ADD CONSTRAINT entry_settlements_company_id_id_key UNIQUE (company_id, id);
ALTER TABLE account_transfers ADD CONSTRAINT account_transfers_company_id_id_key UNIQUE (company_id, id);

CREATE TABLE statement_lines (
    id                    UUID PRIMARY KEY,
    company_id            UUID        NOT NULL REFERENCES companies (id),
    account_id            UUID        NOT NULL,
    posted_on             DATE        NOT NULL,
    -- Signed, in cents, never 0: positive came into the account and negative went out of it.
    amount                NUMERIC     NOT NULL,
    description           TEXT        NOT NULL,
    bank_ref              TEXT,
    -- Day, amount, description, reference and place among identical lines of the file: sending a file
    -- again adds nothing.
    fingerprint           TEXT        NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'pending',
    matched_settlement_id UUID,
    matched_transfer_id   UUID,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT statement_lines_account_fk FOREIGN KEY (company_id, account_id)
        REFERENCES bank_accounts (company_id, id),
    -- A foreign key with a null column is not checked, which is what an optional reference wants.
    CONSTRAINT statement_lines_settlement_fk FOREIGN KEY (company_id, matched_settlement_id)
        REFERENCES entry_settlements (company_id, id),
    CONSTRAINT statement_lines_transfer_fk FOREIGN KEY (company_id, matched_transfer_id)
        REFERENCES account_transfers (company_id, id),
    CONSTRAINT statement_lines_status_valid CHECK (status IN ('pending', 'matched', 'ignored')),
    -- Matched to exactly one thing when matched, and to nothing otherwise.
    CONSTRAINT statement_lines_match_whole CHECK (
        (status = 'matched' AND num_nonnulls(matched_settlement_id, matched_transfer_id) = 1)
        OR (status <> 'matched' AND matched_settlement_id IS NULL AND matched_transfer_id IS NULL)
    ),
    CONSTRAINT statement_lines_amount_valid CHECK (
        amount = round(amount, 2) AND amount <> 0
        AND amount > -10000000000000 AND amount < 10000000000000
    ),
    CONSTRAINT statement_lines_posted_on_valid CHECK (
        posted_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT statement_lines_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT statement_lines_bank_ref_valid CHECK (
        bank_ref IS NULL OR (
            char_length(bank_ref) BETWEEN 1 AND 100
            AND bank_ref = btrim(bank_ref)
            AND bank_ref !~ '[[:cntrl:]]'
        )
    )
);

-- At most one line per fingerprint in an account, one per settlement, and one per transfer in each account.
CREATE UNIQUE INDEX statement_lines_account_fingerprint_key ON statement_lines (account_id, fingerprint);
CREATE UNIQUE INDEX statement_lines_settlement_key
    ON statement_lines (matched_settlement_id) WHERE matched_settlement_id IS NOT NULL;
-- A transfer is two lines of two statements, one in each account: each account matches it once.
CREATE UNIQUE INDEX statement_lines_transfer_key
    ON statement_lines (matched_transfer_id, account_id) WHERE matched_transfer_id IS NOT NULL;

-- The list is newest day first, then id (ADR 0018), narrowed by account and status.
CREATE INDEX statement_lines_company_day_idx ON statement_lines (company_id, posted_on, id);
CREATE INDEX statement_lines_company_account_status_idx ON statement_lines (company_id, account_id, status);
