-- Webhooks (ADR 0088): where a company wants to be told what happens, what was sent and how it went.

-- A job of the queue that sends a delivery (ADR 0059).
ALTER TABLE jobs DROP CONSTRAINT jobs_kind_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_kind_check CHECK (kind IN ('send_email', 'deliver_webhook'));
ALTER TABLE jobs ADD CONSTRAINT jobs_webhook_args CHECK (status = 'dead' OR kind <> 'deliver_webhook' OR cardinality(args) = 1);

CREATE TABLE webhook_subscriptions (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    url             TEXT        NOT NULL,
    -- Sorted, once each; empty is every event.
    events          TEXT[]      NOT NULL,
    active          BOOLEAN     NOT NULL,
    -- The signing secret, sealed by the application's cipher (company and subscription as associated data): never in plain text.
    secret_sealed   BYTEA       NOT NULL,
    -- The secret that was replaced, still signing until `previous_until` (Unix milliseconds).
    previous_sealed BYTEA,
    previous_until  BIGINT,
    -- A subscription is made by a person: no foreign key, so history reads the same after the user is removed.
    created_by_id   UUID        NOT NULL,
    created_by_name TEXT        NOT NULL,
    created_at      BIGINT      NOT NULL,
    CONSTRAINT webhook_subscriptions_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT webhook_subscriptions_url_valid CHECK (
        char_length(url) BETWEEN 9 AND 2000 AND url ~ '^https?://[^[:space:][:cntrl:]]+$'
    ),
    CONSTRAINT webhook_subscriptions_events_valid CHECK (
        events <@ ARRAY['sale.created', 'sale.completed', 'sale.cancelled', 'charge.paid', 'stock.below_minimum']
    ),
    CONSTRAINT webhook_subscriptions_secret_valid CHECK (octet_length(secret_sealed) BETWEEN 1 AND 512),
    CONSTRAINT webhook_subscriptions_previous_whole CHECK ((previous_sealed IS NULL) = (previous_until IS NULL)),
    CONSTRAINT webhook_subscriptions_created_by_valid CHECK (
        char_length(created_by_name) BETWEEN 1 AND 200 AND created_by_name !~ '[[:cntrl:]]'
    )
);

CREATE INDEX webhook_subscriptions_company_idx ON webhook_subscriptions (company_id, created_at, id);

-- One event for one subscription. The body is the company's own data and is kept for 30 days; what the subscriber answered is
-- never kept, only its status code and a short reason. The event's id is deterministic, so publishing the same event twice
-- makes one delivery: the unique key is the claim.
CREATE TABLE webhook_deliveries (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    subscription_id UUID        NOT NULL,
    event_id        UUID        NOT NULL,
    kind            TEXT        NOT NULL,
    payload         TEXT        NOT NULL,
    status          TEXT        NOT NULL,
    attempts        INTEGER     NOT NULL DEFAULT 0,
    last_status     INTEGER,
    last_error      TEXT,
    created_at      BIGINT      NOT NULL,
    updated_at      BIGINT      NOT NULL,
    delivered_at    BIGINT,
    CONSTRAINT webhook_deliveries_subscription_fk FOREIGN KEY (company_id, subscription_id)
        REFERENCES webhook_subscriptions (company_id, id),
    CONSTRAINT webhook_deliveries_event_key UNIQUE (subscription_id, event_id),
    CONSTRAINT webhook_deliveries_kind_valid CHECK (
        kind IN ('sale.created', 'sale.completed', 'sale.cancelled', 'charge.paid', 'stock.below_minimum', 'webhook.test')
    ),
    CONSTRAINT webhook_deliveries_status_valid CHECK (status IN ('pending', 'delivered', 'failed')),
    CONSTRAINT webhook_deliveries_attempts_valid CHECK (attempts >= 0),
    CONSTRAINT webhook_deliveries_last_status_valid CHECK (last_status IS NULL OR last_status BETWEEN 100 AND 599),
    CONSTRAINT webhook_deliveries_error_valid CHECK (
        last_error IS NULL OR (char_length(last_error) <= 200 AND last_error !~ '[[:cntrl:]]')
    ),
    CONSTRAINT webhook_deliveries_delivered_valid CHECK ((status = 'delivered') = (delivered_at IS NOT NULL))
);

CREATE INDEX webhook_deliveries_log_idx ON webhook_deliveries (company_id, subscription_id, created_at DESC, id DESC);
CREATE INDEX webhook_deliveries_age_idx ON webhook_deliveries (created_at);
