-- Purchase orders (ADR 0053) and their receipts, and what a receipt makes in entries: a payable that says it was
-- made by the receipt. The constraints repeat the rules of `domain::PurchaseOrder` and `domain::PurchaseReceipt`
-- that one row can show; that an order has between 1 and 100 lines, that a receipt takes in no more than was
-- ordered, and that a total fits, span rows and are the domain's (the second also the adapter's, with the order
-- locked).

ALTER TABLE document_counters DROP CONSTRAINT document_counters_kind_valid;
ALTER TABLE document_counters ADD CONSTRAINT document_counters_kind_valid CHECK (
    kind IN ('quote', 'sale', 'purchase_order')
);

CREATE TABLE purchase_orders (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    number          BIGINT      NOT NULL,
    supplier_id     UUID        NOT NULL,
    ordered_on      DATE        NOT NULL,
    expected_on     DATE,
    payment_term_id UUID,
    -- The category of the payables the receipts make.
    category_id     UUID        NOT NULL,
    cost_center_id  UUID,
    notes           TEXT,
    status          TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT purchase_orders_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT purchase_orders_company_number_key UNIQUE (company_id, number),
    CONSTRAINT purchase_orders_supplier_fk FOREIGN KEY (company_id, supplier_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT purchase_orders_payment_term_fk FOREIGN KEY (company_id, payment_term_id)
        REFERENCES payment_terms (company_id, id),
    CONSTRAINT purchase_orders_category_fk FOREIGN KEY (company_id, category_id)
        REFERENCES categories (company_id, id),
    CONSTRAINT purchase_orders_cost_center_fk FOREIGN KEY (company_id, cost_center_id)
        REFERENCES cost_centers (company_id, id),
    CONSTRAINT purchase_orders_number_valid CHECK (number >= 1),
    CONSTRAINT purchase_orders_status_valid CHECK (
        status IN ('draft', 'approved', 'partially_received', 'received', 'cancelled')
    ),
    CONSTRAINT purchase_orders_ordered_on_valid CHECK (
        ordered_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT purchase_orders_expected_on_valid CHECK (
        expected_on IS NULL OR (expected_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
            AND expected_on >= ordered_on)
    ),
    CONSTRAINT purchase_orders_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 2000 AND notes = btrim(notes))
    )
);

CREATE INDEX purchase_orders_company_number_idx
    ON purchase_orders (company_id, number DESC, id DESC);

CREATE TABLE purchase_order_items (
    company_id  UUID     NOT NULL,
    purchase_id UUID     NOT NULL,
    position    SMALLINT NOT NULL,
    product_id  UUID     NOT NULL,
    description TEXT     NOT NULL,
    quantity    NUMERIC  NOT NULL,
    unit_price  NUMERIC  NOT NULL,
    discount    NUMERIC  NOT NULL,
    PRIMARY KEY (purchase_id, position),
    CONSTRAINT purchase_order_items_purchase_fk FOREIGN KEY (company_id, purchase_id)
        REFERENCES purchase_orders (company_id, id),
    CONSTRAINT purchase_order_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT purchase_order_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT purchase_order_items_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT purchase_order_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    ),
    CONSTRAINT purchase_order_items_unit_price_valid CHECK (
        unit_price >= 0 AND unit_price = round(unit_price, 6) AND unit_price < 10000000000000
    ),
    CONSTRAINT purchase_order_items_discount_valid CHECK (
        discount >= 0 AND discount = round(discount, 2)
        AND discount <= round(quantity * unit_price, 2)
    )
);

CREATE TABLE purchase_receipts (
    id          UUID PRIMARY KEY,
    company_id  UUID        NOT NULL REFERENCES companies (id),
    purchase_id UUID        NOT NULL,
    received_on DATE        NOT NULL,
    notes       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT purchase_receipts_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT purchase_receipts_purchase_fk FOREIGN KEY (company_id, purchase_id)
        REFERENCES purchase_orders (company_id, id),
    CONSTRAINT purchase_receipts_received_on_valid CHECK (
        received_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT purchase_receipts_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 200
            AND notes = btrim(notes) AND notes !~ '[[:cntrl:]]')
    )
);

CREATE INDEX purchase_receipts_purchase_idx
    ON purchase_receipts (company_id, purchase_id, created_at, id);

CREATE TABLE purchase_receipt_items (
    company_id UUID     NOT NULL,
    receipt_id UUID     NOT NULL,
    position   SMALLINT NOT NULL,
    -- The place of the line in the order, from 1.
    line       SMALLINT NOT NULL,
    quantity   NUMERIC  NOT NULL,
    -- What the quantity cost, a fact of the day of the receipt, worked out then from what had arrived before.
    value      NUMERIC  NOT NULL,
    PRIMARY KEY (receipt_id, position),
    -- A line arrives once in a receipt.
    CONSTRAINT purchase_receipt_items_line_key UNIQUE (receipt_id, line),
    CONSTRAINT purchase_receipt_items_receipt_fk FOREIGN KEY (company_id, receipt_id)
        REFERENCES purchase_receipts (company_id, id),
    CONSTRAINT purchase_receipt_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT purchase_receipt_items_line_valid CHECK (line BETWEEN 1 AND 100),
    CONSTRAINT purchase_receipt_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    ),
    CONSTRAINT purchase_receipt_items_value_valid CHECK (
        value >= 0 AND value = round(value, 2) AND value < 10000000000000
    )
);

-- An entry can say a receipt made it; a receipt makes at most one payable. The origin stays at most one.
ALTER TABLE financial_entries ADD COLUMN purchase_receipt_id UUID;
ALTER TABLE financial_entries
    DROP CONSTRAINT financial_entries_one_origin,
    ADD CONSTRAINT financial_entries_purchase_receipt_fk FOREIGN KEY (company_id, purchase_receipt_id)
        REFERENCES purchase_receipts (company_id, id),
    ADD CONSTRAINT financial_entries_one_origin CHECK (
        num_nonnulls(recurrence_id, sale_id, sale_return_id, charge_fee_id, purchase_receipt_id) <= 1
    );
CREATE UNIQUE INDEX financial_entries_purchase_receipt_key
    ON financial_entries (purchase_receipt_id) WHERE purchase_receipt_id IS NOT NULL;
