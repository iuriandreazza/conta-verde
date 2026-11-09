-- A company's closed period (ADR 0067, ADR 0069): one row per company, replaced as a whole. A company that
-- never closed a month has no row; one that reopened everything has a row with no date.
CREATE TABLE period_locks (
    company_id     UUID PRIMARY KEY REFERENCES companies (id),
    -- The last day that is closed. Always the last day of a month: the lock closes whole months.
    locked_through DATE,
    CONSTRAINT period_locks_last_of_month CHECK (
        locked_through IS NULL
        OR locked_through = (date_trunc('month', locked_through::timestamp) + INTERVAL '1 month - 1 day')::date
    )
);
