-- sdkwork:migration
-- id: 0026_deploy_dns_zone_police_filing
-- engine: postgres
-- module: deploy
-- purpose: Remove root-domain public security (police) filing metadata from deploy_dns_zone.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

ALTER TABLE deploy_dns_zone
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_police_record_number,
    DROP CONSTRAINT IF EXISTS chk_deploy_dns_zone_police_record_url,
    DROP COLUMN IF EXISTS police_record_number,
    DROP COLUMN IF EXISTS police_record_url;
