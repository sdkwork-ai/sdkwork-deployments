-- sdkwork:migration
-- id: 0004_deploy_dns_zone_wechat_verification
-- engine: postgres
-- module: deploy
-- description: Drops the WeChat MP domain-ownership verification file table.
-- reversible: true
-- rollback: up-migration recreates the table
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

DROP TABLE IF EXISTS deploy_dns_zone_wechat_verification;
