# Deploy API Map

Generated from the authored OpenAPI contracts by
`node tools/agent-skills-sdk/generate-api-map.mjs`. Do not hand-edit.

Auth on every operation: `Authorization: Bearer <authToken>` plus
`Access-Token: <accessToken>` (`authMode: "dual-token"`). The `iam` surface is
credential-entry only and accepts the public application access token.

| Surface | Operation | Method | Path | Summary |
| --- | --- | --- | --- | --- |
| app | `appDatabaseMigrations.create` | POST | `/app/v3/api/apps/{appId}/database_profiles/{profileId}/migrations` | Add a versioned migration definition to a database profile |
| app | `appDatabaseMigrations.list` | GET | `/app/v3/api/apps/{appId}/database_profiles/{profileId}/migrations` | List versioned migration definitions of a database profile |
| app | `appDatabaseMigrations.retrieve` | GET | `/app/v3/api/apps/{appId}/database_profiles/{profileId}/migrations/{migrationId}` | Retrieve a versioned migration definition |
| app | `appDatabaseProfiles.create` | POST | `/app/v3/api/apps/{appId}/database_profiles` | Declare the database structure contract of an app |
| app | `appDatabaseProfiles.list` | GET | `/app/v3/api/apps/{appId}/database_profiles` | List the database structure contracts of an app |
| app | `appDatabaseProfiles.retrieve` | GET | `/app/v3/api/apps/{appId}/database_profiles/{profileId}` | Retrieve a database structure contract |
| app | `appDatabaseProfiles.update` | PATCH | `/app/v3/api/apps/{appId}/database_profiles/{profileId}` | Update a database structure contract |
| app | `appEnvironments.create` | POST | `/app/v3/api/apps/{appId}/environments/{environmentId}/promotions` | Promote a release into the environment |
| app | `appEnvironments.list` | GET | `/app/v3/api/apps/{appId}/environments/{environmentId}/promotions` | List the immutable promotion history of an environment |
| app | `appEnvironments.retrieve` | GET | `/app/v3/api/apps/{appId}/environments/{environmentId}` | Retrieve an environment |
| app | `appEnvironments.update` | PATCH | `/app/v3/api/apps/{appId}/environments/{environmentId}` | Update an environment (name, approval requirement, status) |
| app | `apps.activate` | POST | `/app/v3/api/apps/{appId}/activate` | 激活应用 |
| app | `apps.composition.update` | PUT | `/app/v3/api/apps/{appId}/composition` | Replace and publish the complete live app composition |
| app | `apps.create` | POST | `/app/v3/api/apps` | Create a tenant app |
| app | `apps.domains.list` | GET | `/app/v3/api/apps/{appId}/domains` | List an app's publishing domains |
| app | `apps.envVariables.create` | POST | `/app/v3/api/apps/{appId}/env_variables` | 创建环境变量 |
| app | `apps.envVariables.list` | GET | `/app/v3/api/apps/{appId}/env_variables` | 获取环境变量列表 |
| app | `apps.healthChecks.create` | POST | `/app/v3/api/apps/{appId}/health_checks` | 创建健康检查 |
| app | `apps.healthChecks.list` | GET | `/app/v3/api/apps/{appId}/health_checks` | 获取健康检查配置 |
| app | `apps.list` | GET | `/app/v3/api/apps` | List the apps the caller may reach |
| app | `apps.pause` | POST | `/app/v3/api/apps/{appId}/pause` | 暂停应用 |
| app | `apps.remix.retrieve` | GET | `/app/v3/api/apps/{appId}/remix` | Retrieve the remix state and download instructions |
| app | `apps.remix.update` | PUT | `/app/v3/api/apps/{appId}/remix` | Enable or disable remix |
| app | `apps.resourceLinks.delete` | DELETE | `/app/v3/api/apps/{appId}/resource_links/{resourceLinkId}` | Retire an account binding |
| app | `apps.resourceLinks.list` | GET | `/app/v3/api/apps/{appId}/resource_links` | List an application's account bindings |
| app | `apps.resourceLinks.update` | PUT | `/app/v3/api/apps/{appId}/resource_links` | Bind or replace an application's account binding |
| app | `apps.retrieve` | GET | `/app/v3/api/apps/{appId}` | Retrieve a tenant app |
| app | `apps.source.delete` | DELETE | `/app/v3/api/apps/{appId}/source` | Retire the application's remix source |
| app | `apps.source.retrieve` | GET | `/app/v3/api/apps/{appId}/source` | Retrieve the application's remix source |
| app | `apps.source.update` | PUT | `/app/v3/api/apps/{appId}/source` | Replace the application's remix source |
| app | `apps.source.validate` | POST | `/app/v3/api/apps/{appId}/source/validate` | Validate the application's Git source repository |
| app | `apps.sourceSpecs.bindSource` | PUT | `/app/v3/api/apps/{appId}/source_specs/{specId}/source` | Register the uploaded source behind a spec |
| app | `apps.sourceSpecs.create` | POST | `/app/v3/api/apps/{appId}/source_specs` | Declare a source spec for an app |
| app | `apps.sourceSpecs.delete` | DELETE | `/app/v3/api/apps/{appId}/source_specs/{specId}` | Delete a source spec |
| app | `apps.sourceSpecs.list` | GET | `/app/v3/api/apps/{appId}/source_specs` | List an app's source specs |
| app | `apps.sourceSpecs.retrieve` | GET | `/app/v3/api/apps/{appId}/source_specs/{specId}` | Retrieve a source spec |
| app | `apps.sourceSpecs.update` | PATCH | `/app/v3/api/apps/{appId}/source_specs/{specId}` | Update a source spec |
| app | `apps.update` | PATCH | `/app/v3/api/apps/{appId}` | Update a tenant app |
| app | `appTemplates.create` | POST | `/app/v3/api/app_templates` | Publish an app as a template listing |
| app | `appTemplates.delete` | DELETE | `/app/v3/api/app_templates/{templateUuid}` | Withdraw the caller's app template |
| app | `appTemplates.list` | GET | `/app/v3/api/app_templates` | List the caller's app templates |
| app | `appTemplates.retrieve` | GET | `/app/v3/api/app_templates/{templateUuid}` | Retrieve the caller's app template |
| app | `appTemplates.submit` | POST | `/app/v3/api/app_templates/{templateUuid}/submit` | Submit an app template for review |
| app | `appTemplates.update` | PATCH | `/app/v3/api/app_templates/{templateUuid}` | Update the caller's app template listing |
| app | `appTemplateVersions.create` | POST | `/app/v3/api/app_templates/{templateUuid}/versions` | Add a version to the caller's app template |
| app | `appTemplateVersions.list` | GET | `/app/v3/api/app_templates/{templateUuid}/versions` | List versions of the caller's app template |
| app | `artifacts.create` | POST | `/app/v3/api/artifacts` | Register a Drive-backed deployment artifact |
| app | `artifacts.delete` | DELETE | `/app/v3/api/artifacts/{artifactId}` | 保留并下架制品 |
| app | `artifacts.list` | GET | `/app/v3/api/artifacts` | 获取租户制品列表 |
| app | `artifacts.retrieve` | GET | `/app/v3/api/artifacts/{artifactId}` | 获取制品详情 |
| app | `builds.create` | POST | `/app/v3/api/apps/{appId}/builds` | Trigger a governed build |
| app | `builds.list` | GET | `/app/v3/api/apps/{appId}/builds` | List builds of an app |
| app | `builds.retrieve` | GET | `/app/v3/api/apps/{appId}/builds/{buildId}` | Retrieve a build |
| app | `builds.update` | PATCH | `/app/v3/api/apps/{appId}/builds/{buildId}/state` | Report a build state transition from the build runner |
| app | `buildTemplates.create` | POST | `/app/v3/api/build_templates` | Create a governed build template |
| app | `buildTemplates.list` | GET | `/app/v3/api/build_templates` | List tenant build templates |
| app | `buildTemplates.retrieve` | GET | `/app/v3/api/build_templates/{buildTemplateId}` | Retrieve a build template |
| app | `channels.create` | POST | `/app/v3/api/apps/{appId}/channels/{channelId}/promotions` | Promote a release into a channel with immutable rollout history |
| app | `channels.list` | GET | `/app/v3/api/apps/{appId}/channels` | List release channels of an app |
| app | `channels.retrieve` | GET | `/app/v3/api/apps/{appId}/channels/{channelId}` | Retrieve a release channel |
| app | `channels.rollouts.list` | GET | `/app/v3/api/apps/{appId}/channels/{channelId}/rollouts` | List immutable rollout history of a channel |
| app | `deployments.create` | POST | `/app/v3/api/apps/{appId}/deployments` | Create a deployment against a typed target |
| app | `deployments.list` | GET | `/app/v3/api/apps/{appId}/deployments` | List deployments of an app |
| app | `deployments.retrieve` | GET | `/app/v3/api/apps/{appId}/deployments/{deploymentId}` | Retrieve a deployment |
| app | `marketplaceTemplates.list` | GET | `/app/v3/api/marketplace/templates` | Browse the tenant app template marketplace |
| app | `marketplaceTemplates.retrieve` | GET | `/app/v3/api/marketplace/templates/{templateUuid}` | Retrieve a marketplace app template |
| app | `packages.create` | POST | `/app/v3/api/apps/{appId}/packages` | Register an immutable deployment package |
| app | `packages.list` | GET | `/app/v3/api/apps/{appId}/packages` | List deployment packages of an app |
| app | `packages.retrieve` | GET | `/app/v3/api/apps/{appId}/packages/{packageId}` | Retrieve a deployment package |
| app | `platformTargets.create` | POST | `/app/v3/api/apps/{appId}/platform_targets` | Create a platform target |
| app | `platformTargets.list` | GET | `/app/v3/api/apps/{appId}/platform_targets` | List platform targets of an app |
| app | `platformTargets.retrieve` | GET | `/app/v3/api/apps/{appId}/platform_targets/{platformTargetId}` | Retrieve a platform target |
| app | `releases.create` | POST | `/app/v3/api/apps/{appId}/releases` | Create an immutable semantic version release |
| app | `releases.list` | GET | `/app/v3/api/apps/{appId}/releases` | List app releases |
| app | `releases.retrieve` | GET | `/app/v3/api/apps/{appId}/releases/{releaseId}` | Retrieve an app release |
| app | `signingIdentities.create` | POST | `/app/v3/api/signing_identities` | Create a signing identity (secret reference only) |
| app | `signingIdentities.list` | GET | `/app/v3/api/signing_identities` | List tenant signing identities |
| app | `signingIdentities.retrieve` | GET | `/app/v3/api/signing_identities/{signingIdentityId}` | Retrieve a signing identity |
| app | `sourceRepositories.create` | POST | `/app/v3/api/apps/{appId}/source_repositories` | Bind a Git source repository to an app |
| app | `sourceRepositories.list` | GET | `/app/v3/api/apps/{appId}/source_repositories` | List source repositories of an app |
| app | `sourceRepositories.retrieve` | GET | `/app/v3/api/apps/{appId}/source_repositories/{sourceRepositoryId}` | Retrieve a source repository |
| app | `templateCategories.list` | GET | `/app/v3/api/template_categories` | List template categories |
| app | `uploadSessions.cancel` | POST | `/app/v3/api/upload_sessions/{uploadSessionId}/cancel` | 取消上传会话 |
| app | `uploadSessions.complete` | POST | `/app/v3/api/upload_sessions/{uploadSessionId}/complete` | 完成上传会话 |
| app | `uploadSessions.create` | POST | `/app/v3/api/upload_sessions` | 创建 Drive-backed 包上传会话 |
| app | `uploadSessions.retrieve` | GET | `/app/v3/api/upload_sessions/{uploadSessionId}` | 获取上传会话 |
| app | `usageEvents.list` | GET | `/app/v3/api/usage_events` | List tenant usage metering events |
| backend | `apps.identityProvision` | POST | `/backend/v3/api/apps/{appId}/identity_provision` | 重试应用身份开通 |
| backend | `appTemplates.delete` | DELETE | `/backend/v3/api/app_templates/{templateUuid}` | 删除应用模板 |
| backend | `appTemplates.list` | GET | `/backend/v3/api/app_templates` | 获取应用模板列表（全状态） |
| backend | `appTemplates.retrieve` | GET | `/backend/v3/api/app_templates/{templateUuid}` | 获取应用模板详情 |
| backend | `appTemplates.update` | PATCH | `/backend/v3/api/app_templates/{templateUuid}` | 审核或调整应用模板 |
| backend | `appTemplateVersions.list` | GET | `/backend/v3/api/app_templates/{templateUuid}/versions` | 获取应用模板版本列表 |
| backend | `appTemplateVersions.retrieve` | GET | `/backend/v3/api/app_templates/{templateUuid}/versions/{versionUuid}` | 获取应用模板版本详情 |
| backend | `auditLogs.list` | GET | `/backend/v3/api/audit_logs` | 获取审计日志列表 |
| backend | `clusters.create` | POST | `/backend/v3/api/node_clusters` | 创建节点集群 |
| backend | `clusters.list` | GET | `/backend/v3/api/node_clusters` | 获取节点集群列表 |
| backend | `clusters.update` | PUT | `/backend/v3/api/node_clusters/{clusterId}` | 更新节点集群 |
| backend | `configs.create` | POST | `/backend/v3/api/nginx/configs` | 创建 Nginx 配置 |
| backend | `configs.deploy` | POST | `/backend/v3/api/nginx/configs/{configId}/deploy` | 部署 Nginx 配置 |
| backend | `configs.list` | GET | `/backend/v3/api/nginx/configs` | 获取 Nginx 配置列表 |
| backend | `configs.retrieve` | GET | `/backend/v3/api/nginx/configs/{configId}` | 获取 Nginx 配置详情 |
| backend | `configs.update` | PUT | `/backend/v3/api/nginx/configs/{configId}` | 更新 Nginx 配置 |
| backend | `configs.validate` | POST | `/backend/v3/api/nginx/configs/{configId}/validate` | 校验 Nginx 配置 |
| backend | `create` | POST | `/backend/v3/api/source_events` | Ingest a Git webhook push event and trigger builds |
| backend | `entitlements.list` | GET | `/backend/v3/api/entitlements` | List Commerce-backed entitlement projections |
| backend | `list` | GET | `/backend/v3/api/source_events` | List ingested Git webhook events |
| backend | `run` | POST | `/backend/v3/api/retention/run` | Apply platform retention policies |
| backend | `runtime.reload` | POST | `/backend/v3/api/nginx/reload` | 热加载 Nginx |
| backend | `runtime.retrieve` | GET | `/backend/v3/api/nginx/status` | 获取 Nginx 状态 |
| backend | `servers.create` | POST | `/backend/v3/api/servers` | 注册服务器节点 |
| backend | `servers.list` | GET | `/backend/v3/api/servers` | 获取服务器节点列表 |
| backend | `servers.update` | PUT | `/backend/v3/api/servers/{serverId}` | 更新服务器节点 |
| backend | `signingIdentityHealth.list` | GET | `/backend/v3/api/signing_identity_health` | List signing identity expiry health |
| backend | `templateCategories.create` | POST | `/backend/v3/api/template_categories` | 创建模板分类 |
| backend | `templateCategories.delete` | DELETE | `/backend/v3/api/template_categories/{categoryUuid}` | 删除模板分类 |
| backend | `templateCategories.list` | GET | `/backend/v3/api/template_categories` | 获取模板分类列表 |
| backend | `templateCategories.retrieve` | GET | `/backend/v3/api/template_categories/{categoryUuid}` | 获取模板分类详情 |
| backend | `templateCategories.update` | PATCH | `/backend/v3/api/template_categories/{categoryUuid}` | 更新模板分类 |
| backend | `tlsAccounts.create` | POST | `/backend/v3/api/tls/accounts` | Create an ACME account binding |
| backend | `tlsAccounts.list` | GET | `/backend/v3/api/tls/accounts` | List ACME accounts of the tenant |
| backend | `tlsOrders.advance` | POST | `/backend/v3/api/tls/orders/{orderId}/advance` | Advance a certificate order one canonical state machine step |
| backend | `tlsOrders.challengeResult` | POST | `/backend/v3/api/tls/orders/{orderId}/challenge_result` | Record a challenge validation result |
| backend | `tlsOrders.create` | POST | `/backend/v3/api/tls/orders/{orderId}/versions` | Store the issued certificate version and complete the order |
| backend | `tlsOrders.fail` | POST | `/backend/v3/api/tls/orders/{orderId}/fail` | Fail a certificate order with a stable error code |

Operations: 125.
