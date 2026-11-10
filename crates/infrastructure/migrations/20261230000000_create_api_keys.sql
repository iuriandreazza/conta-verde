-- API keys of a company (ADR 0087): what a program uses to speak to the public `v1` API. Only the SHA-256 digest of the
-- secret is stored, never the secret. The constraints repeat the rules of `domain::ApiKey` that one row can show.
CREATE TABLE api_keys (
    id              UUID PRIMARY KEY,
    company_id      UUID        NOT NULL REFERENCES companies (id),
    name            TEXT        NOT NULL,
    -- The first characters of the secret, to tell keys apart.
    label           TEXT        NOT NULL,
    digest          TEXT        NOT NULL,
    -- `area:read` or `area:write`, sorted, once each; never members, accounting or fiscal.
    scopes          TEXT[]      NOT NULL,
    -- Unix milliseconds, like every moment the application passes in.
    expires_at      BIGINT,
    revoked         BOOLEAN     NOT NULL DEFAULT false,
    -- A key is made by a person: no foreign key, so that history reads the same after the user is removed.
    created_by_id   UUID        NOT NULL,
    created_by_name TEXT        NOT NULL,
    created_at      BIGINT      NOT NULL,
    rotated_from    UUID,
    -- Bookkeeping, written without an audit entry, at most once a day.
    last_used_on    DATE,
    CONSTRAINT api_keys_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT api_keys_digest_key UNIQUE (digest),
    CONSTRAINT api_keys_rotated_from_fk FOREIGN KEY (company_id, rotated_from)
        REFERENCES api_keys (company_id, id),
    CONSTRAINT api_keys_name_valid CHECK (
        char_length(name) BETWEEN 1 AND 200 AND name = btrim(name) AND name !~ '[[:cntrl:]]'
    ),
    CONSTRAINT api_keys_label_valid CHECK (label ~ '^cvk_[0-9a-f]{6}$'),
    CONSTRAINT api_keys_digest_valid CHECK (digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT api_keys_scopes_valid CHECK (
        cardinality(scopes) BETWEEN 1 AND 10
        AND scopes <@ ARRAY[
            'parties:read', 'parties:write', 'products:read', 'products:write', 'finance:read', 'finance:write',
            'sales:read', 'sales:write', 'purchases:read', 'purchases:write'
        ]
    ),
    CONSTRAINT api_keys_created_by_name_valid CHECK (
        char_length(created_by_name) BETWEEN 1 AND 200 AND created_by_name !~ '[[:cntrl:]]'
    )
);

CREATE INDEX api_keys_company_idx ON api_keys (company_id, created_at DESC, id DESC);

-- The trail now names who acted: a person or a key.
ALTER TABLE audit_log DROP CONSTRAINT audit_log_actor_kind_valid;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_actor_kind_valid CHECK (actor_kind IN ('user', 'api_key'));

-- The claim on the reference a program gave an order, made before the sale is (ADR 0087): the primary key is what makes a
-- retry after a crash, and two requests at once, end in one sale. The sale's id is the one the claim names; there is no
-- foreign key because the sale is written after its claim.
CREATE TABLE sale_claims (
    company_id UUID NOT NULL REFERENCES companies (id),
    reference  TEXT NOT NULL,
    sale_id    UUID NOT NULL,
    -- The request as the caller said it, to tell a retry from another order that reuses the reference.
    request    TEXT NOT NULL,
    PRIMARY KEY (company_id, reference),
    CONSTRAINT sale_claims_reference_valid CHECK (
        char_length(reference) BETWEEN 1 AND 100 AND reference = btrim(reference) AND reference !~ '[[:cntrl:]]'
    ),
    CONSTRAINT sale_claims_request_valid CHECK (char_length(request) BETWEEN 1 AND 500000)
);

CREATE UNIQUE INDEX sale_claims_sale_key ON sale_claims (company_id, sale_id);
