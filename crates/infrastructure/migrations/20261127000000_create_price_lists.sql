-- Price lists of a company (ADR 0042): a name and the unit price of each product the list names. The constraints
-- repeat the rules of `domain::PriceList` that one row can show. That a list has at least one price and at most
-- 1000, and that what its products are is usable when a price is put in, are the domain's and the use case's.
CREATE TABLE price_lists (
    id         UUID PRIMARY KEY,
    company_id UUID        NOT NULL REFERENCES companies (id),
    name       TEXT        NOT NULL,
    active     BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Only the target of the items' foreign key: an item is in its list's company.
    CONSTRAINT price_lists_company_id_id_key UNIQUE (company_id, id),
    -- The name is in a btree index, which cannot hold a row of more than about 2.7 KB.
    CONSTRAINT price_lists_name_is_short CHECK (char_length(name) BETWEEN 1 AND 200 AND name = btrim(name))
);

-- One name per company, ignoring case; it also serves the order of the list (ADR 0018).
CREATE UNIQUE INDEX price_lists_company_name_key ON price_lists (company_id, lower(name));

CREATE TABLE price_list_items (
    company_id UUID    NOT NULL,
    list_id    UUID    NOT NULL,
    -- The order of the prices as they were written, from 1.
    position   SMALLINT NOT NULL,
    product_id UUID    NOT NULL,
    -- Unconstrained scale on purpose, like every unit price (ADR 0003): more than six decimals is refused, never
    -- rounded (the check below enforces it), and at most thirteen digits before the point.
    price      NUMERIC NOT NULL,
    PRIMARY KEY (list_id, position),
    CONSTRAINT price_list_items_list_fk FOREIGN KEY (company_id, list_id)
        REFERENCES price_lists (company_id, id),
    CONSTRAINT price_list_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT price_list_items_one_per_product_key UNIQUE (list_id, product_id),
    CONSTRAINT price_list_items_position_valid CHECK (position BETWEEN 1 AND 1000),
    CONSTRAINT price_list_items_price_valid CHECK (
        price >= 0 AND price < 10000000000000 AND price = round(price, 6)
    )
);
