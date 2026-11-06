-- Settlements of entry installments (ADR 0030): a payment of an installment, in part or in full, on a
-- day, from a bank account. The constraints repeat the rules of `domain::Settlement` that one row can
-- show. The rules that span rows are the adapter's, inside the write: an installment is not settled
-- for more than it is worth (its row is locked for the check), and an entry with settlements that are
-- not reversed keeps its installments and kind.

-- The targets of the settlements' composite foreign keys: a reference is to a record of the same
-- company.
ALTER TABLE bank_accounts ADD CONSTRAINT bank_accounts_company_id_id_key UNIQUE (company_id, id);
ALTER TABLE payment_methods ADD CONSTRAINT payment_methods_company_id_id_key UNIQUE (company_id, id);

CREATE TABLE entry_settlements (
    id                UUID PRIMARY KEY,
    company_id        UUID        NOT NULL,
    entry_id          UUID        NOT NULL,
    -- The number of the installment it pays. No foreign key to the installments: they are replaced
    -- as a whole when an entry is edited, and the adapter refuses to change them under a settlement.
    installment       SMALLINT    NOT NULL,
    settled_on        DATE        NOT NULL,
    -- Unconstrained scale on purpose, like the installments: more than two decimals is refused,
    -- never rounded (the checks below enforce it).
    applied           NUMERIC     NOT NULL,
    interest          NUMERIC     NOT NULL,
    fine              NUMERIC     NOT NULL,
    discount          NUMERIC     NOT NULL,
    bank_account_id   UUID        NOT NULL,
    payment_method_id UUID,
    -- A settlement is never edited or deleted: undoing one marks it, and the trail keeps who and when.
    reversed          BOOLEAN     NOT NULL DEFAULT false,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT entry_settlements_entry_fk FOREIGN KEY (company_id, entry_id)
        REFERENCES financial_entries (company_id, id),
    CONSTRAINT entry_settlements_bank_account_fk FOREIGN KEY (company_id, bank_account_id)
        REFERENCES bank_accounts (company_id, id),
    -- A foreign key with a null column is not checked, which is what an optional reference wants.
    CONSTRAINT entry_settlements_payment_method_fk FOREIGN KEY (company_id, payment_method_id)
        REFERENCES payment_methods (company_id, id),
    CONSTRAINT entry_settlements_installment_valid CHECK (installment BETWEEN 1 AND 120),
    CONSTRAINT entry_settlements_settled_on_valid CHECK (
        settled_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT entry_settlements_applied_valid CHECK (
        applied = round(applied, 2) AND applied > 0 AND applied < 10000000000000
    ),
    CONSTRAINT entry_settlements_adjustments_valid CHECK (
        interest = round(interest, 2) AND interest >= 0 AND interest < 10000000000000
        AND fine = round(fine, 2) AND fine >= 0 AND fine < 10000000000000
        AND discount = round(discount, 2) AND discount >= 0 AND discount < 10000000000000
    ),
    -- A settlement that moves no money is not a payment.
    CONSTRAINT entry_settlements_moves_money CHECK (applied + interest + fine - discount > 0)
);

CREATE INDEX entry_settlements_company_entry_idx ON entry_settlements (company_id, entry_id);
