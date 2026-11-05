-- 'not_found': the registry answered that it has no such CNPJ. Its open data lags, so a new company
-- can be missing; the party is kept and flagged instead of rejected. The registry columns stay null
-- for this value, as for 'unverified' (parties_registry_fields_match_check already says so).
ALTER TABLE parties DROP CONSTRAINT parties_document_check_valid;
ALTER TABLE parties
    ADD CONSTRAINT parties_document_check_valid
        CHECK (document_check IN ('not_applicable', 'verified', 'not_found', 'unverified'));
