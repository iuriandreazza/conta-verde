-- Recurring contracts (ADR 0040) and the sales they make, one per cycle day. The constraints repeat the rules of
-- `domain::Contract` that one row can show; that a contract has between 1 and 100 lines, that its cycles are
-- counted from its first day and that what it points at is usable are the domain's and the use case's.

CREATE TABLE contracts (
    id                UUID PRIMARY KEY,
    company_id        UUID        NOT NULL REFERENCES companies (id),
    name              TEXT        NOT NULL,
    customer_id       UUID        NOT NULL,
    seller_id         UUID,
    payment_term_id   UUID,
    category_id       UUID        NOT NULL,
    cost_center_id    UUID,
    notes             TEXT,
    cycle             TEXT        NOT NULL,
    starts_on         DATE        NOT NULL,
    ends_on           DATE,
    status            TEXT        NOT NULL,
    -- Bookkeeping, not part of the register: the day the cycles were billed through.
    billed_through    DATE,
    -- Whoever last saved it: the sales it makes are made in their name.
    saved_by_user_id  UUID        NOT NULL,
    saved_by_name     TEXT        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT contracts_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT contracts_customer_fk FOREIGN KEY (company_id, customer_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT contracts_seller_fk FOREIGN KEY (company_id, seller_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT contracts_payment_term_fk FOREIGN KEY (company_id, payment_term_id)
        REFERENCES payment_terms (company_id, id),
    CONSTRAINT contracts_category_fk FOREIGN KEY (company_id, category_id)
        REFERENCES categories (company_id, id),
    CONSTRAINT contracts_cost_center_fk FOREIGN KEY (company_id, cost_center_id)
        REFERENCES cost_centers (company_id, id),
    CONSTRAINT contracts_name_valid CHECK (
        char_length(name) BETWEEN 1 AND 200 AND name = btrim(name) AND name !~ '[[:cntrl:]]'
    ),
    CONSTRAINT contracts_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 2000 AND notes = btrim(notes))
    ),
    CONSTRAINT contracts_cycle_valid CHECK (
        cycle IN ('monthly', 'bimonthly', 'quarterly', 'semiannual', 'yearly')
    ),
    CONSTRAINT contracts_status_valid CHECK (status IN ('active', 'paused', 'terminated')),
    CONSTRAINT contracts_starts_on_valid CHECK (
        starts_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT contracts_ends_on_valid CHECK (
        ends_on IS NULL OR (ends_on >= starts_on AND ends_on <= DATE '2200-12-31')
    ),
    CONSTRAINT contracts_billed_through_valid CHECK (
        billed_through IS NULL OR billed_through BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    -- A terminated contract has an end.
    CONSTRAINT contracts_terminated_ends CHECK (status <> 'terminated' OR ends_on IS NOT NULL)
);

CREATE INDEX contracts_company_name_idx ON contracts (company_id, lower(name), id);
CREATE INDEX contracts_to_bill_idx ON contracts (billed_through) WHERE status = 'active';

CREATE TABLE contract_items (
    company_id  UUID     NOT NULL,
    contract_id UUID     NOT NULL,
    position    SMALLINT NOT NULL,
    product_id  UUID     NOT NULL,
    description TEXT     NOT NULL,
    quantity    NUMERIC  NOT NULL,
    unit_price  NUMERIC  NOT NULL,
    discount    NUMERIC  NOT NULL,
    PRIMARY KEY (contract_id, position),
    CONSTRAINT contract_items_contract_fk FOREIGN KEY (company_id, contract_id)
        REFERENCES contracts (company_id, id),
    CONSTRAINT contract_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT contract_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT contract_items_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT contract_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    ),
    CONSTRAINT contract_items_unit_price_valid CHECK (
        unit_price >= 0 AND unit_price = round(unit_price, 6) AND unit_price < 10000000000000
    ),
    CONSTRAINT contract_items_discount_valid CHECK (
        discount >= 0 AND discount = round(discount, 2)
        AND discount <= round(quantity * unit_price, 2)
    )
);

-- The cycle of a contract a sale bills; a sale that was typed has neither.
ALTER TABLE sales
    ADD COLUMN contract_id UUID,
    ADD COLUMN cycle_on DATE;

ALTER TABLE sales
    ADD CONSTRAINT sales_contract_fk FOREIGN KEY (company_id, contract_id)
        REFERENCES contracts (company_id, id),
    ADD CONSTRAINT sales_cycle_whole CHECK ((contract_id IS NULL) = (cycle_on IS NULL)),
    ADD CONSTRAINT sales_one_origin CHECK (quote_id IS NULL OR contract_id IS NULL),
    ADD CONSTRAINT sales_cycle_on_valid CHECK (
        cycle_on IS NULL OR cycle_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    );

-- A contract has at most one sale per cycle day: what makes billing twice, or in two places, safe.
CREATE UNIQUE INDEX sales_contract_cycle_key
    ON sales (contract_id, cycle_on) WHERE contract_id IS NOT NULL;
