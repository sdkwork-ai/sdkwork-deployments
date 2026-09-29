/**
 * 把**应用面**的 SDK 投影成 backend-admin「源码规格」视图的读取端口。
 *
 * ## 为什么适配器在应用壳里
 *
 * backend-admin 的 SDK 没有源码规格面（`admin-core/src/source-specs/port.tsx`
 * 列了它的全部成员），所以这个视图的数据只能来自应用面客户端。谁来做这个投影是
 * 一个边界问题：
 *
 * - 放进 `admin-core` ⇒ 那个包要多引 `@sdkwork/deployments-app-sdk`，而它是
 *   `tools/materialize_deployments_pc.mjs` 生成 `package.json` 的包，改依赖得
 *   同时在台账与生成物两处动手，且要跑一次跨仓 `pnpm install`。
 * - 放进**应用壳** ⇒ 壳本来就已经拿到 `runtime.clients.deploy`，也已经为 admin
 *   面把 `runtime.clients.drive` 投影成 `sandboxExplorerPort` 了 —— 同一种活，
 *   同一个地方。两个面之间的边界翻译属于组合根，不属于任何一面。
 *
 * 这也让「将来 backend-admin 补上源码规格端点」变成一次纯壳层替换：本文件换成
 * 走 backend 客户端即可，`admin-core` 一个字都不用改。
 *
 * ## 口径（词表 + 翻译）也在这里定
 *
 * 行里同时带原始枚举值与本地化文案。原始值是筛选要比较的东西，文案来自**拥有该
 * 词表的那个能力包**（`@sdkwork/deployments-pc-console-publishing` 的
 * `*_LABEL_KEYS` + 它自己的翻译目录）。admin 面因此不需要第二份词表 ——
 * 契约加一个 `runtimeTarget` 时只有一处要跟。
 *
 * ## 路由结论不在这里算
 *
 * 「哪一档真正应答」由 `projectSourceSpecLedger`（发布能力的路由内核，带单测）
 * 给出。本文件只做映射：应用 → 行。再算一遍就是第二份实现，而两份实现在
 * 「`preference === 0` 只是习惯、`EMPTY` 的规格不产出规则」这两点上都会各自
 * 看起来对，然后悄悄分叉。
 */
