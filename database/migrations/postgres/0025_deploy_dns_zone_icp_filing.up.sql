-- sdkwork:migration
-- id: 0025_deploy_dns_zone_icp_filing
-- engine: postgres
-- module: deploy
-- purpose: Add optional root-domain ICP filing metadata to deploy_dns_zone.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_dns_zone
    ADD COLUMN IF NOT EXISTS icp_record_number VARCHAR(64),
    ADD COLUMN IF NOT EXISTS icp_entity_name VARCHAR(200),
    ADD COLUMN IF NOT EXISTS icp_website_name VARCHAR(200),
    ADD COLUMN IF NOT EXISTS icp_review_date DATE;

ALTER TABLE deploy_dns_zone
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_record_number,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_entity_name,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_website_name,
    ADD CONSTRAINT chk_deploy_dns_zone_icp_record_number CHECK (
        icp_record_number IS NULL
        OR (char_length(icp_record_number) BETWEEN 1 AND 64 AND btrim(icp_record_number) <> '')
    ),
    ADD CONSTRAINT chk_deploy_dns_zone_icp_entity_name CHECK (
        icp_entity_name IS NULL
        OR (char_length(icp_entity_name) BETWEEN 1 AND 200 AND btrim(icp_entity_name) <> '')
    ),
    ADD CONSTRAINT chk_deploy_dns_zone_icp_website_name CHECK (
        icp_website_name IS NULL
        OR (char_length(icp_website_name) BETWEEN 1 AND 200 AND btrim(icp_website_name) <> '')
    );

COMMENT ON COLUMN deploy_dns_zone.icp_record_number IS 'ICP filing record number; NULL when not supplied.';
COMMENT ON COLUMN deploy_dns_zone.icp_entity_name IS 'ICP filing entity name; NULL when not supplied.';
COMMENT ON COLUMN deploy_dns_zone.icp_website_name IS 'ICP filing website name; NULL when not supplied.';
COMMENT ON COLUMN deploy_dns_zone.icp_review_date IS 'ICP filing review date stored as a calendar DATE without timezone conversion.';
