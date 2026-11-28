-- sdkwork:migration
-- id: 0026_deploy_dns_zone_police_filing
-- engine: postgres
-- module: deploy
-- purpose: Add optional root-domain public security (police) filing metadata to deploy_dns_zone.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_dns_zone
    ADD COLUMN IF NOT EXISTS police_record_number VARCHAR(64),
    ADD COLUMN IF NOT EXISTS police_record_url VARCHAR(2048);

ALTER TABLE deploy_dns_zone
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_police_record_number,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_police_record_url,
    ADD CONSTRAINT chk_deploy_dns_zone_police_record_number CHECK (
        police_record_number IS NULL
        OR (char_length(police_record_number) BETWEEN 1 AND 64 AND btrim(police_record_number) <> '')
    ),
    ADD CONSTRAINT chk_deploy_dns_zone_police_record_url CHECK (
        police_record_url IS NULL
        OR (char_length(police_record_url) BETWEEN 1 AND 2048 AND btrim(police_record_url) <> '')
    );

COMMENT ON COLUMN deploy_dns_zone.police_record_number IS 'Public security (police) filing record number; tenant-owned data, NULL when not supplied.';
COMMENT ON COLUMN deploy_dns_zone.police_record_url IS 'Absolute HTTP or HTTPS URL of the public security (police) filing record; tenant-owned data, NULL when not supplied.';
