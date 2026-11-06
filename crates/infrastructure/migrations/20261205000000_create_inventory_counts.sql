-- Inventory counts (ADR 0056): what was found of some products on a day, applied as adjustments of the stock. The
-- constraints repeat the rules of `domain::InventoryCount` that one row can show; that a count has between 1 and 500
-- products is the domain's.

ALTER TABLE document_counters DROP CONSTRAINT document_counters_kind_valid;
ALTER TABLE document_counters ADD CONSTRAINT document_counters_kind_valid CHECK (
    kind IN ('quote', 'sale', 'purchase_order', 'inventory_count')
);

CREATE TABLE inventory_counts (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    number     BIGINT      NOT NULL,
    counted_on DATE        NOT NULL,
    notes      TEXT,
    status     TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT inventory_counts_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT inventory_counts_company_number_key UNIQUE (company_id, number),
    CONSTRAINT inventory_counts_number_valid CHECK (number >= 1),
    CONSTRAINT inventory_counts_status_valid CHECK (status IN ('open', 'applied', 'cancelled')),
    CONSTRAINT inventory_counts_counted_on_valid CHECK (
        counted_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT inventory_counts_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 200 AND notes = btrim(notes)
            AND notes !~ '[[:cntrl:]]')
    )
);

CREATE INDEX inventory_counts_company_number_idx
    ON inventory_counts (company_id, number DESC, id DESC);

CREATE TABLE inventory_count_items (
    company_id UUID     NOT NULL,
    count_id   UUID     NOT NULL,
    position   SMALLINT NOT NULL,
    product_id UUID     NOT NULL,
    counted    NUMERIC  NOT NULL,
    PRIMARY KEY (count_id, position),
    -- A product is counted once in a count.
    CONSTRAINT inventory_count_items_product_key UNIQUE (count_id, product_id),
    CONSTRAINT inventory_count_items_count_fk FOREIGN KEY (company_id, count_id)
        REFERENCES inventory_counts (company_id, id),
    CONSTRAINT inventory_count_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT inventory_count_items_position_valid CHECK (position BETWEEN 1 AND 500),
    CONSTRAINT inventory_count_items_counted_valid CHECK (
        counted >= 0 AND counted = round(counted, 3) AND counted < 10000000000000
    )
);
