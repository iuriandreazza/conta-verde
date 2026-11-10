-- ADR 0080: what an NFS-e needs of the issuer and of a service. The issuer's municipality (IBGE, seven digits) is one
-- column of the settings; a service nature may carry the ISSQN terms: the national tax code of the service, the
-- municipality's own code for it, how the tax is charged and, when the company sets it, the rate. Both are optional
-- while a company has not set them (an NFS-e refuses without them); a product nature carries none.
ALTER TABLE fiscal_settings ADD COLUMN city_ibge_code TEXT;
ALTER TABLE fiscal_settings ADD CONSTRAINT fiscal_settings_city_ibge_code_valid CHECK (
    city_ibge_code IS NULL OR city_ibge_code ~ '^[0-9]{7}$'
);

ALTER TABLE fiscal_natures ADD COLUMN national_tax_code TEXT;
ALTER TABLE fiscal_natures ADD COLUMN municipal_service_code TEXT;
ALTER TABLE fiscal_natures ADD COLUMN iss_taxation TEXT;
ALTER TABLE fiscal_natures ADD COLUMN iss_rate NUMERIC;

-- The national code and the way the tax is charged come together or not at all, and only a service has them. `IS NOT NULL`
-- is explicit because a test of NULL is NULL, and a CHECK passes on NULL.
ALTER TABLE fiscal_natures ADD CONSTRAINT fiscal_natures_service_terms_valid CHECK (
    (national_tax_code IS NULL AND iss_taxation IS NULL AND municipal_service_code IS NULL AND iss_rate IS NULL)
    OR (kind = 'service' AND national_tax_code IS NOT NULL AND national_tax_code ~ '^[0-9]{6}$'
        AND iss_taxation IS NOT NULL AND iss_taxation IN ('taxable', 'immune', 'export', 'not_incident')
        AND (municipal_service_code IS NULL
            OR (char_length(municipal_service_code) BETWEEN 1 AND 20 AND municipal_service_code !~ '^\s|\s$'
                AND municipal_service_code !~ '[[:cntrl:]]' AND municipal_service_code !~ '[:|;]'))
        AND (iss_rate IS NULL OR (iss_rate BETWEEN 0 AND 100 AND iss_rate = round(iss_rate, 2))))
);
