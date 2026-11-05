-- Fails on databases that hold parties of companies that never existed (the API had no
-- authentication before this). Development databases can be reset with `docker compose down -v`.
ALTER TABLE parties
    ADD CONSTRAINT parties_company_fk FOREIGN KEY (company_id) REFERENCES companies (id);
