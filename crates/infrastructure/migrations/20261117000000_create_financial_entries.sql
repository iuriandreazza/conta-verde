-- Financial entries of a company, receivable and payable, and their installments (ADR 0029). The
-- constraints repeat the rules of `domain::FinancialEntry` that one row can show. The rules that span
-- the rows of an entry (at least one installment and at most 120, the due dates not decreasing with
-- the number) are the domain's, and so is whether the category is active and not the transfers one:
-- the database refuses a reference to another company, which is what tenancy needs (ADR 0004).

-- The targets of the entries' composite foreign keys: a reference is to a record of the same company
-- (the parties have theirs since their contacts).
ALTER TABLE categories ADD CONSTRAINT categories_company_id_id_key UNIQUE (company_id, id);
ALTER TABLE cost_centers ADD CONSTRAINT cost_centers_company_id_id_key UNIQUE (company_id, id);

CREATE TABLE financial_entries (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    kind            TEXT        NOT NULL,
    description     TEXT        NOT NULL,
    party_id        UUID,
    category_id     UUID        NOT NULL,
    cost_center_id  UUID,
    competence_date DATE        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Only the target of the installments' foreign key: an installment is in its entry's company.
    CONSTRAINT financial_entries_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT financial_entries_category_fk FOREIGN KEY (company_id, category_id)
        REFERENCES categories (company_id, id),
    -- A foreign key with a null column is not checked, which is what an optional reference wants.
    CONSTRAINT financial_entries_cost_center_fk FOREIGN KEY (company_id, cost_center_id)
        REFERENCES cost_centers (company_id, id),
    CONSTRAINT financial_entries_party_fk FOREIGN KEY (company_id, party_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT financial_entries_kind_valid CHECK (kind IN ('receivable', 'payable')),
    CONSTRAINT financial_entries_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT financial_entries_competence_date_valid CHECK (
        competence_date BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    )
);

-- The list is by competence date, then id (ADR 0018), narrowed by kind or category.
CREATE INDEX financial_entries_company_competence_idx
    ON financial_entries (company_id, competence_date, id);
CREATE INDEX financial_entries_company_category_idx ON financial_entries (company_id, category_id);

CREATE TABLE financial_entry_installments (
    company_id UUID     NOT NULL,
    entry_id   UUID     NOT NULL,
    -- The order of the installments, from 1.
    number     SMALLINT NOT NULL,
    due_date   DATE     NOT NULL,
    -- Unconstrained scale on purpose: NUMERIC(15, 2) would round 10.005 to 10.01 on its own, and the
    -- rule is that more than two decimals is refused, never rounded (the check below enforces it).
    amount     NUMERIC  NOT NULL,
    PRIMARY KEY (entry_id, number),
    CONSTRAINT financial_entry_installments_entry_fk FOREIGN KEY (company_id, entry_id)
        REFERENCES financial_entries (company_id, id),
    CONSTRAINT financial_entry_installments_number_valid CHECK (number BETWEEN 1 AND 120),
    CONSTRAINT financial_entry_installments_due_date_valid CHECK (
        due_date BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT financial_entry_installments_amount_valid CHECK (
        amount = round(amount, 2) AND amount > 0 AND amount < 10000000000000
    )
);
