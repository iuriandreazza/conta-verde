-- Bank connections (ADR 0089): the link between a bank account of the company and an account at a bank aggregator, pulled every day
-- into statement lines. It holds no credential: the aggregator's keys are the installation's, in the environment.

-- A job of the queue that pulls one connection (ADR 0059).
ALTER TABLE jobs DROP CONSTRAINT jobs_kind_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_kind_check CHECK (kind IN ('send_email', 'deliver_webhook', 'sync_bank_connection'));
ALTER TABLE jobs ADD CONSTRAINT jobs_sync_args CHECK (status = 'dead' OR kind <> 'sync_bank_connection' OR cardinality(args) = 1);

CREATE TABLE bank_connections (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    bank_account_id UUID        NOT NULL,
    provider        TEXT        NOT NULL,
    item_id         TEXT        NOT NULL,
    account_id      TEXT        NOT NULL,
    status          TEXT        NOT NULL,
    -- Bookkeeping, written without an audit entry.
    last_synced_on  DATE,
    last_error      TEXT,
    -- Whoever connected it: the pulls are made in their name. No foreign key, so that history reads the same after the user is
    -- removed.
    created_by_id   UUID        NOT NULL,
    created_by_name TEXT        NOT NULL,
    created_at      BIGINT      NOT NULL,
    CONSTRAINT bank_connections_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT bank_connections_bank_account_fk FOREIGN KEY (company_id, bank_account_id)
        REFERENCES bank_accounts (company_id, id),
    CONSTRAINT bank_connections_provider_valid CHECK (provider ~ '^[a-z]{1,30}$'),
    CONSTRAINT bank_connections_item_valid CHECK (item_id ~ '^[[:graph:]]{1,100}$'),
    CONSTRAINT bank_connections_account_valid CHECK (account_id ~ '^[[:graph:]]{1,100}$'),
    CONSTRAINT bank_connections_status_valid CHECK (status IN ('connected', 'needs_attention', 'disconnected')),
    CONSTRAINT bank_connections_error_valid CHECK (
        last_error IS NULL OR (char_length(last_error) <= 200 AND last_error !~ '[[:cntrl:]]')
    ),
    CONSTRAINT bank_connections_created_by_valid CHECK (
        char_length(created_by_name) BETWEEN 1 AND 200 AND created_by_name !~ '[[:cntrl:]]'
    )
);

-- A bank account is fed by one connection at a time: what makes connecting twice, or in two places, safe.
CREATE UNIQUE INDEX bank_connections_one_live_key ON bank_connections (company_id, bank_account_id)
    WHERE status <> 'disconnected';
-- An account at the aggregator feeds one bank account in the whole installation: another company that learns its id cannot take
-- the statement of one that is connected.
CREATE UNIQUE INDEX bank_connections_one_live_account ON bank_connections (provider, account_id)
    WHERE status <> 'disconnected';
CREATE INDEX bank_connections_company_idx ON bank_connections (company_id, created_at DESC, id DESC);
CREATE INDEX bank_connections_due_idx ON bank_connections (last_synced_on NULLS FIRST) WHERE status = 'connected';
