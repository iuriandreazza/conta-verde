-- The ledger of a company's stock (ADR 0054): one row per movement of a product, never edited or deleted. The
-- balance of a product is the sum of its rows and is never stored. The constraints repeat the rules of
-- `domain::StockMovement` that one row can show; the valuation of an exit, which needs the balance, is the
-- domain's, and what a product has is the adapter's to check with the stock locked.

CREATE TABLE stock_movements (
    id          UUID PRIMARY KEY,
    company_id  UUID        NOT NULL REFERENCES companies (id),
    product_id  UUID        NOT NULL,
    kind        TEXT        NOT NULL,
    -- Signed: what comes in is above 0, what goes out is below.
    quantity    NUMERIC     NOT NULL,
    -- Signed like the quantity, in cents.
    value       NUMERIC     NOT NULL,
    occurred_on DATE        NOT NULL,
    -- What made the movement, the document's id and the line of it, all or none.
    origin_kind TEXT,
    origin_id   UUID,
    origin_line SMALLINT,
    -- The movement this one undoes.
    reverses    UUID,
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT stock_movements_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT stock_movements_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT stock_movements_reverses_fk FOREIGN KEY (company_id, reverses)
        REFERENCES stock_movements (company_id, id),
    CONSTRAINT stock_movements_kind_valid CHECK (kind IN ('in', 'out', 'adjustment')),
    CONSTRAINT stock_movements_quantity_valid CHECK (
        quantity <> 0 AND quantity = round(quantity, 3) AND abs(quantity) < 10000000000000
    ),
    -- An `in` adds and an `out` takes; an adjustment goes either way.
    CONSTRAINT stock_movements_direction_valid CHECK (
        kind = 'adjustment' OR (kind = 'in' AND quantity > 0) OR (kind = 'out' AND quantity < 0)
    ),
    CONSTRAINT stock_movements_value_valid CHECK (
        value = round(value, 2) AND abs(value) < 10000000000000 AND (value = 0 OR sign(value) = sign(quantity))
    ),
    CONSTRAINT stock_movements_occurred_on_valid CHECK (
        occurred_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT stock_movements_origin_valid CHECK (
        (origin_kind IS NULL AND origin_id IS NULL AND origin_line IS NULL)
        OR (origin_kind IN ('purchase_receipt', 'sale', 'sale_return', 'inventory_count')
            AND origin_id IS NOT NULL AND origin_line BETWEEN 1 AND 100)
    ),
    CONSTRAINT stock_movements_note_valid CHECK (
        note IS NULL OR (char_length(note) BETWEEN 1 AND 200 AND note = btrim(note)
            AND note !~ '[[:cntrl:]]')
    )
);

-- An event makes each of its movements once, so making them twice, or in two places, is safe. A reversal carries
-- the origin of what it undoes and is left out.
CREATE UNIQUE INDEX stock_movements_origin_key
    ON stock_movements (company_id, origin_kind, origin_id, origin_line)
    WHERE origin_kind IS NOT NULL AND reverses IS NULL;
-- A movement is undone once.
CREATE UNIQUE INDEX stock_movements_reverses_key
    ON stock_movements (company_id, reverses) WHERE reverses IS NOT NULL;
CREATE INDEX stock_movements_product_idx
    ON stock_movements (company_id, product_id, occurred_on);
CREATE INDEX stock_movements_day_idx
    ON stock_movements (company_id, occurred_on DESC, id DESC);