import type {
  AppPublishEnvironment,
  AppResponse,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";
import {
  APP_PUBLISH_ENVIRONMENTS,
  APP_STATUS_LABEL_KEYS,
  CLIENT_ARCHITECTURE_LABEL_KEYS,
  CLIENT_CLASS_LABEL_KEYS,
  RUNTIME_TARGET_LABEL_KEYS,
  SOURCE_BINDING_LABEL_KEYS,
  SOURCE_SPEC_HANDLER_LABEL_KEYS,
  SOURCE_SPEC_STATUS_LABEL_KEYS,
  projectSourceSpecLedger,
  publishingTranslator,
  resolveAppEnvironment,
  type SourceSpecLedgerEntry,
} from "@sdkwork/deployments-pc-console-publishing";
import type {
  SourceSpecsAdminApplication,
  SourceSpecsAdminPort,
  SourceSpecsAdminSpec,
} from "@sdkwork/deployments-pc-admin-core";

/**
 * Applications read per page.
 *
 * The page never sends a size of its own — it only asks for a page number — so
 * this is the single source of the number, not one of two that must be kept
 * equal. It is still deliberately small: each application costs a second request
 * for its specs, so this value *is* the request count of one page load, and that
 * cost belongs next to the requests that cause it.
 */
const ADMIN_PAGE_SIZE = 10;

export interface SourceSpecsAdminPortOptions {
  readonly deployClient: SdkworkDeployAppClient
  readonly locale: DeploymentsLocale
}

export function createSourceSpecsAdminPort({
  deployClient,
  locale,
}: SourceSpecsAdminPortOptions): SourceSpecsAdminPort {
  const t = publishingTranslator(locale)

  return {
    environments: APP_PUBLISH_ENVIRONMENTS,
    /**
     * No app to read here, so the usual "the app's own default environment" is
     * unavailable. `production` is the one an operator opening an operations
     * ledger expects to see first; the picker is right next to it either way.
     */
    defaultEnvironment: "production",

    async list({ page, environment }) {
      const apps = await deployClient.app.list({ page, pageSize: ADMIN_PAGE_SIZE })

      // One request per application, all in flight together: 10 round trips to the
      // same origin is the practical ceiling for a page, and serialising them would
      // make the page feel broken. `allSettled` rather than `all` because a single
      // unreadable app must not blank the whole ledger — it is named instead.
      const settled = await Promise.allSettled(
        apps.items.map(async (app): Promise<SourceSpecsAdminApplication> => ({
          appId: app.id,
          appName: app.name,
          // Localised rather than passed through: the contract types this as the
          // `AppStatus` enum, and a status badge reading `ARCHIVED` is a token
          // the console would never show. Same table the console uses.
          appStatusLabel: t(APP_STATUS_LABEL_KEYS[app.appStatus]),
          specs: await loadSpecs(deployClient, app, environment, t),
        })),
      )

      const applications: SourceSpecsAdminApplication[] = []
      const unreadableApplications: string[] = []
      settled.forEach((outcome, index) => {
        if (outcome.status === "fulfilled") applications.push(outcome.value)
        else unreadableApplications.push(apps.items[index]?.name ?? String(index))
      })

      return {
        applications,
        page,
        /**
         * `PageInfo.hasMore` is optional on the contract, and this port's ledger
         * declares it definite so no consumer has to re-handle "unknown". The
         * fallback is the one `normalizeDeploymentsPage` already uses for the
         * `total === undefined` branch — a full page means "probably more" — and
         * it is correct by construction here: this adapter is the caller that
         * chose `ADMIN_PAGE_SIZE`, so `items.length >= ADMIN_PAGE_SIZE` is
         * exactly the question the server left unanswered.
         */
        hasMore: apps.pageInfo.hasMore ?? apps.items.length >= ADMIN_PAGE_SIZE,
        unreadableApplications,
      }
    },
  }
}

async function loadSpecs(
  deployClient: SdkworkDeployAppClient,
  app: AppResponse,
  environment: string,
  t: ReturnType<typeof publishingTranslator>,
): Promise<readonly SourceSpecsAdminSpec[]> {
  // The page hands the environment over as a string (its `select` is driven by
  // the port's own list), while the SDK narrows the query parameter to the
  // contract's closed union. The assertion is the boundary where the app shell
  // takes responsibility for what it put in that select — which is exactly the
  // `APP_PUBLISH_ENVIRONMENTS` list below, i.e. the contract's own members.
  const page = await deployClient.app.sourceSpecs.list(app.id, {
    environment: environment as AppPublishEnvironment,
  })
  // The ledger kernel reads the routing slice; these two add the columns the admin
  // table shows. `resolveAppEnvironment` is deliberately **not** used here: the
  // caller already picked the environment, and substituting the app's own default
  // would silently answer a different question than the one asked.
  const ledger = projectSourceSpecLedger(page.items)
  return ledger.map((entry) => toAdminSpec(entry, t))
}

function toAdminSpec(
  entry: SourceSpecLedgerEntry,
  t: ReturnType<typeof publishingTranslator>,
): SourceSpecsAdminSpec {
  return {
    specId: entry.specId,
    specKey: entry.specKey,
    label: entry.label,
    runtimeTargetLabel: t(RUNTIME_TARGET_LABEL_KEYS[entry.runtimeTarget]),
    clientArchitectureLabel: t(CLIENT_ARCHITECTURE_LABEL_KEYS[entry.clientArchitecture]),
    handlerLabel: t(SOURCE_SPEC_HANDLER_LABEL_KEYS[entry.handler]),
    statusLabel: t(SOURCE_SPEC_STATUS_LABEL_KEYS[entry.status]),
    sourceStatusLabel: t(SOURCE_BINDING_LABEL_KEYS[entry.sourceStatus]),
    disabled: entry.status !== "ACTIVE",
    // 「没有来源 ⇒ 不产出任何路由规则」是契约行为，不是文案推理 —— 路由内核的
    // `specServes` 就是这条规则（`ACTIVE` 且 `BOUND`），这里读它的结果而不是重述
    // 一遍条件。
    silent: !entry.routes.some((route) => route.serves),
    isAppDefault: entry.isAppDefault,
    routes: entry.routes.map((route) => ({
      clientClass: route.clientClass,
      clientClassLabel: t(CLIENT_CLASS_LABEL_KEYS[route.clientClass]),
      preference: route.preference,
      isDeclaredDefault: route.isDeclaredDefault,
      serves: route.serves,
      isEffective: route.isEffective,
    })),
  }
}

/** The environment a ledger should open on for one app; exported for symmetry with the console. */
export { resolveAppEnvironment };
