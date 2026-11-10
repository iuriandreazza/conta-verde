-- An access key names one document of a company **in one environment**: a document of homologation is not a fiscal
-- document, and the key carries no environment, so the same series, number and month can make the same key there and in
-- production (ADR 0076).
DROP INDEX fiscal_documents_access_key_key;
CREATE UNIQUE INDEX fiscal_documents_access_key_key
    ON fiscal_documents (company_id, environment, access_key) WHERE access_key IS NOT NULL;
