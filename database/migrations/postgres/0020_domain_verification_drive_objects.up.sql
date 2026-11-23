-- sdkwork:migration
-- id: 0020_domain_verification_drive_objects
-- engine: postgres
-- module: deploy
-- purpose: Give a verification file a durable home in sdkwork-drive.
--
--   A domain-ownership verification file is small, but it is still a file the
--   tenant published: it should live in Drive next to everything else the
--   tenant owns, so quota, audit, retention and deletion are Drive's job
--   instead of four kilobytes of text in a control-plane table.
--
--   The row keeps the inline `content` for now. It is not a second source of
--   truth: the edge serves from it while the Drive reference is recorded, and a
--   row written before this migration (no `drive_node_id`) keeps working
--   unchanged. Moving serving onto Drive is a later, separately verified step.
--
--   The columns are nullable on purpose: NULL means "this file was registered
--   before Drive storage existed" (or Drive was unavailable when it was
--   written), and the name/size/type beside it describe whatever bytes the row
--   still carries.

ALTER TABLE deploy_dns_zone_verification_file
    ADD COLUMN IF NOT EXISTS drive_node_id    VARCHAR(128),
    ADD COLUMN IF NOT EXISTS drive_space_id   VARCHAR(128),
    ADD COLUMN IF NOT EXISTS content_sha256   VARCHAR(64),
    ADD COLUMN IF NOT EXISTS content_bytes    BIGINT,
    ADD COLUMN IF NOT EXISTS content_type     VARCHAR(64);

COMMENT ON COLUMN deploy_dns_zone_verification_file.drive_node_id IS
    'sdkwork-drive 节点 id；NULL 表示该行在 Drive 存储上线前登记，内容仍在 content 列';
COMMENT ON COLUMN deploy_dns_zone_verification_file.drive_space_id IS
    'sdkwork-drive 空间 id（每租户一个「域名验证」空间）';
COMMENT ON COLUMN deploy_dns_zone_verification_file.content_sha256 IS
    '写入 Drive 的字节的 SHA-256（hex），用于证明行内容与对象一致';
COMMENT ON COLUMN deploy_dns_zone_verification_file.content_bytes IS
    '字节长度，边缘 HEAD 应答与上限复核使用';
COMMENT ON COLUMN deploy_dns_zone_verification_file.content_type IS
    '按扩展名推断的 Content-Type，边缘按它应答';
