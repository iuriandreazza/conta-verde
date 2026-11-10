-- Events of fiscal documents (ADR 0076): the correction letters of an authorized NF-e and the invalidations of ranges of
-- numbers. An event is **recorded as `sent` before the gateway is asked** and then settled once, so a call whose answer
-- was lost is never repeated blindly (the events are not idempotent by reference at the authority, ADR 0075). It is never
-- deleted, its content never changes, and its status moves only from `sent` to `accepted` or `refused`.
CREATE TABLE fiscal_events (
    id                    UUID PRIMARY KEY,
    company_id            UUID        NOT NULL REFERENCES companies (id),
    kind                  TEXT        NOT NULL,
    document_id           UUID,
    sequence              SMALLINT,
    body                  TEXT        NOT NULL,
    series                SMALLINT,
    first_number          BIGINT,
    last_number           BIGINT,
    environment           TEXT,
    status                TEXT        NOT NULL,
    protocol              TEXT,
    refusal_code          TEXT,
    refusal_message       TEXT,
    blob_id               UUID,
    xml_size              BIGINT,
    requested_by_user_id  UUID        NOT NULL,
    requested_by_name     TEXT        NOT NULL,
    CONSTRAINT fiscal_events_company_id_id_key UNIQUE (company_id, id),
    CONSTRAINT fiscal_events_document_fk FOREIGN KEY (company_id, document_id)
        REFERENCES fiscal_documents (company_id, id),
    CONSTRAINT fiscal_events_kind_valid CHECK (kind IN ('correction', 'invalidation')),
    CONSTRAINT fiscal_events_status_valid CHECK (status IN ('sent', 'accepted', 'refused')),
    -- A correction names its document and its place among that document's letters (1 to 20) and has no range; an
    -- invalidation names a series, a range and the environment and has no document.
    CONSTRAINT fiscal_events_kind_follows_columns CHECK (
        (kind = 'correction' AND document_id IS NOT NULL AND sequence BETWEEN 1 AND 20
            AND char_length(body) BETWEEN 15 AND 1000
            AND series IS NULL AND first_number IS NULL AND last_number IS NULL AND environment IS NULL)
        OR (kind = 'invalidation' AND document_id IS NULL AND sequence IS NULL
            AND char_length(body) BETWEEN 15 AND 255
            AND series BETWEEN 1 AND 999
            AND first_number BETWEEN 1 AND 999999999 AND last_number BETWEEN first_number AND 999999999
            AND environment IN ('homologation', 'production'))
    ),
    CONSTRAINT fiscal_events_body_one_line CHECK (body !~ '[[:cntrl:]]' AND body !~ '^\s|\s$'),
    CONSTRAINT fiscal_events_protocol_valid CHECK (
        protocol IS NULL OR (char_length(protocol) BETWEEN 1 AND 50 AND protocol !~ '[[:cntrl:]]')
    ),
    -- The answer follows the status: `sent` has none, `accepted` may have a protocol and a file, `refused` has the words.
    CONSTRAINT fiscal_events_answer_follows_status CHECK (
        (status = 'sent' AND protocol IS NULL AND refusal_code IS NULL AND refusal_message IS NULL
            AND blob_id IS NULL AND xml_size IS NULL)
        OR (status = 'accepted' AND refusal_code IS NULL AND refusal_message IS NULL
            AND ((blob_id IS NULL AND xml_size IS NULL) OR (blob_id IS NOT NULL AND xml_size > 0)))
        OR (status = 'refused' AND refusal_code IS NOT NULL AND refusal_message IS NOT NULL
            AND char_length(refusal_code) <= 50 AND char_length(refusal_message) <= 500
            AND refusal_code !~ '[[:cntrl:]]' AND refusal_message !~ '[[:cntrl:]]'
            AND protocol IS NULL AND blob_id IS NULL AND xml_size IS NULL)
    ),
    CONSTRAINT fiscal_events_requested_by_valid CHECK (
        char_length(requested_by_name) BETWEEN 1 AND 150 AND requested_by_name !~ '[[:cntrl:]]'
    )
);

-- One letter per place in a document's sequence: two writers asking at once cannot both take number 3.
CREATE UNIQUE INDEX fiscal_events_sequence_key ON fiscal_events (company_id, document_id, sequence)
    WHERE kind = 'correction';

CREATE INDEX fiscal_events_invalidations_idx ON fiscal_events (company_id, last_number) WHERE kind = 'invalidation';

CREATE FUNCTION fiscal_events_keep_the_record() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'a fiscal event is never deleted';
    END IF;
    IF OLD.status <> 'sent' THEN
        RAISE EXCEPTION 'a settled fiscal event never changes';
    END IF;
    IF (NEW.company_id, NEW.kind, NEW.document_id, NEW.sequence, NEW.body, NEW.series, NEW.first_number,
        NEW.last_number, NEW.environment, NEW.requested_by_user_id, NEW.requested_by_name)
       IS DISTINCT FROM
       (OLD.company_id, OLD.kind, OLD.document_id, OLD.sequence, OLD.body, OLD.series, OLD.first_number,
        OLD.last_number, OLD.environment, OLD.requested_by_user_id, OLD.requested_by_name) THEN
        RAISE EXCEPTION 'the content of a fiscal event never changes';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER fiscal_events_are_a_record
    BEFORE UPDATE OR DELETE ON fiscal_events
    FOR EACH ROW EXECUTE FUNCTION fiscal_events_keep_the_record();
