-- sdkwork:migration
-- id: 0002_deploy_domain_dns_record
-- engine: postgres
-- module: deploy
-- reversible: true
-- rollback: down-migration drops the snapshot table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

DROP INDEX IF EXISTS idx_deploy_domain_dns_record_domain;
DROP INDEX IF EXISTS idx_deploy_domain_dns_record_zone;

DROP TABLE IF EXISTS deploy_domain_dns_record;
