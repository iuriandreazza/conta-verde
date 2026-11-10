-- ADR 0078: a gateway may name the documents itself (NFE.io), and may answer a cancellation later. So a document keeps the
-- gateway's own reference (set once, while it waits for the authority, never changed), and a cancellation is an event
-- like the correction letter: recorded as `sent` before the gateway is asked and settled once.
ALTER TABLE fiscal_documents ADD COLUMN gateway_reference TEXT;

ALTER TABLE fiscal_documents ADD CONSTRAINT fiscal_documents_gateway_reference_valid CHECK (
    gateway_reference IS NULL
    OR (status <> 'draft' AND char_length(gateway_reference) BETWEEN 1 AND 100
        AND gateway_reference !~ '[[:cntrl:]]' AND gateway_reference !~ '^\s|\s$')
);

-- The same function, with the reference among what never changes once it is set, and what is authorized or cancelled.
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
    IF OLD.gateway_reference IS NOT NULL AND NEW.gateway_reference IS DISTINCT FROM OLD.gateway_reference THEN
        RAISE EXCEPTION 'the gateway reference of a fiscal document never changes';
    END IF;
    IF OLD.status IN ('authorized', 'cancelled') AND (
        NEW.company_id, NEW.kind, NEW.sale_id, NEW.issuer_cnpj, NEW.issuer_name, NEW.regime, NEW.environment,
        NEW.recipient_name, NEW.recipient_document, NEW.nature_code, NEW.total, NEW.issued_at, NEW.issued_on,
        NEW.submitted_by_user_id, NEW.submitted_by_name, NEW.access_key, NEW.number, NEW.series, NEW.protocol,
        NEW.gateway_reference
    ) IS DISTINCT FROM (
        OLD.company_id, OLD.kind, OLD.sale_id, OLD.issuer_cnpj, OLD.issuer_name, OLD.regime, OLD.environment,
        OLD.recipient_name, OLD.recipient_document, OLD.nature_code, OLD.total, OLD.issued_at, OLD.issued_on,
        OLD.submitted_by_user_id, OLD.submitted_by_name, OLD.access_key, OLD.number, OLD.series, OLD.protocol,
        OLD.gateway_reference
    ) THEN
        RAISE EXCEPTION 'the content of an authorized fiscal document never changes';
    END IF;
    RETURN NEW;
END;
$$;

-- A cancellation names its document, has no place in a sequence and no range, and its reason is 15 to 255 characters.
ALTER TABLE fiscal_events DROP CONSTRAINT fiscal_events_kind_valid;
ALTER TABLE fiscal_events DROP CONSTRAINT fiscal_events_kind_follows_columns;
ALTER TABLE fiscal_events ADD CONSTRAINT fiscal_events_kind_valid CHECK (kind IN ('correction', 'cancellation', 'invalidation'));
ALTER TABLE fiscal_events ADD CONSTRAINT fiscal_events_kind_follows_columns CHECK (
    (kind = 'correction' AND document_id IS NOT NULL AND sequence BETWEEN 1 AND 99
        AND char_length(body) BETWEEN 15 AND 1000
        AND series IS NULL AND first_number IS NULL AND last_number IS NULL AND environment IS NULL)
    OR (kind = 'cancellation' AND document_id IS NOT NULL AND sequence IS NULL
        AND char_length(body) BETWEEN 15 AND 255
        AND series IS NULL AND first_number IS NULL AND last_number IS NULL AND environment IS NULL)
    OR (kind = 'invalidation' AND document_id IS NULL AND sequence IS NULL
        AND char_length(body) BETWEEN 15 AND 255
        AND series BETWEEN 1 AND 999
        AND first_number BETWEEN 1 AND 999999999 AND last_number BETWEEN first_number AND 999999999
        AND environment IN ('homologation', 'production'))
);

-- At most one cancellation of a document is waiting or accepted: asking twice while one is on its way is refused here, not
-- only by the use case.
CREATE UNIQUE INDEX fiscal_events_one_open_cancellation_key ON fiscal_events (company_id, document_id)
    WHERE kind = 'cancellation' AND status IN ('sent', 'accepted');
