-- The role the server serves requests as (docs/adr/0093-row-level-security.md): it owns nothing and has no
-- BYPASSRLS, so the row-level policies apply to it. The schema's owner (DATABASE_MIGRATE_URL) migrates and
-- grants it what it needs at every start. Development password only: choose your own in production.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'conta_verde_app') THEN
        CREATE ROLE conta_verde_app LOGIN PASSWORD 'conta_verde_app' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
    END IF;
END
$$;
