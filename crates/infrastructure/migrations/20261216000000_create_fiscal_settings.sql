-- How a company issues fiscal documents (ADR 0073, ADR 0074): one row per company, replaced as a whole with its
-- operation natures and their tax rules. The constraints repeat the rules of `domain::FiscalSettings` that one row can
-- show (shapes, ranges, the pairing of a nature's kind with its CFOPs). That a code and a name are used once across the
-- natures is a unique index; that a company has at most 50 natures and a nature at most 20 rules is the position's range.
-- The A1 certificate is never here: only the day it expires, as the person typed it.
CREATE TABLE fiscal_settings (
    company_id              UUID PRIMARY KEY REFERENCES companies (id),
    legal_name              TEXT     NOT NULL,
    cnpj                    TEXT     NOT NULL,
    state_registration      TEXT,
    municipal_registration  TEXT,
    postal_code             TEXT     NOT NULL,
    street                  TEXT     NOT NULL,
    street_number           TEXT,
    complement              TEXT,
    district                TEXT,
    city                    TEXT     NOT NULL,
    state                   TEXT     NOT NULL,
    regime                  TEXT     NOT NULL,
    environment             TEXT     NOT NULL,
    nfe_series              SMALLINT NOT NULL,
    nfce_series             SMALLINT NOT NULL,
    nfse_series             SMALLINT NOT NULL,
    certificate_valid_until DATE,
    -- `btrim` only strips spaces, the domain trims every white space: the checks are patterns, like the parties' address.
    CONSTRAINT fiscal_settings_legal_name_valid CHECK (
        char_length(legal_name) BETWEEN 1 AND 150 AND legal_name !~ '^\s|\s$' AND legal_name !~ '[[:cntrl:]]'
    ),
    CONSTRAINT fiscal_settings_cnpj_valid CHECK (cnpj ~ '^[0-9A-Z]{14}$'),
    CONSTRAINT fiscal_settings_state_registration_valid CHECK (
        state_registration IS NULL OR state_registration ~ '^(ISENTO|[0-9]{2,14})$'
    ),
    CONSTRAINT fiscal_settings_municipal_registration_valid CHECK (
        municipal_registration IS NULL
        OR (char_length(municipal_registration) BETWEEN 1 AND 20 AND municipal_registration !~ '^\s|\s$'
            AND municipal_registration !~ '[[:cntrl:]]')
    ),
    CONSTRAINT fiscal_settings_postal_code_valid CHECK (postal_code ~ '^[0-9]{8}$'),
    CONSTRAINT fiscal_settings_state_valid CHECK (state IN (
        'AC', 'AL', 'AM', 'AP', 'BA', 'CE', 'DF', 'ES', 'GO', 'MA', 'MG', 'MS', 'MT', 'PA',
        'PB', 'PE', 'PI', 'PR', 'RJ', 'RN', 'RO', 'RR', 'RS', 'SC', 'SE', 'SP', 'TO')),
    CONSTRAINT fiscal_settings_address_text_valid CHECK (
        char_length(street) BETWEEN 1 AND 150 AND street !~ '^\s|\s$'
        AND char_length(city) BETWEEN 1 AND 80 AND city !~ '^\s|\s$'
        AND (street_number IS NULL
            OR (char_length(street_number) BETWEEN 1 AND 20 AND street_number !~ '^\s|\s$'))
        AND (complement IS NULL
            OR (char_length(complement) BETWEEN 1 AND 80 AND complement !~ '^\s|\s$'))
        AND (district IS NULL
            OR (char_length(district) BETWEEN 1 AND 80 AND district !~ '^\s|\s$'))
    ),
    CONSTRAINT fiscal_settings_regime_valid CHECK (regime IN ('simples_nacional', 'lucro_presumido')),
    CONSTRAINT fiscal_settings_environment_valid CHECK (environment IN ('homologation', 'production')),
    CONSTRAINT fiscal_settings_series_valid CHECK (
        nfe_series BETWEEN 1 AND 999 AND nfce_series BETWEEN 1 AND 999 AND nfse_series BETWEEN 1 AND 999
    ),
    -- `IS NOT NULL` is explicit because a CHECK passes when its result is NULL.
    CONSTRAINT fiscal_settings_production_needs_certificate CHECK (
        environment <> 'production' OR certificate_valid_until IS NOT NULL
    )
);

-- The operation natures, in the order they were written, from 1.
CREATE TABLE fiscal_natures (
    company_id      UUID     NOT NULL REFERENCES fiscal_settings (company_id),
    position        SMALLINT NOT NULL,
    code            TEXT     NOT NULL,
    name            TEXT     NOT NULL,
    kind            TEXT     NOT NULL,
    cfop_in_state   TEXT,
    cfop_interstate TEXT,
    active          BOOLEAN  NOT NULL,
    PRIMARY KEY (company_id, position),
    CONSTRAINT fiscal_natures_code_key UNIQUE (company_id, code),
    CONSTRAINT fiscal_natures_position_valid CHECK (position BETWEEN 1 AND 50),
    CONSTRAINT fiscal_natures_code_valid CHECK (code ~ '^[a-z0-9_-]{1,30}$'),
    CONSTRAINT fiscal_natures_name_valid CHECK (
        char_length(name) BETWEEN 1 AND 60 AND name !~ '^\s|\s$' AND name !~ '[[:cntrl:]]'
    ),
    CONSTRAINT fiscal_natures_kind_valid CHECK (kind IN ('product', 'service')),
    -- Goods carry the CFOP for inside the state (5xxx) and for another (6xxx); a service carries none.
    CONSTRAINT fiscal_natures_cfop_follows_kind CHECK (
        (kind = 'product' AND cfop_in_state ~ '^5[0-9]{3}$' AND cfop_interstate ~ '^6[0-9]{3}$')
        OR (kind = 'service' AND cfop_in_state IS NULL AND cfop_interstate IS NULL)
    )
);

-- One name per company, ignoring case.
CREATE UNIQUE INDEX fiscal_natures_name_key ON fiscal_natures (company_id, lower(name));

CREATE TABLE fiscal_tax_rules (
    company_id      UUID     NOT NULL,
    nature_position SMALLINT NOT NULL,
    position        SMALLINT NOT NULL,
    regime          TEXT     NOT NULL,
    ncm_prefix      TEXT,
    icms_code       TEXT     NOT NULL,
    pis_code        TEXT     NOT NULL,
    cofins_code     TEXT     NOT NULL,
    icms_rate       NUMERIC,
    pis_rate        NUMERIC,
    cofins_rate     NUMERIC,
    PRIMARY KEY (company_id, nature_position, position),
    CONSTRAINT fiscal_tax_rules_nature_fk FOREIGN KEY (company_id, nature_position)
        REFERENCES fiscal_natures (company_id, position),
    CONSTRAINT fiscal_tax_rules_position_valid CHECK (position BETWEEN 1 AND 20),
    CONSTRAINT fiscal_tax_rules_regime_valid CHECK (regime IN ('simples_nacional', 'lucro_presumido')),
    CONSTRAINT fiscal_tax_rules_ncm_prefix_valid CHECK (ncm_prefix IS NULL OR ncm_prefix ~ '^[0-9]{2,8}$'),
    -- A CSOSN has three digits (Simples Nacional) and a CST two; PIS and COFINS have a two-digit CST.
    CONSTRAINT fiscal_tax_rules_icms_code_valid CHECK (
        (regime = 'simples_nacional' AND icms_code ~ '^[0-9]{3}$')
        OR (regime = 'lucro_presumido' AND icms_code ~ '^[0-9]{2}$')
    ),
    CONSTRAINT fiscal_tax_rules_pis_cofins_codes_valid CHECK (pis_code ~ '^[0-9]{2}$' AND cofins_code ~ '^[0-9]{2}$'),
    CONSTRAINT fiscal_tax_rules_rates_valid CHECK (
        (icms_rate IS NULL OR (icms_rate BETWEEN 0 AND 100 AND icms_rate = round(icms_rate, 2)))
        AND (pis_rate IS NULL OR (pis_rate BETWEEN 0 AND 100 AND pis_rate = round(pis_rate, 2)))
        AND (cofins_rate IS NULL OR (cofins_rate BETWEEN 0 AND 100 AND cofins_rate = round(cofins_rate, 2)))
    )
);

-- One rule per regime and NCM prefix within a nature (no prefix is its own value).
CREATE UNIQUE INDEX fiscal_tax_rules_one_per_prefix_key
    ON fiscal_tax_rules (company_id, nature_position, regime, coalesce(ncm_prefix, ''));
