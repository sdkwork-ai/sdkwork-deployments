-- Staging environment: stable channel, full rollout, ten-minute checks.
-- Applied after 010_birdcoder_catalog_core.sql.

UPDATE deploy_download_catalog
SET product_version = '0.2.2',
    default_channel_key = 'stable',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 910001200001;

UPDATE deploy_download_item
SET installer_version = '0.2.2',
    release_tag = 'v0.2.2',
    updated_at = CURRENT_TIMESTAMP
WHERE id BETWEEN 910001200201 AND 910001200206;

UPDATE deploy_download_update
SET channel_key = 'stable',
    allowed_channels = '["stable"]'::jsonb,
    allow_prerelease = FALSE,
    minimum_supported_version = '0.2.0',
    check_interval_minutes = 10,
    rollout_percentage = 100,
    updated_at = CURRENT_TIMESTAMP
WHERE id BETWEEN 910001200301 AND 910001200306;
