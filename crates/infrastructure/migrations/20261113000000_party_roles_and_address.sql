-- Carriers and sellers are roles like customers and suppliers: a party can have any of them.
-- Existing parties are none of the new ones.
ALTER TABLE parties
    ADD COLUMN is_carrier BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN is_seller  BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE parties
    ALTER COLUMN is_carrier DROP DEFAULT,
    ALTER COLUMN is_seller DROP DEFAULT;

ALTER TABLE parties DROP CONSTRAINT parties_has_role;
ALTER TABLE parties
    ADD CONSTRAINT parties_has_role CHECK (is_customer OR is_supplier OR is_carrier OR is_seller);

-- One optional address per party. It is all or nothing: the CEP, the street, the city and the state
-- come together, and the number, the complement and the district are optional. Only the shape is
-- checked, as in `domain::Address`; whether a CEP exists is a lookup, not a rule.
ALTER TABLE parties
    ADD COLUMN postal_code   TEXT,
    ADD COLUMN street        TEXT,
    ADD COLUMN street_number TEXT,
    ADD COLUMN complement    TEXT,
    ADD COLUMN district      TEXT,
    ADD COLUMN city          TEXT,
    ADD COLUMN state         TEXT;

-- `IS NOT NULL` is explicit in the second branch because a CHECK passes when its result is NULL.
ALTER TABLE parties
    ADD CONSTRAINT parties_address_is_whole CHECK (
        (postal_code IS NULL AND street IS NULL AND street_number IS NULL AND complement IS NULL
            AND district IS NULL AND city IS NULL AND state IS NULL)
        OR (postal_code IS NOT NULL AND street IS NOT NULL AND city IS NOT NULL AND state IS NOT NULL)
    ),
    ADD CONSTRAINT parties_postal_code_valid CHECK (postal_code ~ '^[0-9]{8}$'),
    ADD CONSTRAINT parties_state_valid CHECK (state IN (
        'AC', 'AL', 'AM', 'AP', 'BA', 'CE', 'DF', 'ES', 'GO', 'MA', 'MG', 'MS', 'MT', 'PA',
        'PB', 'PE', 'PI', 'PR', 'RJ', 'RN', 'RO', 'RR', 'RS', 'SC', 'SE', 'SP', 'TO')),
    ADD CONSTRAINT parties_address_text_valid CHECK (
        char_length(street) BETWEEN 1 AND 150 AND street = btrim(street)
        AND char_length(city) BETWEEN 1 AND 80 AND city = btrim(city)
        AND (street_number IS NULL OR (char_length(street_number) BETWEEN 1 AND 20 AND street_number = btrim(street_number)))
        AND (complement IS NULL OR (char_length(complement) BETWEEN 1 AND 80 AND complement = btrim(complement)))
        AND (district IS NULL OR (char_length(district) BETWEEN 1 AND 80 AND district = btrim(district)))
    );
