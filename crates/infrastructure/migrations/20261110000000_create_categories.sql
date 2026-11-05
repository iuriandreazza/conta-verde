-- The tree of categories of a company (ADR 0012). The constraints repeat the rules of
-- `domain::Category`, so a bug in the application cannot store what the domain would refuse.
CREATE TABLE categories (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    parent_id       UUID,
    name            TEXT        NOT NULL,
    -- Chosen by a root and copied to every descendant, so a report reads a column.
    dre_group       TEXT        NOT NULL,
    cash_flow_group TEXT        NOT NULL,
    level           SMALLINT    NOT NULL,
    active          BOOLEAN     NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT categories_company_id_id_key UNIQUE (company_id, id),
    -- The parent is in the same company; a root has no parent and so is not checked.
    CONSTRAINT categories_parent_fk FOREIGN KEY (company_id, parent_id)
        REFERENCES categories (company_id, id),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT categories_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT categories_dre_group_valid CHECK (dre_group IN (
        'gross_revenue', 'deductions', 'costs', 'operating_expenses', 'financial_result',
        'investments', 'outside_result', 'transfers'
    )),
    CONSTRAINT categories_cash_flow_group_valid CHECK (cash_flow_group IN (
        'operating', 'investing', 'financing', 'transfer'
    )),
    CONSTRAINT categories_transfer_groups_go_together CHECK (
        (dre_group = 'transfers') = (cash_flow_group = 'transfer')
    ),
    CONSTRAINT categories_level_matches_parent CHECK (
        level BETWEEN 1 AND 5 AND ((level = 1) = (parent_id IS NULL))
    ),
    -- The category for moves between accounts is a root, has no subcategories and stays active.
    CONSTRAINT categories_transfer_is_a_root CHECK (dre_group <> 'transfers' OR parent_id IS NULL),
    CONSTRAINT categories_transfer_stays_active CHECK (dre_group <> 'transfers' OR active)
);

-- Names are unique among siblings, ignoring case. NULLS NOT DISTINCT makes the roots (no parent)
-- siblings of each other, which a plain unique index would not.
CREATE UNIQUE INDEX categories_siblings_name_key
    ON categories (company_id, parent_id, lower(name)) NULLS NOT DISTINCT;

-- At most one category for transfers per company: Phase 2 finds it by its group, not its name.
CREATE UNIQUE INDEX categories_one_transfer_key
    ON categories (company_id)
    WHERE dre_group = 'transfers';

CREATE INDEX categories_company_name_idx ON categories (company_id, lower(name), id);
