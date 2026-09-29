import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";

const enUs = {
  "page.eyebrow": "operations",
  "page.title": "Source specs",
  "page.description":
    "Which uploaded source serves each kind of client, across every application. The same rows the application console shows one app at a time, laid out as a ledger.",
  "toolbar.environment": "Environment",
  "toolbar.refresh": "Reload",
  "toolbar.prev": "Previous page",
  "toolbar.next": "Next page",
  "toolbar.page": "Page {page}",
  "toolbar.loading": "Reading source specs…",
  "summary.applications": "Applications",
  "summary.specs": "Specs",
  "summary.bound": "With a source",
  "summary.silent": "Producing no rule",
  "app.specCount": "{count} spec(s)",
  "app.noSpecs": "No source spec declared in this environment.",
  "spec.appDefault": "app fallback",
  "spec.disabled": "disabled",
  "spec.bound": "source attached",
  "spec.empty": "no source",
  "route.declaredDefault": "default",
  "route.fallback": "fallback {rank}",
  "route.serves": "serving",
  "route.answers": "answers",
  "route.dark": "not serving",
  "route.none": "No client class is routed to this source.",
  "readOnly.title": "Read-only view",
  "readOnly.note":
    "Declaring a source spec, changing which one is a client's default, and attaching a source are application-console actions. This page reports the outcome those actions produced.",
  "error.load": "Unable to read the source specs: {message}",
  "error.partial": "Some applications could not be read and are missing from this page: {apps}",
  "error.noPort": "The source-specs reader is not configured for this Admin session.",
  "empty": "No application is visible to this session.",
} as const;

const zhCn: Record<keyof typeof enUs, string> = {
  "page.eyebrow": "运维",
  "page.title": "源码规格",
  "page.description":
    "跨应用查看每一类客户端由哪份已上传源码服务。与控制台「源码规格」是同一批事实，只是从「一次一个应用」换成台账。",
  "toolbar.environment": "环境",
  "toolbar.refresh": "重新加载",
  "toolbar.prev": "上一页",
  "toolbar.next": "下一页",
  "toolbar.page": "第 {page} 页",
  "toolbar.loading": "正在读取源码规格…",
  "summary.applications": "应用",
  "summary.specs": "规格",
  "summary.bound": "已绑定来源",
  "summary.silent": "不产出规则",
  "app.specCount": "{count} 条规格",
  "app.noSpecs": "该环境下没有声明任何源码规格。",
  "spec.appDefault": "应用级兜底",
  "spec.disabled": "已停用",
  "spec.bound": "已绑定来源",
  "spec.empty": "无来源",
  "route.declaredDefault": "默认",
  "route.fallback": "回退 {rank}",
  "route.serves": "服务中",
  "route.answers": "应答",
  "route.dark": "不服务",
  "route.none": "该源码没有声明服务任何一端。",
  "readOnly.title": "只读视图",
  "readOnly.note":
    "声明规格、改某端的默认、绑定来源都是控制台的动作。本页只呈现这些动作产生的结果。",
  "error.load": "读取源码规格失败：{message}",
  "error.partial": "部分应用读取失败，未出现在本页：{apps}",
  "error.noPort": "当前会话未配置源码规格读取端口。",
  "empty": "当前会话看不到任何应用。",
};

export type SourceSpecsMessageKey = keyof typeof enUs;

export function translateSourceSpecs(
  locale: DeploymentsLocale,
  key: SourceSpecsMessageKey,
  values: Record<string, string | number> = {},
): string {
  const catalog = locale === "zh-CN" ? zhCn : enUs;
  return Object.entries(values).reduce(
    (message, [name, value]) => message.replaceAll(`{${name}}`, String(value)),
    catalog[key],
  );
}
