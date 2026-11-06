-- Recurring entries (ADR 0033). The constraints repeat the rules of `domain::Recurrence` that one row can
-- show; that what it points at is usable now is the use case's, and so is that an end day is not before the
-- first one across a replacement (here it is one row, so it is also a CHECK).

CREATE TABLE recurrences (
    id                UUID PRIMARY KEY,
    company_id        UUID        NOT NULL REFERENCES companies (id),
    kind              TEXT        NOT NULL,
    description       TEXT        NOT NULL,
    party_id          UUID,
    category_id       UUID        NOT NULL,
    cost_center_id    UUID,
    -- Unconstrained scale on purpose: more than two decimals is refused, never rounded.
    amount            NUMERIC     NOT NULL,
    frequency         TEXT        NOT NULL,
    starts_on         DATE        NOT NULL,
    ends_on           DATE,
    active            BOOLEAN     NOT NULL DEFAULT true,
    -- Bookkeeping, not part of the register: the day the occurrences were generated through.
    generated_through DATE,
    -- Whoever last saved it: the entries it generates are made in their name.
    saved_by_user_id  UUID        NOT NULL,
    saved_by_name     TEXT        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The target of the foreign key of the entries it generates.
    CONSTRAINT recurrences_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT recurrences_category_fk FOREIGN KEY (company_id, category_id)
        REFERENCES categories (company_id, id),
    CONSTRAINT recurrences_cost_center_fk FOREIGN KEY (company_id, cost_center_id)
        REFERENCES cost_centers (company_id, id),
    CONSTRAINT recurrences_party_fk FOREIGN KEY (company_id, party_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT recurrences_kind_valid CHECK (kind IN ('receivable', 'payable')),
    CONSTRAINT recurrences_frequency_valid CHECK (frequency IN ('weekly', 'monthly', 'yearly')),
    CONSTRAINT recurrences_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT recurrences_amount_valid CHECK (
        amount = round(amount, 2) AND amount > 0 AND amount < 10000000000000
    ),
    CONSTRAINT recurrences_starts_on_valid CHECK (
        starts_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT recurrences_ends_on_valid CHECK (
        ends_on IS NULL OR (ends_on >= starts_on AND ends_on <= DATE '2200-12-31')
    ),
    CONSTRAINT recurrences_generated_through_valid CHECK (
        generated_through IS NULL OR generated_through BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    )
);

-- The list is by description, then id (ADR 0018), and the job reads the active ones that are not done.
CREATE INDEX recurrences_company_description_idx ON recurrences (company_id, lower(description), id);
CREATE INDEX recurrences_active_idx ON recurrences (generated_through) WHERE active;

-- Where a generated entry came from; an entry that was typed has neither.
ALTER TABLE financial_entries
    ADD COLUMN recurrence_id UUID,
    ADD COLUMN occurrence_on DATE;

ALTER TABLE financial_entries
    ADD CONSTRAINT financial_entries_origin_fk FOREIGN KEY (company_id, recurrence_id)
        REFERENCES recurrences (company_id, id),
    ADD CONSTRAINT financial_entries_origin_whole CHECK (
        (recurrence_id IS NULL) = (occurrence_on IS NULL)
    ),
    ADD CONSTRAINT financial_entries_occurrence_valid CHECK (
        occurrence_on IS NULL OR occurrence_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    );

-- A recurrence has at most one entry per occurrence day: what makes generating twice, or in two places at
-- once, safe.
CREATE UNIQUE INDEX financial_entries_occurrence_key
    ON financial_entries (recurrence_id, occurrence_on) WHERE recurrence_id IS NOT NULL;
