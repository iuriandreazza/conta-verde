-- Commission rules (ADR 0041). The constraints repeat the rules of `domain::CommissionRule` that one row can show;
-- which rule applies to a sale is the domain's, and so is that what it names is usable now (the use case's).

CREATE TABLE commission_rules (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    -- Null is every seller and every product.
    seller_id  UUID,
    product_id UUID,
    -- Unconstrained scale on purpose: more than two decimals is refused, never rounded.
    rate       NUMERIC     NOT NULL,
    starts_on  DATE        NOT NULL,
    ends_on    DATE,
    active     BOOLEAN     NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT commission_rules_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT commission_rules_seller_fk FOREIGN KEY (company_id, seller_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT commission_rules_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT commission_rules_rate_valid CHECK (rate > 0 AND rate <= 100 AND rate = round(rate, 2)),
    CONSTRAINT commission_rules_starts_on_valid CHECK (
        starts_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT commission_rules_ends_on_valid CHECK (
        ends_on IS NULL OR (ends_on >= starts_on AND ends_on <= DATE '2200-12-31')
    )
);

CREATE INDEX commission_rules_company_idx ON commission_rules (company_id, starts_on DESC, id DESC);

-- A statement reads the completed sales of a span.
CREATE INDEX sales_company_sold_on_idx ON sales (company_id, sold_on);
