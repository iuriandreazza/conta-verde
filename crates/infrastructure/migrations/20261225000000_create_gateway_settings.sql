-- A company's own gateway settings (ADR 0085): which gateway it issues through and the secrets to do it, **sealed** by the server
-- (AES-256-GCM, a fresh nonce each time, the company and the field as associated data) before they get here. The database never holds
-- a secret in the clear, and the application never returns one. The gateway's address and the hosts files are fetched from stay in
-- the server's environment: a company cannot set them.
CREATE TABLE gateway_settings (
    company_id                  UUID PRIMARY KEY REFERENCES companies (id),
    -- How many times the settings were written: a write names the version it was made from (a stale one is refused).
    version                     INTEGER NOT NULL,
    kind                        TEXT    NOT NULL,
    focus_token_homologation    BYTEA,
    focus_token_production      BYTEA,
    nfeio_api_key               BYTEA,
    webhook_secret              BYTEA,
    nfeio_company_homologation  TEXT,
    nfeio_company_production    TEXT,
    nfeio_search_confirmed      BOOLEAN NOT NULL,
    CONSTRAINT gateway_settings_kind_valid CHECK (kind IN ('focus', 'nfeio')),
    CONSTRAINT gateway_settings_version_valid CHECK (version >= 1),
    -- A sealed secret is a version byte, a 12-byte nonce, at least one byte of text and a 16-byte tag, and at most 400 bytes.
    CONSTRAINT gateway_settings_sealed_valid CHECK (
        (focus_token_homologation IS NULL OR octet_length(focus_token_homologation) BETWEEN 30 AND 400)
        AND (focus_token_production IS NULL OR octet_length(focus_token_production) BETWEEN 30 AND 400)
        AND (nfeio_api_key IS NULL OR octet_length(nfeio_api_key) BETWEEN 30 AND 400)
        AND (webhook_secret IS NULL OR octet_length(webhook_secret) BETWEEN 30 AND 400)
    ),
    CONSTRAINT gateway_settings_company_ids_valid CHECK (
        (nfeio_company_homologation IS NULL
            OR (nfeio_company_homologation ~ '^[A-Za-z0-9_-]{1,100}$'))
        AND (nfeio_company_production IS NULL
            OR (nfeio_company_production ~ '^[A-Za-z0-9_-]{1,100}$'))
    )
);
