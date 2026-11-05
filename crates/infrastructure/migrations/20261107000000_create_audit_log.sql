-- Who changed what, and when ([ADR 0008]). Rows are written in the same transaction as the business
-- write they describe.
CREATE TABLE audit_log (
    -- Orders the whole trail, also for rows written in the same millisecond.
    sequence   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- No ON DELETE CASCADE: a company with a history is not silently deleted.
    company_id UUID        NOT NULL REFERENCES companies (id),
    actor_kind TEXT        NOT NULL,
    -- No foreign key on the actor and a snapshot of the name: history must read the same after
    -- the user is renamed or removed.
    actor_id   UUID        NOT NULL,
    actor_name TEXT        NOT NULL,
    entity     TEXT        NOT NULL,
    entity_id  UUID        NOT NULL,
    action     TEXT        NOT NULL,
    -- [{"field": ..., "from": ... or null, "to": ... or null}, ...]
    changes    JSONB       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT audit_log_actor_kind_valid CHECK (actor_kind IN ('user')),
    CONSTRAINT audit_log_action_valid CHECK (action IN ('created', 'updated')),
    CONSTRAINT audit_log_changes_is_array CHECK (jsonb_typeof(changes) = 'array')
);

CREATE INDEX audit_log_entity_idx ON audit_log (company_id, entity, entity_id, sequence);

-- The application connects with a single database role, so the "no update or delete grants" of the
-- ADR cannot be enforced with privileges. A trigger keeps the log append-only even against a bug
-- in the application (not against a superuser, who can drop the trigger).
CREATE FUNCTION audit_log_is_append_only() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only';
END;
$$;

CREATE TRIGGER audit_log_rows_are_immutable
    BEFORE UPDATE OR DELETE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_log_is_append_only();

CREATE TRIGGER audit_log_is_not_truncated
    BEFORE TRUNCATE ON audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION audit_log_is_append_only();
