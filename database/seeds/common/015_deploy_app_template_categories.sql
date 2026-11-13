-- Deploy application-template marketplace: the initial category taxonomy.
--
-- Categories are tenant-scoped reference data (`deploy_app_template_category`)
-- and the publish API requires one. A freshly provisioned tenant therefore
-- cannot publish a template until the taxonomy exists. This seed is the
-- baseline set shipped with every profile; platform administrators may extend,
-- rename or disable it afterwards.
--
-- Keys are stable per tenant and are the dedupe key for reseeds. Reserved ids
-- (DATABASE_SPEC section 6.4) keep the rows stable across installs:
-- 910001300001..010 and UUIDs 91000130-0000-4000-8000-00000000000N.
--
-- The conflict target is the live tenant/key unique index, not the reserved id:
-- an administrator-created category with the same key is updated in place
-- instead of aborting the whole seed.
WITH seed
    (id, uuid, category_key, display_name, description, sort_order) AS (
    VALUES
        (910001300001, '91000130-0000-4000-8000-000000000001', 'web-h5',
         'Web 与 H5 应用', '基于 Web 技术、可在浏览器或 H5 容器运行的应用脚手架。', 10),
        (910001300002, '91000130-0000-4000-8000-000000000002', 'pc-desktop',
         'PC 与桌面应用', 'PC Web 与 Electron/Tauri 等桌面应用脚手架。', 20),
        (910001300003, '91000130-0000-4000-8000-000000000003', 'mobile-native',
         '移动原生应用', 'Android、iOS、HarmonyOS 原生应用脚手架。', 30),
        (910001300004, '91000130-0000-4000-8000-000000000004', 'cross-platform',
         '跨端应用', 'Flutter、React Native、uni-app 等多端共享代码的应用脚手架。', 40),
        (910001300005, '91000130-0000-4000-8000-000000000005', 'mini-program',
         '小程序', '微信、支付宝等小程序应用脚手架。', 50),
        (910001300006, '91000130-0000-4000-8000-000000000006', 'api-service',
         'API 与服务端', '后端服务、API 与微服务工程脚手架。', 60),
        (910001300007, '91000130-0000-4000-8000-000000000007', 'full-stack',
         '全栈应用', '前后端一体的应用工程模板。', 70),
        (910001300008, '91000130-0000-4000-8000-000000000008', 'ai-agent',
         'AI 与智能体应用', '集成模型、Agent、MCP 或 RAG 能力的应用模板。', 80),
        (910001300009, '91000130-0000-4000-8000-000000000009', 'developer-tooling',
         '开发工具与脚手架', 'CLI、SDK、组件库与开发工具模板。', 90),
        (910001300010, '91000130-0000-4000-8000-000000000010', 'other',
         '其他应用', '暂不属于以上分类的应用模板。', 100)
)
INSERT INTO deploy_app_template_category
    (id, uuid, tenant_id, organization_id, parent_id, category_key,
     display_name, description, sort_order, status, created_by, updated_by,
     created_at, updated_at, version, deleted_at)
SELECT
    seed.id, seed.uuid, 100001, 0, NULL, seed.category_key,
    seed.display_name, seed.description, seed.sort_order, 'ACTIVE', NULL, NULL,
    CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, NULL
FROM seed
ON CONFLICT (tenant_id, category_key) WHERE deleted_at IS NULL
DO UPDATE SET
    display_name = EXCLUDED.display_name,
    description = EXCLUDED.description,
    sort_order = EXCLUDED.sort_order,
    status = 'ACTIVE',
    updated_at = CURRENT_TIMESTAMP,
    deleted_at = NULL;
