-- Whether the owner of an address has shown they read its mail (ADR 0063). Null until they do; accounts that existed before
-- are unconfirmed, and say so until the person confirms.
ALTER TABLE users ADD COLUMN email_verified_at TIMESTAMPTZ;

-- Links that confirm an address. Keyed by the SHA-256 digest of the token, like a session or a password reset: the raw token
-- only ever lives in the e-mail. Spending one deletes the user's others.
CREATE TABLE email_confirmations (
    token_digest TEXT        PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX email_confirmations_user_idx ON email_confirmations (user_id, created_at DESC);
CREATE INDEX email_confirmations_expires_idx ON email_confirmations (expires_at);
