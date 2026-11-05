-- Bank accounts of a company: checking, savings, cash and card. No balance: opening balances and
-- movements come with the entries (Phase 2). The constraints repeat the rules of
-- `domain::BankAccount`, so a bug in the application cannot store what the domain would refuse.
CREATE TABLE bank_accounts (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    kind       TEXT        NOT NULL,
    name       TEXT        NOT NULL,
    -- The institution: 3 digits (COMPE), or 8 (ISPB) for one that has no such code.
    bank_code  TEXT,
    -- Upper case letters and digits only, without the punctuation people write: one form, so that
    -- "the same account" is something an index can see.
    branch     TEXT,
    number     TEXT,
    active     BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT bank_accounts_kind_valid CHECK (kind IN ('checking', 'savings', 'cash', 'card')),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT bank_accounts_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT bank_accounts_bank_code_shape CHECK (bank_code IS NULL OR bank_code ~ '^([0-9]{3}|[0-9]{8})$'),
    CONSTRAINT bank_accounts_branch_shape CHECK (branch IS NULL OR branch ~ '^[0-9A-Z]{1,10}$'),
    CONSTRAINT bank_accounts_number_shape CHECK (number IS NULL OR number ~ '^[0-9A-Z]{1,20}$'),
    CONSTRAINT bank_accounts_cash_has_no_bank CHECK (
        kind <> 'cash' OR (bank_code IS NULL AND branch IS NULL AND number IS NULL)
    )
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX bank_accounts_company_name_key ON bank_accounts (company_id, lower(name));

-- One account per bank, branch and number, when all three are known.
CREATE UNIQUE INDEX bank_accounts_company_identity_key
    ON bank_accounts (company_id, bank_code, branch, number)
    WHERE bank_code IS NOT NULL AND branch IS NOT NULL AND number IS NOT NULL;
