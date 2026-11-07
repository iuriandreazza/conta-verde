-- An entry can say a row of an import file made it (ADR 0064). The id is derived from the company and the
-- reference the file gave the row, so one reference is one entry per company and sending a file again
-- finds what it made. The origin stays at most one.
ALTER TABLE financial_entries ADD COLUMN import_id UUID;
ALTER TABLE financial_entries
    DROP CONSTRAINT financial_entries_one_origin,
    ADD CONSTRAINT financial_entries_one_origin CHECK (
        num_nonnulls(recurrence_id, sale_id, sale_return_id, charge_fee_id, purchase_receipt_id, import_id) <= 1
    );
CREATE UNIQUE INDEX financial_entries_import_key
    ON financial_entries (import_id) WHERE import_id IS NOT NULL;
