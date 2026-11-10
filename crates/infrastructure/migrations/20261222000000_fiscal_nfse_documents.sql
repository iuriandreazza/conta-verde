-- The NFS-e as a fiscal document (ADR 0081). An NFS-e has no 44-digit access key: its identity is a number the system
-- gives it and, when the gateway says them, the national key and a verification code, so they have columns of their own and
-- the NF-e's `access_key`, `number` and `series` stay the NF-e's. The DPS (declaração de prestação de serviço) is
-- numbered by the issuer, once, when the document is first sent.
ALTER TABLE fiscal_documents
    ADD COLUMN nfse_number            BIGINT,
    ADD COLUMN nfse_key               TEXT,
    ADD COLUMN nfse_verification_code TEXT,
    ADD COLUMN dps_series             INTEGER,
    ADD COLUMN dps_number             BIGINT;

ALTER TABLE fiscal_documents DROP CONSTRAINT fiscal_documents_kind_valid;
ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_kind_valid CHECK (kind IN ('nfe', 'nfse'));

-- Each kind has what the authority gave in its own columns and none in the other's. `IS NOT NULL` is explicit because a
-- CHECK passes when its result is NULL.
ALTER TABLE fiscal_documents DROP CONSTRAINT fiscal_documents_authorization_follows_status;
ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_authorization_follows_status CHECK (
    (kind = 'nfe'
        AND nfse_number IS NULL AND nfse_key IS NULL AND nfse_verification_code IS NULL
        AND dps_series IS NULL AND dps_number IS NULL
        AND (
            (status IN ('authorized', 'cancelled')
                AND access_key IS NOT NULL AND access_key ~ '^[0-9]{44}$'
                AND number IS NOT NULL AND number BETWEEN 1 AND 999999999
                AND series IS NOT NULL AND series BETWEEN 1 AND 999)
            OR (status NOT IN ('authorized', 'cancelled')
                AND access_key IS NULL AND number IS NULL AND series IS NULL AND protocol IS NULL)
        ))
    OR (kind = 'nfse'
        AND access_key IS NULL AND number IS NULL AND series IS NULL
        AND (
            (status IN ('authorized', 'cancelled')
                AND nfse_number IS NOT NULL AND nfse_number BETWEEN 1 AND 999999999999999
                AND dps_number IS NOT NULL)
            OR (status NOT IN ('authorized', 'cancelled')
                AND nfse_number IS NULL AND nfse_key IS NULL AND nfse_verification_code IS NULL
                AND protocol IS NULL)
        ))
);

ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_dps_valid CHECK (
    (dps_series IS NULL AND dps_number IS NULL)
    OR (kind = 'nfse' AND dps_series IS NOT NULL AND dps_series BETWEEN 1 AND 49999
        AND dps_number IS NOT NULL AND dps_number BETWEEN 1 AND 999999999999999)
);

ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_nfse_identity_valid CHECK (
    (nfse_key IS NULL OR nfse_key ~ '^[0-9A-Za-z]{1,60}$')
    AND (nfse_verification_code IS NULL
        OR (char_length(nfse_verification_code) BETWEEN 1 AND 50 AND nfse_verification_code !~ '[[:cntrl:]]'))
);

-- A DPS series and number name one document of a company in one environment: the national system refuses a number
-- twice, and the issuer never gives one twice.
CREATE UNIQUE INDEX fiscal_documents_dps_key
    ON fiscal_documents (company_id, environment, dps_series, dps_number) WHERE dps_number IS NOT NULL;

-- The national key, when there is one, names one NFS-e.
CREATE UNIQUE INDEX fiscal_documents_nfse_key_key
    ON fiscal_documents (company_id, environment, nfse_key) WHERE nfse_key IS NOT NULL;

-- The same function, with what the NFS-e keeps among what never changes once authorized, and the DPS number among what
-- never changes once it is set.
CREATE OR REPLACE FUNCTION fiscal_documents_keep_the_record() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'a fiscal document is never deleted';
    END IF;
    IF OLD.status IN ('cancelled', 'denied') AND NEW.status <> OLD.status THEN
        RAISE EXCEPTION 'a cancelled or denied fiscal document never moves again';
    END IF;
    IF OLD.gateway_reference IS NOT NULL AND OLD.status <> 'rejected'
       AND NEW.gateway_reference IS DISTINCT FROM OLD.gateway_reference THEN
        RAISE EXCEPTION 'the gateway reference of a fiscal document never changes';
    END IF;
    IF OLD.dps_number IS NOT NULL
       AND (NEW.dps_series, NEW.dps_number) IS DISTINCT FROM (OLD.dps_series, OLD.dps_number) THEN
        RAISE EXCEPTION 'the DPS number of a fiscal document never changes';
    END IF;
    IF OLD.status IN ('authorized', 'cancelled') AND (
        NEW.company_id, NEW.kind, NEW.sale_id, NEW.issuer_cnpj, NEW.issuer_name, NEW.regime, NEW.environment,
        NEW.recipient_name, NEW.recipient_document, NEW.nature_code, NEW.total, NEW.issued_at, NEW.issued_on,
        NEW.submitted_by_user_id, NEW.submitted_by_name, NEW.access_key, NEW.number, NEW.series, NEW.protocol,
        NEW.gateway_reference, NEW.nfse_number, NEW.nfse_key, NEW.nfse_verification_code,
        NEW.dps_series, NEW.dps_number
    ) IS DISTINCT FROM (
        OLD.company_id, OLD.kind, OLD.sale_id, OLD.issuer_cnpj, OLD.issuer_name, OLD.regime, OLD.environment,
        OLD.recipient_name, OLD.recipient_document, OLD.nature_code, OLD.total, OLD.issued_at, OLD.issued_on,
        OLD.submitted_by_user_id, OLD.submitted_by_name, OLD.access_key, OLD.number, OLD.series, OLD.protocol,
        OLD.gateway_reference, OLD.nfse_number, OLD.nfse_key, OLD.nfse_verification_code,
        OLD.dps_series, OLD.dps_number
    ) THEN
        RAISE EXCEPTION 'the content of an authorized fiscal document never changes';
    END IF;
    RETURN NEW;
END;
$$;
