-- The links that lead a customer to the payment page of a charge (ADR 0047). Only the digest of the
-- token is kept, so a leaked database gives no usable link. A link is a capability and not a register,
-- so it is not audited; a charge may have several (each reminder makes its own).

CREATE TABLE payment_links (
    digest     TEXT PRIMARY KEY,
    company_id UUID        NOT NULL,
    charge_id  UUID        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT payment_links_charge_fk FOREIGN KEY (company_id, charge_id)
        REFERENCES charges (company_id, id),
    CONSTRAINT payment_links_digest_valid CHECK (char_length(digest) BETWEEN 16 AND 200)
);

CREATE INDEX payment_links_charge_idx ON payment_links (company_id, charge_id);
