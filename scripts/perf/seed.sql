-- A deterministic five-year dataset for the performance baseline (ADR 0097, docs/performance/baseline.md).
--
--   psql -v user_id=<uuid of a registered user> -v scale=1 -v accounts=8 -f scripts/perf/seed.sql <owner URL>
--
-- It makes one LARGE company that the user owns, and a few small ones (so the planner sees other tenants and the policies have
-- something to hide), directly in SQL as the schema's owner: nothing here goes through the application, which is the point (a
-- five-year history cannot be made through the API in a reasonable time). The same input always makes the same rows: every id is
-- an md5 of what the row is, and nothing is random. `scale` multiplies the sizes of the large company (1 = the table below; 0.01 is a quick trial).
--
--   per large company, scale 1:  3,000 parties, 800 products, 120,000 entries (1 to 3 installments, about 250,000), 190,000
--   settlements, 70,000 sales with 3 lines each, 240,000 stock movements, 1,000,000 audit rows, `accounts` bank accounts (8 by default; the cash flow does its work once for each), 5 years of days.
\set ON_ERROR_STOP on
\if :{?accounts}
\else
    \set accounts 8
\endif
\timing off

BEGIN;

CREATE TEMP TABLE plan (slug text, name text, entries int, parties int, products int, sales int, moves int, audits int, accts int);
INSERT INTO plan VALUES
    ('large', 'Large company', (120000 * :scale)::int, 3000, 800, (70000 * :scale)::int, (240000 * :scale)::int, (1000000 * :scale)::int, :accounts),
    ('small1', 'Small company 1', 2000, 60, 30, 800, 2000, 8000, 3),
    ('small2', 'Small company 2', 2000, 60, 30, 800, 2000, 8000, 3),
    ('small3', 'Small company 3', 2000, 60, 30, 800, 2000, 8000, 3);

CREATE TEMP TABLE co AS SELECT md5('company-' || slug)::uuid AS id, p.* FROM plan p;

INSERT INTO companies (id, name) SELECT id, name FROM co;
INSERT INTO memberships (user_id, company_id, role) SELECT :'user_id'::uuid, id, 'owner' FROM co WHERE slug = 'large';

-- Registers --------------------------------------------------------------------------------------------------------------
INSERT INTO categories (id, company_id, name, dre_group, cash_flow_group, level, active)
SELECT md5('category-' || co.slug || '-' || n)::uuid, co.id, 'Category ' || n,
       CASE WHEN n % 2 = 0 THEN 'gross_revenue' ELSE 'operating_expenses' END, 'operating', 1, true
FROM co, generate_series(1, 20) n;
INSERT INTO categories (id, company_id, name, dre_group, cash_flow_group, level, active)
SELECT md5('category-transfers-' || co.slug)::uuid, co.id, 'Transfers', 'transfers', 'transfer', 1, true FROM co;

INSERT INTO cost_centers (id, company_id, name, active)
SELECT md5('cc-' || co.slug || '-' || n)::uuid, co.id, 'Cost center ' || n, true FROM co, generate_series(1, 8) n;

INSERT INTO bank_accounts (id, company_id, kind, name, active)
SELECT md5('account-' || co.slug || '-' || n)::uuid, co.id, 'checking', 'Account ' || n, true
FROM co, LATERAL generate_series(1, co.accts) n;

INSERT INTO account_openings (account_id, company_id, balance, on_date)
SELECT md5('account-' || co.slug || '-' || n)::uuid, co.id, 1000 * n, DATE '2021-11-01'
FROM co, LATERAL generate_series(1, co.accts) n;

INSERT INTO payment_terms (id, company_id, name, active)
SELECT md5('term-' || co.slug)::uuid, co.id, '30 days', true FROM co;

INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, is_seller, document_check)
SELECT md5('party-' || co.slug || '-' || n)::uuid, co.id, 'Party ' || lpad(n::text, 6, '0'),
       n % 3 <> 0, n % 3 = 0, false, n % 50 = 0, 'not_applicable'
FROM co, LATERAL generate_series(1, co.parties) n;

INSERT INTO products (id, company_id, kind, name, sku, unit, sale_price, cost_price, minimum_stock, active)
SELECT md5('product-' || co.slug || '-' || n)::uuid, co.id, 'product', 'Product ' || lpad(n::text, 5, '0'),
       'SKU-' || lpad(n::text, 5, '0'), 'UN', 10 + n % 90, 4 + n % 40, 5, true
FROM co, LATERAL generate_series(1, co.products) n;

