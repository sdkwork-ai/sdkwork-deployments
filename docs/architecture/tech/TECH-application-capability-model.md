# 应用能力模型（Application Capability Model）

> 状态：active ｜ 权威范围：`deploy_app_capability*` / `deploy_app_work_mode*` / `deploy_app_scene*` /
> `deploy_app_model_binding` / `deploy_app_market_asset` / `deploy_app_embed`（本仓）与
> `iam_application_enterprise_order*` / `iam_application_enterprise_seat`（IAM 仓）
> ｜ 对标：WorkBuddy Buddy 应用（`https://open.workbuddy.cn/docs/buddy-app`）与第三方应用规范

## 1. 为什么需要这一层

本仓已有的 `deploy_app` 只回答「这是什么应用、部署到哪、发什么包」。一个应用作为 **AI 工作台**
被用户打开时还缺一层：用户看到的 slogan、输入框上方的场景胶囊、可切换的工作模式、可用的模型、
应用内能发现哪些专家/技能/连接器，以及「这套东西」如何定版、送审、发布、再按企业授权分发。

WorkBuddy 把这层叫 **Buddy 应用**，并把它拆成六个自定义维度（工作模式、场景胶囊、模型配置、
垂直行业市场、内嵌已有功能）+ 身份与授权 + 审核发布 + 企业单。本模型把这套能力落成 SDKWork 的
数据与接口，**词汇使用 SDKWork 领域词**（capability / work mode / scene / market asset / embed /
review / enterprise order），不复制 WorkBuddy 的品牌词。

## 2. 权威边界（谁能写哪张表）

| 领域 | 权威 | 说明 |
| --- | --- | --- |
| 应用身份、部署、发布包、环境、域名 | `deploy_app` 及其既有子表 | 本模型不复制这些事实，只引用 `app_id` |
| 应用体验配置（六维） | **本模型（deploy）** | 仅本仓写入；对外只经 `apps.capability.*` |
| 配置定版与审核发布 | **本模型（deploy）** | 不可变 `..._revision` + `..._review` |
| OAuth 客户端、scopes、回调、可信 Origin、client_secret | **sdkwork-iam** | deploy 只存 `oauth_client_id` 字符串引用，**不建跨模块外键** |
| 企业单 / 席位（企业授权） | **sdkwork-iam** | deploy 只声明分发范围（`distribution_scope`） |

跨模块引用规则沿用 `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md`：按**稳定业务键/uuid** 引用，不存事实
副本；`asset_snapshot` 只允许放展示用快照，权威读取一律回源。

`deploy_app_capability.oauth_client_id` 是这条规则的实例：它存 IAM 凭据行的 `client_id`
（`iam_application_installation_credential.client_id`），不复制回调、scopes、可信 Origin，也不建
外键——IAM 拥有那张表的生命周期，客户端轮换由应用自己的 provisioning 流程更新引用，永远不会从
另一个模块级联下来。

IAM 侧对应的能力（迁移 `0055_iam_installation_credential_policy`）：

| 列 | 作用 |
| --- | --- |
| `allowed_origins_json` | 可信 Origin 白名单；空数组 = 不限制（向前兼容）。非空时 `/iam/v3/oauth/authorize` 拒绝 origin 未登记的 `redirect_uri` |
| `client_status` | `pending_review` / `enabled` / `rejected` / `disabled`（与 `credential_status` 的密钥生命周期正交） |
| `review_note` / `reviewed_at` | 审核意见与裁决时间 |

授权服务器解析客户端时**先读** `runtime_config.oauth.relyingParty`（既有安装行为不变），
未命中再回退到凭据表——这让历史上「只写不读」的凭据行第一次真正可以完成授权。

## 3. 六个维度 → 表 → 字段

### 3.1 维度一：工作模式（Work Mode）

`deploy_app_work_mode`（每应用 0..N 条，`mode_key` 稳定）+ `deploy_app_work_mode_tool`（模式预绑定的工具）。
文案（名称/描述）在 `deploy_app_work_mode_text`，按 `(work_mode_id, locale, field_name)` 唯一。

- `system_prompt`：模式的人设、术语、输出结构（WorkBuddy 的 System Prompt）。
- `deploy_app_work_mode_tool`：`tool_kind ∈ (BUILTIN, MCP)`，`tool_ref` 是内置工具键或 MCP
  `server_key`/`connector_key`，`source_module` 由约束钉死（BUILTIN→deploy，MCP→mcp）。
- 每应用**至多一个** `is_default`（部分唯一索引）；`deploy_app_capability.default_work_mode_key`
  指向它，指针失效时不级联删配置行（删除模式由服务层在同一事务里清指针）。

### 3.2 维度二：场景胶囊（Scene）

`deploy_app_scene`：输入框上方的快捷提示词模板（`prompt_template`、`icon_ref`、`sort_order`、
`is_featured` 精选位）。`deploy_app_scene_asset` 把场景一键绑定到专家/专家团/技能/连接器/案例。

