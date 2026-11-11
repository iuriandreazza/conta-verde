-- Indexes the performance baseline justified (docs/adr/0097-performance-baseline.md, docs/performance/baseline.md). Each one is for the
-- queries named beside it, measured on a five-year company (about 126,000 entries, 250,000 installments, 190,000 settlements); there is no
-- index here that a recorded plan did not ask for. Built inside the migration's transaction, so on a large installation the table is
-- locked against writes for the seconds the build takes.

-- The installments of a company by due day: the cash flow's forecast (`open_by_due_day`), the overdue list, the delinquency report and the
-- reports on the due basis all read "this company's installments due between two days", and the only index on the table is the primary key
-- (entry_id, number), so each read every installment of every company. INCLUDE makes the scan cover the columns they take.
CREATE INDEX financial_entry_installments_company_due_idx
    ON financial_entry_installments (company_id, due_date) INCLUDE (entry_id, number, amount);

-- The settlements of one bank account through a day: the balance of an account and the cash flow's realized part
-- (`cash_of_account`, `cash_by_day`) read one account's settlements by day, and the existing indexes start with (company_id, settled_on)
-- or (company_id, entry_id), so each account's balance read every settlement of the company.
CREATE INDEX entry_settlements_company_account_day_idx
    ON entry_settlements (company_id, bank_account_id, settled_on) WHERE NOT reversed;
