-- Connectors (ADR 0090): a company's use of an e-commerce package for one store. It holds no credential of the store: turning it on
-- makes an API key and a webhook subscription, which are referenced here.

CREATE TABLE connectors (
    id                      UUID PRIMARY KEY,
    company_id              UUID        NOT NULL REFERENCES companies (id),
    kind                    TEXT        NOT NULL,
    name                    TEXT        NOT NULL,
    active                  BOOLEAN     NOT NULL,
    api_key_id              UUID,
    webhook_subscription_id UUID,
    -- Whoever made it. No foreign key, so that history reads the same after the user is removed.
    created_by_id           UUID        NOT NULL,
    created_by_name         TEXT        NOT NULL,
    created_at              BIGINT      NOT NULL,
    CONSTRAINT connectors_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT connectors_api_key_fk FOREIGN KEY (company_id, api_key_id)
        REFERENCES api_keys (company_id, id),
    CONSTRAINT connectors_subscription_fk FOREIGN KEY (company_id, webhook_subscription_id)
        REFERENCES webhook_subscriptions (company_id, id),
    CONSTRAINT connectors_kind_valid CHECK (kind IN ('woocommerce')),
    CONSTRAINT connectors_name_valid CHECK (
        char_length(name) BETWEEN 1 AND 100 AND name !~ '[[:cntrl:]]' AND name = btrim(name)
    ),
    -- A connector that is on has the key it speaks with; one that is off has none.
    CONSTRAINT connectors_key_follows_state CHECK (active = (api_key_id IS NOT NULL)),
    CONSTRAINT connectors_created_by_valid CHECK (
        char_length(created_by_name) BETWEEN 1 AND 200 AND created_by_name !~ '[[:cntrl:]]'
    )
);

CREATE UNIQUE INDEX connectors_name_key ON connectors (company_id, lower(name));
CREATE INDEX connectors_company_idx ON connectors (company_id, created_at DESC, id DESC);
