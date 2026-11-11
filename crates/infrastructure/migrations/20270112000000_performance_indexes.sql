-- The index the performance baseline justified (docs/adr/0097-performance-baseline.md, docs/performance/baseline.md), measured on a
-- five-year company (about 126,000 entries, 250,000 installments, 190,000 settlements). There is no index here that a recorded plan did not ask
-- for: one on the settlements by bank account was tried and measured at no gain, because the join to the entries dominates (ADR 0097). Built
-- inside the migration's transaction, so on a large installation the table is locked against writes for the seconds the build takes.

-- The installments of a company by due day: the DRE, the reports on the competence and due bases, the overdue list and the cash flow's forecast
-- read "this company's installments" (by entry, or between two due days), and the table's only index was its primary key (entry_id, number), so
-- each read the heap of every installment of every company. INCLUDE makes the scan cover the columns they take, with no heap fetch.
CREATE INDEX financial_entry_installments_company_due_idx
    ON financial_entry_installments (company_id, due_date) INCLUDE (entry_id, number, amount);
