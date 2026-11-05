-- Contacts (people to talk to) and tags (free labels) of a party. Both are replaced as a whole in the
-- transaction that writes the party and its audit entry. The constraints repeat the rules of
-- `domain::Contact` and `domain::Tag` that one row can show; the limits that span the rows of a
-- party (at most 20 contacts, at most 10 tags) are the domain's.

-- Only the target of the foreign keys below: a contact or a tag is in its party's company.
ALTER TABLE parties ADD CONSTRAINT parties_company_id_id_key UNIQUE (company_id, id);

-- As in the address checks, white space at either end is refused with a pattern, not `btrim`,
-- which only strips spaces while the domain trims every white space. What counts as white space
-- beyond ASCII depends on the database's locale; the domain decides, and never stores such a text.
CREATE TABLE party_tags (
    company_id UUID NOT NULL,
    party_id   UUID NOT NULL,
    tag        TEXT NOT NULL,
    -- A tag is on a party once.
    PRIMARY KEY (party_id, tag),
    CONSTRAINT party_tags_party_fk FOREIGN KEY (company_id, party_id)
        REFERENCES parties (company_id, id),
    -- The domain stores a tag in lower case, trimmed, without a comma (the audit trail writes the
    -- tags on one comma-separated line) and without control characters.
    CONSTRAINT party_tags_tag_valid CHECK (
        char_length(tag) BETWEEN 1 AND 30
        AND tag = lower(tag)
        AND tag !~ '^\s|\s$'
        AND strpos(tag, ',') = 0
        AND tag !~ '[\u0001-\u001f\u007f-\u009f]'
    )
);

-- The list filter: the parties of a company that carry a tag.
CREATE INDEX party_tags_company_tag_idx ON party_tags (company_id, tag, party_id);

CREATE TABLE party_contacts (
    company_id UUID     NOT NULL,
    party_id   UUID     NOT NULL,
    -- The order the contacts were given in, from 1.
    position   SMALLINT NOT NULL,
    name       TEXT     NOT NULL,
    job_title  TEXT,
    email      TEXT,
    phone      TEXT,
    PRIMARY KEY (party_id, position),
    CONSTRAINT party_contacts_party_fk FOREIGN KEY (company_id, party_id)
        REFERENCES parties (company_id, id),
    CONSTRAINT party_contacts_position_valid CHECK (position BETWEEN 1 AND 20),
    CONSTRAINT party_contacts_text_valid CHECK (
        char_length(name) BETWEEN 1 AND 100 AND name !~ '^\s|\s$'
        AND (job_title IS NULL
            OR (char_length(job_title) BETWEEN 1 AND 80 AND job_title !~ '^\s|\s$'))
    ),
    -- Stored in lower case, with something before the first `@` and a dot after it, like `Email`.
    CONSTRAINT party_contacts_email_valid CHECK (
        email IS NULL
        OR (email = lower(email) AND email !~ '^\s|\s$' AND email ~ '^[^@]+@.*[.]')
    ),
    -- Kept as written, but only digits, spaces and + - ( ) . with 8 to 15 digits.
    CONSTRAINT party_contacts_phone_valid CHECK (
        phone IS NULL
        OR (phone ~ '^[0-9 +().-]+$'
            AND char_length(phone) <= 30
            AND phone !~ '^\s|\s$'
            AND char_length(regexp_replace(phone, '[^0-9]', '', 'g')) BETWEEN 8 AND 15)
    )
);
