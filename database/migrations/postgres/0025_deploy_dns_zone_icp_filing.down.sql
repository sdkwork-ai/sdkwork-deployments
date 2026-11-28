-- sdkwork:migration
-- id: 0025_deploy_dns_zone_icp_filing
-- engine: postgres
-- module: deploy
-- purpose: Remove root-domain ICP filing metadata from deploy_dns_zone.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_dns_zone
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_record_number,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_entity_name,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_icp_website_name,
    DROP COLUMN IF EXISTS icp_record_number,
    DROP COLUMN IF EXISTS icp_entity_name,
    DROP COLUMN IF EXISTS icp_website_name,
    DROP COLUMN IF EXISTS icp_review_date;
