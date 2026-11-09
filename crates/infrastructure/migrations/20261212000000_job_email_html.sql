-- An e-mail job may carry the HTML of its layout as a fourth argument (ADR 0068); a job queued before it has three.
-- The rule was written without a name, so it is looked up by what it says.
DO $$
DECLARE
    rule TEXT;
BEGIN
    SELECT conname INTO rule
      FROM pg_constraint
     WHERE conrelid = 'jobs'::regclass
       AND contype = 'c'
       AND pg_get_constraintdef(oid) LIKE '%cardinality(args) = 3%';
    EXECUTE format('ALTER TABLE jobs DROP CONSTRAINT %I', rule);
END $$;

ALTER TABLE jobs
    ADD CONSTRAINT jobs_send_email_arguments
    CHECK (status = 'dead' OR kind <> 'send_email' OR cardinality(args) IN (3, 4));
