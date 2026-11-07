-- Links that let someone choose a new password (ADR 0060). Keyed by the SHA-256 digest of the token, like a session: the raw
-- token only ever lives in the e-mail. A user may have several at once (asking again does not kill the link already
-- sent), the newest few are kept, and spending any of them deletes them all.
CREATE TABLE password_resets (
    token_digest TEXT        PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX password_resets_user_idx ON password_resets (user_id, created_at DESC);
CREATE INDEX password_resets_expires_idx ON password_resets (expires_at);
