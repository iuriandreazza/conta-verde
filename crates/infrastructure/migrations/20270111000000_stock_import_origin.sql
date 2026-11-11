-- An opening balance given by a file is a stock movement with its own origin (docs/adr/0094-stock-balance-import.md): the
-- origin is the file's reference and the product, so sending the same file again finds it done.
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_origin_valid;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_origin_valid CHECK (
    (origin_kind IS NULL AND origin_id IS NULL AND origin_line IS NULL)
    OR (origin_kind IN ('purchase_receipt', 'sale', 'sale_return', 'inventory_count', 'import')
        AND origin_id IS NOT NULL AND origin_line BETWEEN 1 AND 100)
);
