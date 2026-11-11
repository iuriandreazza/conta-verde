-- Row-level security as the second barrier to the application's company checks (docs/adr/0093-row-level-security.md).
--
-- The server tells the database which companies a connection acts for through the session setting `app.company_ids`
-- (a comma separated list of ids: one company for a request, every company for the workers). A role that owns the
-- tables or has BYPASSRLS is not held to the policies, which is why the server refuses to serve requests as one.
-- Nothing is forced on the owner: migrations and backups run as the owner and must see every row.

CREATE FUNCTION cv_company_ids() RETURNS uuid[]
    LANGUAGE sql STABLE PARALLEL SAFE
AS $$
    SELECT COALESCE(
        string_to_array(NULLIF(current_setting('app.company_ids', true), ''), ',')::uuid[],
        '{}'::uuid[]
    )
$$;

-- What every migration that adds a table with a `company_id` calls (a test fails when one forgets).
CREATE FUNCTION cv_enable_tenancy(target regclass) RETURNS void
    LANGUAGE plpgsql
AS $$
BEGIN
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
    EXECUTE format(
        'CREATE POLICY tenant_isolation ON %s USING (company_id = ANY (cv_company_ids())) '
        'WITH CHECK (company_id = ANY (cv_company_ids()))',
        target
    );
END
$$;

-- Left out on purpose: they are read before any company is known (who a person belongs to, what an invitation's
-- link names). The ADR lists them with the reason, and the guard test holds the same list.
DO $$
DECLARE
    target regclass;
BEGIN
    FOR target IN
        SELECT c.oid::regclass
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'company_id' AND NOT a.attisdropped
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND c.relname NOT IN ('memberships', 'invitations')
    LOOP
        PERFORM cv_enable_tenancy(target);
    END LOOP;
END
$$;
