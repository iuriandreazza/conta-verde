-- When a company's monthly package is announced to its accountants (ADR 0067, ADR 0072): one row per company, replaced
-- as a whole. A company that never chose has no row, and is off.
CREATE TABLE package_schedules (
    company_id UUID PRIMARY KEY REFERENCES companies (id),
    enabled    BOOLEAN  NOT NULL,
    -- A day every month has: the package goes out for the previous month once the day has come.
    day        SMALLINT NOT NULL,
    CONSTRAINT package_schedules_day CHECK (day BETWEEN 1 AND 28)
);

-- What the hourly pass reads: the schedules that are on, by the day they go out.
CREATE INDEX package_schedules_due_idx ON package_schedules (day) WHERE enabled;

-- A month announced for a company, claimed by one statement before anything is queued: a unique row per company and
-- month is what makes it go out once, however often the pass runs and on however many instances. Operational, like
-- `jobs`: not audited, and given back when nothing could be queued.
CREATE TABLE package_sends (
    company_id UUID   NOT NULL REFERENCES companies (id),
    -- The first day of the month the package is for.
    month      DATE   NOT NULL,
    -- When it was claimed, in milliseconds since the Unix epoch, from the pass's own clock.
    claimed_at BIGINT NOT NULL,
    PRIMARY KEY (company_id, month),
    CONSTRAINT package_sends_first_of_month CHECK (month = date_trunc('month', month::timestamp)::date)
);
