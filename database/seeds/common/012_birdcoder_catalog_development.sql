-- Development environment: nightly channel, full rollout, five-minute checks so
-- a local build sees a new feed quickly.
-- Applied after 010_birdcoder_catalog_core.sql.

UPDATE deploy_download_catalog
SET product_version = '0.2.2-rc.1',
    default_channel_key = 'nightly',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 910001200001;

UPDATE deploy_download_item
SET installer_version = '0.2.2-rc.1',
    release_tag = 'v0.2.2-rc.1',
    updated_at = CURRENT_TIMESTAMP
WHERE id BETWEEN 910001200201 AND 910001200206;

UPDATE deploy_download_update
SET channel_key = 'nightly',
    allowed_channels = '["nightly"]'::jsonb,
    allow_prerelease = TRUE,
    minimum_supported_version = '0.2.0',
    check_interval_minutes = 5,
    rollout_percentage = 100,
    updated_at = CURRENT_TIMESTAMP
WHERE id BETWEEN 910001200301 AND 910001200306;
