-- Work to do later and once (ADR 0059). Operational, not a register of the business: no company, no audit trail, and a
-- finished job is deleted. Moments are Unix seconds, taken from the worker's clock, never from the database's.
CREATE TABLE jobs (
    id           UUID PRIMARY KEY,
    kind         TEXT    NOT NULL CHECK (kind IN ('send_email')),
    -- The text the kind needs, in order. Emptied when the job is given up on, so a message is not kept.
    args         TEXT[]  NOT NULL,
    status       TEXT    NOT NULL CHECK (status IN ('pending', 'running', 'dead')),
    attempts     INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Not tried before this moment.
    run_at       BIGINT  NOT NULL,
    -- While running: when another worker may take the job, if this one has not finished it.
    locked_until BIGINT,
    last_error   TEXT,
    CHECK ((status = 'running') = (locked_until IS NOT NULL)),
    CHECK (status = 'dead' OR kind <> 'send_email' OR cardinality(args) = 3)
);

CREATE INDEX jobs_due ON jobs (run_at, id) WHERE status = 'pending';
CREATE INDEX jobs_leases ON jobs (locked_until) WHERE status = 'running';
