-- What a member is in a company (ADR 0026). Every membership so far is the one that came with the
-- company, so it is its owner's.
ALTER TABLE memberships ADD COLUMN role TEXT NOT NULL DEFAULT 'owner';
-- A new membership says its role: nothing may become an owner by leaving it out.
ALTER TABLE memberships ALTER COLUMN role DROP DEFAULT;

ALTER TABLE memberships
    ADD CONSTRAINT memberships_role_valid CHECK (
        role IN ('owner', 'admin', 'financial', 'sales', 'stock', 'accountant', 'viewer')
    );
