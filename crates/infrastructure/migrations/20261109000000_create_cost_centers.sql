-- Cost centers of a company: where a cost is incurred, to read results per store, project or
-- department. The CHECK repeats the rule of `domain::CostCenter`.
CREATE TABLE cost_centers (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    name       TEXT        NOT NULL,
    active     BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT cost_centers_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200)
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX cost_centers_company_name_key ON cost_centers (company_id, lower(name));
