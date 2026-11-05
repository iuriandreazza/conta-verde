-- What the federal registry said when a CNPJ was registered. 'not_applicable' covers parties without
-- a document and CPFs; 'unverified' means the registry could not be reached at the time.
ALTER TABLE parties
    ADD COLUMN document_check      TEXT NOT NULL DEFAULT 'not_applicable',
    ADD COLUMN registry_legal_name TEXT,
    ADD COLUMN registry_situation  TEXT;

-- Parties created before the lookup existed were never checked against the registry.
UPDATE parties SET document_check = 'unverified' WHERE length(document) = 14;

ALTER TABLE parties
    ADD CONSTRAINT parties_document_check_valid
        CHECK (document_check IN ('not_applicable', 'verified', 'unverified')),
    ADD CONSTRAINT parties_registry_fields_match_check
        CHECK (
            (document_check = 'verified' AND registry_legal_name IS NOT NULL AND registry_situation IS NOT NULL)
            OR (document_check <> 'verified' AND registry_legal_name IS NULL AND registry_situation IS NULL)
        );
