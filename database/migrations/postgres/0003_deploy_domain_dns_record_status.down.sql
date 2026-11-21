-- sdkwork:migration
-- id: 0003_deploy_domain_dns_record_status
-- engine: postgres
-- module: deploy
-- reversible: true
-- rollback: down-migration drops the record_status column
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE deploy_domain_dns_record
    DROP CONSTRAINT IF EXISTS chk_deploy_domain_dns_record_status;

ALTER TABLE deploy_domain_dns_record
    DROP COLUMN IF EXISTS record_status;
