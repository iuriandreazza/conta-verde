-- The NFC-e as a fiscal document (ADR 0082): the consumer's invoice of the counter, model 65. It has an NF-e's identity (a 44-digit
-- access key, a number and a series), so it takes the same columns and the same rules as the NF-e, and none of the NFS-e's.
ALTER TABLE fiscal_documents DROP CONSTRAINT fiscal_documents_kind_valid;
ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_kind_valid CHECK (kind IN ('nfe', 'nfce', 'nfse'));

ALTER TABLE fiscal_documents DROP CONSTRAINT fiscal_documents_authorization_follows_status;
ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_authorization_follows_status CHECK (
    (kind IN ('nfe', 'nfce')
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
