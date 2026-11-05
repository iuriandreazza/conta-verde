-- Products and services of a company. The CHECK constraints repeat the rules of `domain::Product`,
-- so a bug in the application cannot store what the domain would have refused.
CREATE TABLE products (
    id            UUID PRIMARY KEY,
    company_id    UUID        NOT NULL REFERENCES companies (id),
    kind          TEXT        NOT NULL,
    name          TEXT        NOT NULL,
    -- Stored in upper case by the domain, so uniqueness needs no function.
    sku           TEXT,
    unit          TEXT        NOT NULL,
    ncm           TEXT,
    -- Unit prices and quantities (ADR 0003).
    sale_price    NUMERIC(19, 6),
    cost_price    NUMERIC(19, 6),
    minimum_stock NUMERIC(19, 6),
    active        BOOLEAN     NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT products_kind_valid CHECK (kind IN ('product', 'service')),
    CONSTRAINT products_ncm_has_eight_digits CHECK (ncm IS NULL OR ncm ~ '^[0-9]{8}$'),
    CONSTRAINT products_amounts_not_negative CHECK (
        COALESCE(sale_price, 0) >= 0 AND COALESCE(cost_price, 0) >= 0 AND COALESCE(minimum_stock, 0) >= 0
    ),
    CONSTRAINT products_service_has_no_stock_data CHECK (
        kind = 'product' OR (ncm IS NULL AND minimum_stock IS NULL)
    )
);

CREATE UNIQUE INDEX products_company_sku_key
    ON products (company_id, sku)
    WHERE sku IS NOT NULL;

-- Serves the order of every list (ADR 0018), ascending and descending.
CREATE INDEX products_company_name_idx ON products (company_id, lower(name), id);
