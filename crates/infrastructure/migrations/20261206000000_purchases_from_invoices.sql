-- A purchase entered from a supplier's NF-e (ADR 0057): the order says which invoice it came from, by its access key,
-- and a key makes at most one order, so entering the same file twice, or in two places, adds nothing. A receipt may
-- say when its payable falls due (an invoice gives its own installments) instead of the order's payment term.

ALTER TABLE purchase_orders ADD COLUMN invoice_key TEXT;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_invoice_key_valid CHECK (
    invoice_key IS NULL OR invoice_key ~ '^[0-9]{44}$'
);
CREATE UNIQUE INDEX purchase_orders_invoice_key_key
    ON purchase_orders (company_id, invoice_key) WHERE invoice_key IS NOT NULL;

CREATE TABLE purchase_receipt_installments (
    company_id UUID     NOT NULL,
    receipt_id UUID     NOT NULL,
    position   SMALLINT NOT NULL,
    due_date   DATE     NOT NULL,
    amount     NUMERIC  NOT NULL,
    PRIMARY KEY (receipt_id, position),
    CONSTRAINT purchase_receipt_installments_receipt_fk FOREIGN KEY (company_id, receipt_id)
        REFERENCES purchase_receipts (company_id, id),
    CONSTRAINT purchase_receipt_installments_position_valid CHECK (position BETWEEN 1 AND 120),
    CONSTRAINT purchase_receipt_installments_due_date_valid CHECK (
        due_date BETWEEN DATE '1900-01-01' AND DATE '2200-12-31'
    ),
    CONSTRAINT purchase_receipt_installments_amount_valid CHECK (
        amount > 0 AND amount = round(amount, 2) AND amount < 10000000000000
    )
);
