-- Down migration for 0024_deploy_app_capability.
--
-- Drops the eleven tables added by the up migration in reverse dependency order.
-- No pre-existing table or column is touched, so this migration cannot destroy
-- data that predates it. It exists because the up migration is declared
-- reversible (the schema is additive and pre-launch).

BEGIN;

DROP TABLE IF EXISTS deploy_app_capability_text;
DROP TABLE IF EXISTS deploy_app_embed;
DROP TABLE IF EXISTS deploy_app_market_asset;
DROP TABLE IF EXISTS deploy_app_model_binding;
DROP TABLE IF EXISTS deploy_app_scene_asset;
DROP TABLE IF EXISTS deploy_app_scene;
DROP TABLE IF EXISTS deploy_app_work_mode_tool;
DROP TABLE IF EXISTS deploy_app_work_mode;
DROP TABLE IF EXISTS deploy_app_capability_review;
DROP TABLE IF EXISTS deploy_app_capability_revision;
DROP TABLE IF EXISTS deploy_app_capability;

COMMIT;
