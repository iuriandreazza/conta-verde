-- Quotes (ADR 0038) and the numbering shared by quotes and sales (ADR 0037). The constraints repeat the
-- rules of `domain::Quote` and `domain::Line` that one row can show; that a quote has between 1 and 100
-- lines, and that its total fits, span rows and are the domain's.

ALTER TABLE products ADD CONSTRAINT products_company_id_id_key UNIQUE (company_id, id);

-- The last number given to a kind of document of a company: the next one is taken inside the write that
-- uses it, so two writers never get the same.
CREATE TABLE document_counters (
    company_id UUID   NOT NULL REFERENCES companies (id),
    kind       TEXT   NOT NULL,
    last       BIGINT NOT NULL,
    PRIMARY KEY (company_id, kind),
    CONSTRAINT document_counters_kind_valid CHECK (kind IN ('quote', 'sale')),
    CONSTRAINT document_counters_last_valid CHECK (last >= 1)
);

CREATE TABLE quotes (
    id               UUID PRIMARY KEY,
    company_id       UUID        NOT NULL REFERENCES companies (id),
    number           BIGINT      NOT NULL,
    customer_id      UUID        NOT NULL,
    issued_on        DATE        NOT NULL,
    valid_until      DATE        NOT NULL,
    payment_term_id  UUID,
    notes            TEXT,
    status           TEXT        NOT NULL,
    -- SHA-256 of the token in the link the customer answers through; the token itself is never kept.
    link_digest      TEXT,
    -- Whoever sent it last: the customer's answer is recorded in their name.
    sent_by_user_id  UUID,
    sent_by_name     TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT quotes_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT quotes_company_number_key UNIQUE (company_id, number),
    CONSTRAINT quotes_customer_fk FOREIGN KEY (company_id, customer_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT quotes_payment_term_fk FOREIGN KEY (company_id, payment_term_id)
        REFERENCES payment_terms (company_id, id),
    CONSTRAINT quotes_number_valid CHECK (number >= 1),
    CONSTRAINT quotes_status_valid CHECK (
        status IN ('draft', 'sent', 'approved', 'declined', 'converted')
    ),
    CONSTRAINT quotes_dates_valid CHECK (
        issued_on BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
        AND valid_until >= issued_on AND valid_until <= DATE '2200-12-31'
    ),
    CONSTRAINT quotes_notes_valid CHECK (
        notes IS NULL OR (char_length(notes) BETWEEN 1 AND 2000 AND notes = btrim(notes))
    ),
    -- A link exists only while the quote waits for an answer or has been answered through it.
    CONSTRAINT quotes_link_valid CHECK (
        link_digest IS NULL OR (status IN ('sent', 'approved', 'declined') AND sent_by_user_id IS NOT NULL)
    ),
    CONSTRAINT quotes_sent_by_whole CHECK ((sent_by_user_id IS NULL) = (sent_by_name IS NULL))
);

CREATE UNIQUE INDEX quotes_link_digest_key ON quotes (link_digest) WHERE link_digest IS NOT NULL;
CREATE INDEX quotes_company_number_idx ON quotes (company_id, number DESC, id DESC);

CREATE TABLE quote_items (
    company_id  UUID     NOT NULL,
    quote_id    UUID     NOT NULL,
    position    SMALLINT NOT NULL,
    product_id  UUID     NOT NULL,
    description TEXT     NOT NULL,
    -- Unconstrained scale on purpose: more decimals than the rules allow are refused, never rounded.
    quantity    NUMERIC  NOT NULL,
    unit_price  NUMERIC  NOT NULL,
    discount    NUMERIC  NOT NULL,
    PRIMARY KEY (quote_id, position),
    CONSTRAINT quote_items_quote_fk FOREIGN KEY (company_id, quote_id)
        REFERENCES quotes (company_id, id),
    CONSTRAINT quote_items_product_fk FOREIGN KEY (company_id, product_id)
        REFERENCES products (company_id, id),
    CONSTRAINT quote_items_position_valid CHECK (position BETWEEN 1 AND 100),
    CONSTRAINT quote_items_description_valid CHECK (
        char_length(description) BETWEEN 1 AND 200
        AND description = btrim(description)
        AND description !~ '[[:cntrl:]]'
    ),
    CONSTRAINT quote_items_quantity_valid CHECK (
        quantity > 0 AND quantity = round(quantity, 3) AND quantity < 10000000000000
    ),
    CONSTRAINT quote_items_unit_price_valid CHECK (
        unit_price >= 0 AND unit_price = round(unit_price, 6) AND unit_price < 10000000000000
    ),
    -- The rounding rule of ADR 0037: the gross is rounded to the cent, halves away from zero, and a
    -- discount is in cents and at most the gross.
    CONSTRAINT quote_items_discount_valid CHECK (
        discount >= 0 AND discount = round(discount, 2)
        AND discount <= round(quantity * unit_price, 2)
    )
);
