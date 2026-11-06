-- How a company's stock behaves (ADR 0055): one row per company, replaced as a whole.
CREATE TABLE stock_settings (
    company_id                 UUID PRIMARY KEY REFERENCES companies (id),
    -- Whether a sale that the stock cannot cover is refused.
    block_sale_without_balance BOOLEAN NOT NULL
);
