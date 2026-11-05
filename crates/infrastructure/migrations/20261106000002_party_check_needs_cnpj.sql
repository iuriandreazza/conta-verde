-- Only a CNPJ (14 characters) is looked up, so only a CNPJ can carry a verified, not_found or
-- unverified check. Nothing writes anything else today; this keeps it that way even for code that
-- bypasses the domain. `IS NOT NULL` is explicit because a CHECK passes when its result is NULL.
ALTER TABLE parties
    ADD CONSTRAINT parties_document_check_needs_cnpj
        CHECK (document_check = 'not_applicable' OR (document IS NOT NULL AND length(document) = 14));
