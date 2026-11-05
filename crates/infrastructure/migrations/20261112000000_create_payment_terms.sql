-- Payment terms of a company: a name and a list of installments, each due some days after the date
-- the term is applied to, for a share of the amount. The constraints repeat the rules of
-- `domain::PaymentTerm` that one row can show. The rules that span the rows of a term (the
-- percentages add up to exactly 100, the days increase with the position) are the domain's: the
-- database only refuses the same day twice.
CREATE TABLE payment_terms (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    name       TEXT        NOT NULL,
    active     BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Only the target of the installments' foreign key: an installment is in its term's company.
    CONSTRAINT payment_terms_company_id_id_key UNIQUE (company_id, id),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT payment_terms_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200 AND name = btrim(name))
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX payment_terms_company_name_key ON payment_terms (company_id, lower(name));

CREATE TABLE payment_term_installments (
    company_id UUID         NOT NULL,
    term_id    UUID         NOT NULL,
    -- The order of the installments, from 1.
    position   SMALLINT     NOT NULL,
    days       INTEGER      NOT NULL,
    -- Two decimals at most: the domain refuses more, it never rounds.
    percent    NUMERIC(5, 2) NOT NULL,
    PRIMARY KEY (term_id, position),
    CONSTRAINT payment_term_installments_term_fk FOREIGN KEY (company_id, term_id)
        REFERENCES payment_terms (company_id, id),
    CONSTRAINT payment_term_installments_one_per_day_key UNIQUE (term_id, days),
    CONSTRAINT payment_term_installments_position_valid CHECK (position BETWEEN 1 AND 24),
    CONSTRAINT payment_term_installments_days_valid CHECK (days BETWEEN 0 AND 3650),
    CONSTRAINT payment_term_installments_percent_valid CHECK (percent > 0 AND percent <= 100)
);