资产引用是 **(asset_kind, source_module, asset_ref)** 三元组，`CHECK` 强制配对：

| asset_kind | source_module | asset_ref 形态 |
| --- | --- | --- |
| `EXPERT` | `agents` | `agent.expert.<slug>` |
| `EXPERT_TEAM` | `agents` | `team-<template>` |
| `SKILL` | `skills` | `skill.<...>` |
| `CONNECTOR` | `mcp` | `server_key` / `connector_key` |
| `CASE` | `appstore` | 模板业务键 |

### 3.3 维度三：模型配置（Model Binding）

`deploy_app_model_binding`：`(provider_code, model_key)` 勾选、`display_order` 排序、每应用一个
`is_default`。模型池本身属于平台模型目录（`sdkwork-llm` / `sdkwork-models`），本表只登记「这个应用
允许哪些模型、以什么顺序展示」，不复制模型属性。

### 3.4 维度四：垂直行业市场（Market Asset）

`deploy_app_market_asset`：应用内市场展示哪些生态资产，`section ∈ (EXPERTS, SKILLS, CONNECTORS,
FEATURED_SCENES, CASES)`，`CHECK` 钉死 section ↔ asset_kind 的合法组合；`category_code` 承接分类标签，
`is_featured` 承接精选位。与 `deploy_app_scene_asset` 的区别是**用途**：场景绑定是「一键启动」，
市场登记是「可发现」。

### 3.5 维度五：内嵌已有功能（Embed）

`deploy_app_embed`：`embed_kind ∈ (MCP_APP, IFRAME)`；MCP_APP 要求 `server_ref`（MCP server 业务键）+
`transport`，IFRAME 要求 `endpoint_url`（https）。`scopes_json` 是这个内嵌应用被授予的最小 scope 集合，
`auth_mode ∈ (NONE, OAUTH, API_KEY)`。认证凭据本身归 IAM，本表只登记引用与授权策略。

### 3.6 维度六：身份与体验（Capability Root）

`deploy_app_capability`（每应用 1 条）：`avatar_url`、`distribution_scope`、`capability_status`、
`default_work_mode_key`、当前/已发布 `revision_id`、`config_schema_version`。
文案（slogan、输入框占位符、简介）在 `deploy_app_capability_text`。

## 4. 状态机

### 4.1 配置状态（`capability_status`）

```
DRAFT ──submit──▶ IN_REVIEW ──approve──▶ PUBLISHED
  ▲                  │  │                    │
  │               reject  withdraw           │ 变更（新建 revision）
  └──────────────────┴─────┴──── DRAFT ◀─────┘
PUBLISHED ──suspend──▶ SUSPENDED ──resume──▶ PUBLISHED
```

不变式：

1. `IN_REVIEW` 期间草稿仍可编辑，但送审的 `revision_id` 是**快照**，审核对象不随草稿漂移。
2. `approve` 只允许把**当前送审**的那个 revision 置为 `published_revision_id`；`published_at` 非空。
3. 任何对已发布配置的写入都把状态退回 `DRAFT`（WorkBuddy「已发布应用变更需重新提交审核」），
   但 `published_revision_id` 保留，直到新版本过审 —— 线上永远有一个可回滚的已发布版本。
4. `SUSPENDED` 只影响新会话，不删除配置。

### 4.2 审核记录（`deploy_app_capability_review`）

`PENDING → APPROVED | REJECTED | WITHDRAWN`；`attempt_no` 每应用自增，`decision_code` 走
`CONTENT_POLICY / SECURITY / DESIGN / FUNCTIONAL / OTHER`（WorkBuddy 驳回原因的收敛词表）。
同一应用同刻**至多一条** `PENDING`（部分唯一索引）。

### 4.3 配置版本（`deploy_app_capability_revision`）

不可变、append-only：`revision_no` 每应用自增，`snapshot` 是完整六维配置的 JSONB 文档，
`snapshot_sha256` 固定字节。审核、发布、回滚、导出（WorkBuddy 的「导出配置 JSON」）都读它。
`source ∈ (DRAFT, REVIEW, PUBLISH)` 说明这份快照由哪一步产生。

### 4.4 企业授权（IAM）

```
PENDING_AUTHORIZATION ──enterprise admin confirms──▶ ACTIVE ──publisher terminates──▶ PENDING_TERMINATION
        │                                              │                                    │
     cancel                                          expire                            confirm
        ▼                                              ▼                                    ▼
    CANCELLED                                      TERMINATED                          TERMINATED
```

- 授权模式 `ALL_MEMBERS`（全员制，无席位）| `SEAT`（席位制，`seat_count` 必填）。
- 授权时长 `duration_months ∈ (3, 6, 12)`（WorkBuddy 当前 3/6/12）。
- 授权链接：一次性、长期有效（`expires_at` 可空）、**只存 `token_hash`**，用后 `CONSUMED`。
- 席位 `iam_application_enterprise_seat`：`(order_id, user_id)` 活跃唯一；活跃席位 ≤ `seat_count`
  由服务层在 `SELECT ... FOR UPDATE` 的事务里保证（数据库无法表达聚合上限，文档化该约束）。
