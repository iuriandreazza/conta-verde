-- Bank accounts of a company: checking, savings, cash and card. No balance: opening balances and
-- movements come with the entries (Phase 2). The constraints repeat the rules of
-- `domain::BankAccount` that one row can show, so a bug in the application cannot store those.
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
    CONSTRAINT bank_accounts_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200 AND name = btrim(name)),
    CONSTRAINT bank_accounts_bank_code_shape CHECK (bank_code IS NULL OR bank_code ~ '^([0-9]{3}|[0-9]{8})$'),
    CONSTRAINT bank_accounts_branch_shape CHECK (branch IS NULL OR branch ~ '^[0-9A-Z]{1,10}$'),
    CONSTRAINT bank_accounts_number_shape CHECK (number IS NULL OR number ~ '^[0-9A-Z]{1,20}$'),
    CONSTRAINT bank_accounts_cash_has_no_bank CHECK (
        kind <> 'cash' OR (bank_code IS NULL AND branch IS NULL AND number IS NULL)
    ),
    -- A card number must not be stored: the audit trail is append-only and would keep it for good.
    CONSTRAINT bank_accounts_card_has_no_number CHECK (
        kind <> 'card' OR (branch IS NULL AND number IS NULL)
    )
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX bank_accounts_company_name_key ON bank_accounts (company_id, lower(name));

-- One account of a kind per bank, branch and number, when all three are known. The kind is part of
-- it because some banks give a savings account the branch and number of the checking account.
-- It sees the same text twice, not the same account written differently: the ISPB instead of the
-- code, a branch with leading zeros, a number without its check digit are other values.
CREATE UNIQUE INDEX bank_accounts_company_identity_key
    ON bank_accounts (company_id, kind, bank_code, branch, number)
    WHERE bank_code IS NOT NULL AND branch IS NOT NULL AND number IS NOT NULL;
