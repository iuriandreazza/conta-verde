-- Offers to join a company in a role, sent to an e-mail address (ADR 0027). The link that accepts one
-- carries a token, and only the digest of the token is stored here: a leaked database does not leak
-- usable links. Accepting or withdrawing is recorded on the row, and never audited: it is a
-- membership write, which the trail does not hold (ADR 0008).
CREATE TABLE invitations (
    id           UUID PRIMARY KEY,
    company_id   UUID        NOT NULL REFERENCES companies (id),
    email        TEXT        NOT NULL,
    role         TEXT        NOT NULL,
    token_digest TEXT        NOT NULL,
    invited_by   UUID        NOT NULL REFERENCES users (id),
    created_at   TIMESTAMPTZ NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    accepted_at  TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    CONSTRAINT invitations_token_digest_key UNIQUE (token_digest),
    -- Nobody is invited as an owner: creating a company is what makes one.
    CONSTRAINT invitations_role_valid CHECK (
        role IN ('admin', 'financial', 'sales', 'stock', 'accountant', 'viewer')
    ),
    -- The domain stores the address trimmed and in lower case, with something before the `@` and a
    -- dot after it (`domain::Email`).
    CONSTRAINT invitations_email_valid CHECK (
        email = lower(email) AND email !~ '^\s|\s$' AND email ~ '^[^@]+@.*[.]'
    ),
    CONSTRAINT invitations_expires_after_creation CHECK (expires_at > created_at),
    -- An invitation is used or withdrawn, not both.
    CONSTRAINT invitations_resolved_once CHECK (accepted_at IS NULL OR revoked_at IS NULL)
);

-- A company has one open invitation per address. An open one that has expired is replaced by the
-- next invitation, which revokes it first (the index cannot know what time it is).
CREATE UNIQUE INDEX invitations_one_open_key
    ON invitations (company_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE INDEX invitations_company_idx ON invitations (company_id, created_at);
