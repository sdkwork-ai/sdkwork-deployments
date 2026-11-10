-- BirdCoder download catalog: shared rows for every seed profile.
--
-- The catalog, the desktop card and the six installers are identical across
-- environments; profiles adjust the product version, the channel and the staged
-- rollout only (see 011..014). URLs point at the public ModelScope dataset so a
-- fresh environment can download without any manual console entry.
--
-- Reserved ids (DATABASE_SPEC section 6.4) keep the rows stable across installs
-- and reseeds: catalog 910001200001, card 910001200101, installers
-- 910001200201..206, update configurations 910001200301..306.

INSERT INTO deploy_download_catalog
    (id, uuid, tenant_id, organization_id, catalog_key, app_id, product_key,
     product_name, product_version, default_channel_key, status, revision,
     published_at, created_by, updated_by, created_at, updated_at, deleted_at, version)
VALUES
    (910001200001, '91000120-0000-4000-8000-000000000001', 100001, 0,
     'birdcoder-desktop', NULL, 'app.sdkwork.birdcoder', 'BirdCoder', '0.2.2-rc.1',
     'nightly', 'PUBLISHED', 1, CURRENT_TIMESTAMP, NULL, NULL,
     CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, NULL, 1)
ON CONFLICT (id) DO UPDATE SET
    catalog_key = EXCLUDED.catalog_key,
    product_key = EXCLUDED.product_key,
    product_name = EXCLUDED.product_name,
    status = EXCLUDED.status,
    published_at = EXCLUDED.published_at,
    updated_at = EXCLUDED.updated_at,
    deleted_at = NULL;

INSERT INTO deploy_download_card
    (id, uuid, tenant_id, organization_id, catalog_id, card_key, title, description,
     icon, tone, kind, badge, primary_strategy, hidden, sort_order,
     created_by, updated_by, created_at, updated_at, deleted_at, version)
VALUES
    (910001200101, '91000120-0000-4000-8000-000000000101', 100001, 0, 910001200001,
     'desktop', 'BirdCoder Desktop',
     'Install BirdCoder on Windows, macOS or Linux. The build matches the CPU architecture of your machine.',
     'desktop', 'brand', 'desktop', '', 'detected-platform', FALSE, 0,
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, NULL, 1)
ON CONFLICT (id) DO UPDATE SET
    title = EXCLUDED.title,
    description = EXCLUDED.description,
    icon = EXCLUDED.icon,
    tone = EXCLUDED.tone,
    kind = EXCLUDED.kind,
    primary_strategy = EXCLUDED.primary_strategy,
    hidden = EXCLUDED.hidden,
    sort_order = EXCLUDED.sort_order,
    updated_at = EXCLUDED.updated_at,
    deleted_at = NULL;

INSERT INTO deploy_download_item
    (id, uuid, tenant_id, organization_id, card_id, item_key, platform, architecture,
     label, file_name, installer_version, release_tag, package_id, artifact_id,
     download_source, size_bytes, checksum_sha256, primary_action, disabled,
     unavailable_label, sort_order, created_by, updated_by, created_at, updated_at, version)
VALUES
    (910001200201, '91000120-0000-4000-8000-000000000201', 100001, 0, 910001200101,
     'win-x64', 'windows', 'x64', 'Windows x64', 'BirdCoder-win-x64.exe', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-win-x64.exe?view=false"}',
     NULL, NULL, TRUE, FALSE, '', 0, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200202, '91000120-0000-4000-8000-000000000202', 100001, 0, 910001200101,
     'win-arm64', 'windows', 'arm64', 'Windows ARM64', 'BirdCoder-win-arm64.exe', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-win-arm64.exe?view=false"}',
     NULL, NULL, FALSE, FALSE, '', 1, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200203, '91000120-0000-4000-8000-000000000203', 100001, 0, 910001200101,
     'macos-arm64', 'macos', 'arm64', 'macOS Apple Silicon', 'BirdCoder-mac-arm64.dmg', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-mac-arm64.dmg?view=false"}',
     NULL, NULL, FALSE, FALSE, '', 2, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200204, '91000120-0000-4000-8000-000000000204', 100001, 0, 910001200101,
     'macos-x64', 'macos', 'x64', 'macOS Intel', 'BirdCoder-mac-x64.dmg', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-mac-x64.dmg?view=false"}',
     NULL, NULL, FALSE, FALSE, '', 3, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200205, '91000120-0000-4000-8000-000000000205', 100001, 0, 910001200101,
     'linux-x64', 'linux', 'x64', 'Linux x64', 'BirdCoder-linux-x64.AppImage', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-linux-x64.AppImage?view=false"}',
     NULL, NULL, FALSE, FALSE, '', 4, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200206, '91000120-0000-4000-8000-000000000206', 100001, 0, 910001200101,
     'linux-arm64', 'linux', 'arm64', 'Linux ARM64', 'BirdCoder-linux-arm64.AppImage', '0.2.2-rc.1', 'v0.2.2-rc.1',
     NULL, NULL,
     '{"href": "https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/BirdCoder-linux-arm64.AppImage?view=false"}',
     NULL, NULL, FALSE, FALSE, '', 5, NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1)
ON CONFLICT (id) DO UPDATE SET
    label = EXCLUDED.label,
    file_name = EXCLUDED.file_name,
    installer_version = EXCLUDED.installer_version,
    release_tag = EXCLUDED.release_tag,
    download_source = EXCLUDED.download_source,
    primary_action = EXCLUDED.primary_action,
    disabled = EXCLUDED.disabled,
    sort_order = EXCLUDED.sort_order,
    updated_at = EXCLUDED.updated_at,
    deleted_at = NULL;

-- Update feeds follow the published layout `<dataset>/feeds/<target>/` and stay
-- disabled until the release pipeline uploads the channel file: an enabled feed
-- without a document would turn every check into a visible failure.
INSERT INTO deploy_download_update
    (id, uuid, tenant_id, organization_id, item_id, enabled, provider, feed_url,
     github_owner, github_repo, channel_id, channel_key, channel_file, allowed_channels,
     allow_prerelease, allow_downgrade, minimum_supported_version, check_interval_minutes,
     rollout_percentage, release_notes_url, created_by, updated_by, created_at, updated_at, version)
VALUES
    (910001200301, '91000120-0000-4000-8000-000000000301', 100001, 0, 910001200201,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/win-x64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200302, '91000120-0000-4000-8000-000000000302', 100001, 0, 910001200202,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/win-arm64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200303, '91000120-0000-4000-8000-000000000303', 100001, 0, 910001200203,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/mac-arm64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200304, '91000120-0000-4000-8000-000000000304', 100001, 0, 910001200204,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/mac-x64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200305, '91000120-0000-4000-8000-000000000305', 100001, 0, 910001200205,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/linux-x64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1),
    (910001200306, '91000120-0000-4000-8000-000000000306', 100001, 0, 910001200206,
     FALSE, 'generic', 'https://www.modelscope.cn/datasets/sdkwork/birdcoder/resolve/master/feeds/linux-arm64/',
     '', '', NULL, 'nightly', '', '["nightly"]'::jsonb, TRUE, FALSE, '0.2.0', 10, 100, '',
     NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1)
ON CONFLICT (id) DO UPDATE SET
    provider = EXCLUDED.provider,
    feed_url = EXCLUDED.feed_url,
    channel_key = EXCLUDED.channel_key,
    allowed_channels = EXCLUDED.allowed_channels,
    minimum_supported_version = EXCLUDED.minimum_supported_version,
    updated_at = EXCLUDED.updated_at;
