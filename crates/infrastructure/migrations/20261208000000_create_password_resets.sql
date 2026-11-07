-- A link that lets someone choose a new password (ADR 0060). Keyed by the SHA-256 digest of the token, like a session:
-- the raw token only ever lives in the e-mail. One per user, so asking again replaces the last link, and spending it
-- deletes the row.
CREATE TABLE password_resets (
    user_id      UUID        PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    token_digest TEXT        NOT NULL UNIQUE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX password_resets_expires_idx ON password_resets (expires_at);
