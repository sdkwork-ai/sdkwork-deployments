-- sdkwork:migration
-- id: 0004_deploy_dns_zone_wechat_verification
-- engine: postgres
-- module: deploy
-- description: Stores the WeChat MP domain-ownership verification file for one
--   Zone (公众号/小程序 JS 接口安全域名、网页授权域名、业务域名验证)。The platform
--   issues an MP_verify_xxx.txt file the operator must make reachable at the
--   domain root; the operator uploads the file verbatim here and the edge
--   serves it for every hostname in the Zone (a hostname resolves to its Zone,
--   so a subdomain row and the apex row manage the same Zone file). Content is
--   kept byte-exact — WeChat compares bytes, and any normalization would turn
--   a passing verification into "content mismatch". One row per Zone: a fresh
--   platform file replaces the previous one whole.
-- reversible: true
-- rollback: down-migration drops the table
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

CREATE TABLE IF NOT EXISTS deploy_dns_zone_wechat_verification (
    zone_id    BIGINT      NOT NULL,
    file_name  VARCHAR(64) NOT NULL,
    content    TEXT        NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (zone_id),
    CONSTRAINT fk_deploy_dns_zone_wechat_verification_zone
        FOREIGN KEY (zone_id) REFERENCES deploy_dns_zone(id) ON DELETE CASCADE,
    CONSTRAINT chk_deploy_dns_zone_wechat_verification_name
        CHECK (file_name <> '' AND file_name LIKE '%.txt'),
    CONSTRAINT chk_deploy_dns_zone_wechat_verification_content
        CHECK (content <> '')
);

COMMENT ON TABLE deploy_dns_zone_wechat_verification IS
    '微信公众号域名归属验证文件（每 Zone 一份，边缘按请求主机名原样提供）';
COMMENT ON COLUMN deploy_dns_zone_wechat_verification.file_name IS
    '平台发放的文件名（如 MP_verify_xxx.txt），按精确名称服务，不做大小写归一';
COMMENT ON COLUMN deploy_dns_zone_wechat_verification.content IS
    '文件原文：逐字节保存、逐字节返回';