- 到期：`expires_at <= now()` 的 `ACTIVE` 订单由后台任务转 `TERMINATED` 并记事件。

## 5. 对外的接口面

| 操作 | 路径 | 权限点 |
| --- | --- | --- |
| 读配置 | `GET /app/v3/api/apps/{appId}/capability` | `deploy.apps.read` |
| 整体替换配置 | `PUT /app/v3/api/apps/{appId}/capability` | `deploy.apps.write` |
| 提交审核 | `POST /app/v3/api/apps/{appId}/capability/submit` | `deploy.apps.write` |
| 审核列表 | `GET /app/v3/api/apps/{appId}/capability/reviews` | `deploy.apps.read` |
| 审核裁决 | `PUT /app/v3/api/apps/{appId}/capability/reviews/{reviewId}/decision` | `deploy.appCapabilities.approve` |
| 版本列表/单条 | `GET /app/v3/api/apps/{appId}/capability/revisions[/{revisionId}]` | `deploy.apps.read` |

**为什么「提交」是命令而不是 `POST .../reviews`**：在 `API_SPEC.md` §15.4 的 operation 语义里，
`POST` 到集合就是「创建资源」，必须带 `Idempotency-Key` 并在重放时返回同一行。提交的幂等性
已经由「每应用同刻至多一条 pending」唯一索引表达，重试得到的是 `409` 而不是同一行；把它写成
创建会迫使实现引入一个并不需要的幂等键表。`/capability/submit` 是命令路径，语义与
`/apps/{appId}/activate` 一致。

**为什么裁决是 `PUT .../decision`**：一次审核只有一个裁决，`decision` 是审核单上的单值子资源；
`PUT` 表达「把这个裁决置为 X」，重复提交同一裁决幂等。驳回必须带 `decisionCode`。

IAM 侧企业单（`/backend/v3/api/iam/...`）：`applications/{appId}/enterprise_orders`（create/list/
retrieve/cancel/terminate）+ `enterprise_orders/{orderId}/seats`（list/assign/release）+
`enterprise_order_links/{token}/authorize|terminate`（企业管理员确认）。

**为什么是「整体替换 + 定版」而不是逐子资源 CRUD**：六维配置是一个**有内部引用**的文档（默认工作
模式、场景绑定、精选位），逐条 PATCH 会在中途产生非法组合（例如先删默认模式再加新默认模式，中间
态没有默认）。`PUT` 在单事务里 replace 全部子行，校验通过才落库，天然满足
`API_SPEC.md` §15.4 的 update 语义，也让「导出/导入配置 JSON」成为同一个契约。

## 6. 与既有模型的边界（明确不做的事）

- **不复制** `deploy_app_variant` / `deploy_app_source_spec` 的客户端路由语义：那回答「这个端加载哪份
  源码」，本模型回答「用户在这个应用里看到什么」。
- **不复制** `deploy_app_revision`（部署修订）与 `deploy_app_environment`（环境）：本模型的 revision
  是**配置**快照，与部署修订是两条独立的版本线。
- **不内建** OAuth 服务端：`oauth_client_ref` 指向 IAM 的安装凭据；client_secret 永不出现在本仓。
- **不内建** 资产目录：专家/技能/连接器/案例的权威在各自模块，本模型只存引用 + 展示快照。
- **不做** 运行时编排：模式、工具、场景的**执行**由宿主运行时负责，本模型只提供配置事实。

## 7. 校验与失败语义

| 场景 | 结果 |
| --- | --- |
| `distribution_scope = ENTERPRISE` 但无任何企业单 | 允许（等待授权），但 `publish` 时给出明确提示 |
| 默认工作模式不存在 / 已禁用 | `40003`，字段级 `defaultWorkModeKey` |
| 场景绑定的 asset_ref 非法（kind/module 不配对） | `40003`，字段级 `scenes[i].assets[j]` |
| 送审时配置为空（无工作模式且无场景） | `40003`，`capability.empty` |
| 对 `IN_REVIEW` 快照做裁决时快照已被替换 | `40901`（乐观并发，revision 不匹配） |
| 删除仍被 `default_work_mode_key` 引用的模式 | 同事务清指针，不返回错误 |
| 席位超配 | `40901`，`seat_quota_exceeded` |
| 企业单重复（同应用同企业仍有活跃单） | `40901` |
| 授权链接已使用/已撤销 | `40901` |

## 8. 迁移与兼容

- 新增表全部走 `database/ddl/baseline/postgres/0001_deploy_baseline.sql` + `0024_*` 迁移双轨，
  已初始化库靠迁移收敛。
- `deploy_app_capability` 对既有应用**不预置行**：没有配置的应用读接口返回「默认配置文档」
  （`capability_status = DRAFT`，空六维），首次 `PUT` 才落行。这样上线不需要回填。
- `snapshot` JSONB 带 `schemaVersion`，未来字段演进靠它做读时升级，不改表结构。