-- Entries: 5 years of days, 1 to 3 installments, a category, a cost center and a party each ------------------------------
INSERT INTO financial_entries (id, company_id, kind, description, party_id, category_id, cost_center_id, competence_date)
SELECT md5('entry-' || co.slug || '-' || n)::uuid, co.id,
       CASE WHEN n % 3 = 0 THEN 'payable' ELSE 'receivable' END,
       'Entry ' || n,
       md5('party-' || co.slug || '-' || (1 + n % co.parties))::uuid,
       md5('category-' || co.slug || '-' || (1 + n % 20))::uuid,
       md5('cc-' || co.slug || '-' || (1 + n % 8))::uuid,
       DATE '2021-11-11' + (n % 1826)
FROM co, LATERAL generate_series(1, co.entries) n;
ANALYZE financial_entries;

INSERT INTO financial_entry_installments (company_id, entry_id, number, due_date, amount)
SELECT e.company_id, e.id, i, e.competence_date + 30 * i, 50 + (hashtext(e.id::text) % 500 + 500) % 500 + i
FROM financial_entries e
JOIN LATERAL generate_series(1, 1 + (hashtext(e.id::text) % 3 + 3) % 3) i ON true;
ANALYZE financial_entry_installments;

-- Settlements: most installments due before the end of the history are paid on their due day -------------------------------
INSERT INTO entry_settlements (id, company_id, entry_id, installment, settled_on, applied, interest, fine, discount, bank_account_id)
SELECT md5('settlement-' || i.entry_id || '-' || i.number)::uuid, i.company_id, i.entry_id, i.number, i.due_date, i.amount, 0, 0, 0,
       md5('account-' || co.slug || '-' || (1 + (hashtext(i.entry_id::text) % co.accts + co.accts) % co.accts))::uuid
FROM financial_entry_installments i
JOIN co ON co.id = i.company_id
WHERE i.due_date < DATE '2026-10-01' AND (hashtext(i.entry_id::text) % 10 + 10) % 10 < 8;

-- Sales with three lines each ---------------------------------------------------------------------------------------------
INSERT INTO sales (id, company_id, number, customer_id, seller_id, sold_on, payment_term_id, category_id, status)
SELECT md5('sale-' || co.slug || '-' || n)::uuid, co.id, n,
       md5('party-' || co.slug || '-' || (1 + n % co.parties))::uuid,
       md5('party-' || co.slug || '-' || (50 * (1 + n % LEAST(50, co.parties / 50))))::uuid,
       DATE '2021-11-11' + (n % 1826), md5('term-' || co.slug)::uuid,
       md5('category-' || co.slug || '-' || (2 * (1 + n % 10)))::uuid,
       CASE WHEN n % 20 = 0 THEN 'cancelled' WHEN n % 10 = 0 THEN 'open' ELSE 'completed' END
FROM co, LATERAL generate_series(1, co.sales) n;
INSERT INTO document_counters (company_id, kind, last)
SELECT id, 'sale', sales FROM co ON CONFLICT DO NOTHING;

INSERT INTO sale_items (company_id, sale_id, position, product_id, description, quantity, unit_price, discount)
SELECT co.id, md5('sale-' || co.slug || '-' || n)::uuid, p, md5('product-' || co.slug || '-' || (1 + (n * 7 + p) % co.products))::uuid,
       'Item', 1 + (n + p) % 5, 10 + (n + p) % 90, 0
FROM co, LATERAL generate_series(1, co.sales) n, generate_series(1, 3) p;

-- Stock: half goods in, half out ----------------------------------------------------------------------------------------
INSERT INTO stock_movements (id, company_id, product_id, kind, quantity, value, occurred_on)
SELECT md5('move-' || co.slug || '-' || n)::uuid, co.id, md5('product-' || co.slug || '-' || (1 + (n / 2) % co.products))::uuid,
       CASE WHEN n % 2 = 0 THEN 'in' ELSE 'out' END,
       CASE WHEN n % 2 = 0 THEN 1 + n % 9 ELSE -(1 + n % 5) END,
       CASE WHEN n % 2 = 0 THEN (1 + n % 9) * 4 ELSE -(1 + n % 5) * 4 END,
       DATE '2021-11-11' + (n % 1826)
FROM co, LATERAL generate_series(1, co.moves) n;

-- The audit trail ----------------------------------------------------------------------------------------------------------
INSERT INTO audit_log (company_id, actor_kind, actor_id, actor_name, entity, entity_id, action, changes, created_at)
SELECT co.id, 'user', :'user_id'::uuid, 'Seed user', 'financial_entry', md5('entry-' || co.slug || '-' || (1 + n % co.entries))::uuid,
       'created', '[{"field":"description","before":null,"after":"x"}]'::jsonb,
       TIMESTAMPTZ '2021-11-11 12:00:00+00' + (n % 1826) * interval '1 day' + (n % 86400) * interval '1 second'
FROM co, LATERAL generate_series(1, co.audits) n;

COMMIT;
ANALYZE;
