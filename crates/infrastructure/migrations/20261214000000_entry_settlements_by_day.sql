-- The accountant's export reads the settlements that are not reversed, settled in a span of days, oldest first
-- (ADR 0071). Without this it scans every settlement of the company and sorts them. Partial: a reversed settlement
-- never counts there.
CREATE INDEX entry_settlements_company_day_idx
    ON entry_settlements (company_id, settled_on, id)
    WHERE NOT reversed;
