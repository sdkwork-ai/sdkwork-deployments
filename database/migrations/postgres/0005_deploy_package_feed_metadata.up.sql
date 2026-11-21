-- sdkwork:migration
-- id: 0005_deploy_package_feed_metadata
-- engine: postgres
-- module: deploy
-- description: Adds the SHA-512 digest and feed metadata a package needs before
--   Deploy can serve an electron-updater compatible feed. The existing
--   checksum_sha256 column satisfies artifact integrity checks but not the
--   updater protocol, which publishes a sha512 per file; feed_metadata carries
--   the remaining protocol-specific facts (release notes, minimum OS, blockmap
--   name) without widening the package row per updater field. Both columns are
--   nullable/defaulted so packages registered before this migration keep
--   working and simply stay out of a generated feed until they are re-digested.
-- reversible: true
-- rollback: down-migration drops the two package columns
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE deploy_package
    ADD COLUMN IF NOT EXISTS checksum_sha512 VARCHAR(256);

ALTER TABLE deploy_package
    ADD COLUMN IF NOT EXISTS feed_metadata JSONB NOT NULL DEFAULT '{}';

COMMENT ON COLUMN deploy_package.checksum_sha512 IS 'SHA-512 of the published installer; required by the electron-updater feed';
COMMENT ON COLUMN deploy_package.feed_metadata IS 'Updater protocol facts for this package (release notes, minimum OS, blockmap name)';
