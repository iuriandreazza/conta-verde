-- Sales (ADR 0039), their returns, and what they change in entries (ADR 0037): an entry can be cancelled
-- ("voided") and can say it was made by a sale or by a return. The constraints repeat the rules of
-- `domain::Sale` and `domain::SaleReturn` that one row can show; that a sale has between 1 and 100 lines, that
-- a return takes back no more than was sold, and that a total fits, span rows and are the domain's (the
-- second also the adapter's, with the sale locked).

-- A quote that became a sale keeps its link, so the customer who opens it again sees it approved.
ALTER TABLE quotes DROP CONSTRAINT quotes_link_valid;
ALTER TABLE quotes ADD CONSTRAINT quotes_link_valid CHECK (
    link_digest IS NULL
    OR (status IN ('sent', 'approved', 'declined', 'converted') AND sent_by_user_id IS NOT NULL)
);

CREATE TABLE sales (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    number          BIGINT      NOT NULL,
    customer_id     UUID        NOT NULL,
    seller_id       UUID,
    sold_on         DATE        NOT NULL,
    payment_term_id UUID,
    -- The category of the receivable the sale makes.
    category_id     UUID        NOT NULL,
    cost_center_id  UUID,
    notes           TEXT,
    status          TEXT        NOT NULL,
    quote_id        UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT sales_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT sales_company_number_key UNIQUE (company_id, number),
    CONSTRAINT sales_customer_fk FOREIGN KEY (company_id, customer_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT sales_seller_fk FOREIGN KEY (company_id, seller_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT sales_payment_term_fk FOREIGN KEY (company_id, payment_term_id)
        REFERENCES payment_terms (company_id, id),
    CONSTRAINT sales_category_fk FOREIGN KEY (company_id, category_id)
        REFERENCES categories (company_id, id),
    CONSTRAINT sales_cost_center_fk FOREIGN KEY (company_id, cost_center_id)
        REFERENCES cost_centers (company_id, id),
    CONSTRAINT sales_quote_fk FOREIGN KEY (company_id, quote_id)
        REFERENCES quotes (company_id, id),
    CONSTRAINT sales_number_valid CHECK (number >= 1),
    CONSTRAINT sales_status_valid CHECK (status IN ('open', 'completed', 'cancelled')),
    CONSTRAINT sales_sold_on_valid CHECK (sold_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'),
    CONSTRAINT sales_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 2000 AND notes = btrim(notes))
    )
);

-- A quote makes at most one sale: what makes converting twice, or in two places, safe.
CREATE UNIQUE INDEX sales_quote_key ON sales (quote_id) WHERE quote_id IS NOT NULL;
CREATE INDEX sales_company_number_idx ON sales (company_id, number DESC, id DESC);

CREATE TABLE sale_items (
    company_id  UUID     NOT NULL,
    sale_id     UUID     NOT NULL,
    position    SMALLINT NOT NULL,
    product_id  UUID     NOT NULL,
    description TEXT     NOT NULL,
    quantity    NUMERIC  NOT NULL,
    unit_price  NUMERIC  NOT NULL,
    discount    NUMERIC  NOT NULL,
    PRIMARY KEY (sale_id, position),
    CONSTRAINT sale_items_sale_fk FOREIGN KEY (company_id, sale_id)
        REFERENCES sales (company_id, id),
    CONSTRAINT sale_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT sale_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT sale_items_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT sale_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    ),
    CONSTRAINT sale_items_unit_price_valid CHECK (
        unit_price >= 0 AND unit_price = round(unit_price, 6) AND unit_price < 10000000000000
    ),
    CONSTRAINT sale_items_discount_valid CHECK (
        discount >= 0 AND discount = round(discount, 2)
        AND discount <= round(quantity * unit_price, 2)
    )
);

CREATE TABLE sale_returns (
    id          UUID PRIMARY KEY,
    company_id  UUID        NOT NULL REFERENCES companies (id),
    sale_id     UUID        NOT NULL,
    returned_on DATE        NOT NULL,
    reason      TEXT,
    -- What was refunded is a fact of the day of the return, worked out then from what had been returned before.
    refund      NUMERIC     NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT sale_returns_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT sale_returns_sale_fk FOREIGN KEY (company_id, sale_id)
        REFERENCES sales (company_id, id),
    CONSTRAINT sale_returns_returned_on_valid CHECK (
        returned_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT sale_returns_reason_valid CHECK (
        reason IS NULL OR (char_length(reason) BETWEEN 1 AND 200
            AND reason = btrim(reason) AND reason !~ '[[:cntrl:]]')
    ),
    CONSTRAINT sale_returns_refund_valid CHECK (
        refund = round(refund, 2) AND refund > 0 AND refund < 10000000000000
    )
);

CREATE INDEX sale_returns_sale_idx ON sale_returns (company_id, sale_id, created_at, id);

CREATE TABLE sale_return_items (
    company_id UUID     NOT NULL,
    return_id  UUID     NOT NULL,
    position   SMALLINT NOT NULL,
    -- The place of the line in the sale, from 1.
    line       SMALLINT NOT NULL,
    quantity   NUMERIC  NOT NULL,
    PRIMARY KEY (return_id, position),
    -- A line comes back once in a return.
    CONSTRAINT sale_return_items_line_key UNIQUE (return_id, line),
    CONSTRAINT sale_return_items_return_fk FOREIGN KEY (company_id, return_id)
        REFERENCES sale_returns (company_id, id),
    CONSTRAINT sale_return_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT sale_return_items_line_valid CHECK (line BETWEEN 1 AND 100),
    CONSTRAINT sale_return_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    )
);

-- An entry can be cancelled, once and for good, and can say a sale or a return made it.
ALTER TABLE financial_entries
    ADD COLUMN voided BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN sale_id UUID,
    ADD COLUMN sale_return_id UUID;

ALTER TABLE financial_entries
    ADD CONSTRAINT financial_entries_sale_fk FOREIGN KEY (company_id, sale_id)
        REFERENCES sales (company_id, id),
    ADD CONSTRAINT financial_entries_sale_return_fk FOREIGN KEY (company_id, sale_return_id)
        REFERENCES sale_returns (company_id, id),
    ADD CONSTRAINT financial_entries_one_origin CHECK (
        num_nonnulls(recurrence_id, sale_id, sale_return_id) <= 1
    );

-- A sale has at most one receivable and a return at most one refund.
CREATE UNIQUE INDEX financial_entries_sale_key
    ON financial_entries (sale_id) WHERE sale_id IS NOT NULL;
CREATE UNIQUE INDEX financial_entries_sale_return_key
    ON financial_entries (sale_return_id) WHERE sale_return_id IS NOT NULL;
