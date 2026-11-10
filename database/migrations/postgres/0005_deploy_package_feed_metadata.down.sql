-- sdkwork:migration
-- id: 0005_deploy_package_feed_metadata
-- engine: postgres
-- module: deploy
-- description: Reverts the package feed metadata columns added by the 0005 up
--   migration. Dropping the columns does not touch package rows, artifacts or
--   any generated feed file; the next feed generation simply reports the
--   package as not publishable until it is re-digested.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE deploy_package
    DROP COLUMN IF EXISTS feed_metadata;

ALTER TABLE deploy_package
    DROP COLUMN IF EXISTS checksum_sha512;
