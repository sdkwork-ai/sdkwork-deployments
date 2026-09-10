export type DeploymentsEnvironment = "development" | "test" | "staging" | "demo" | "production";
export type DeploymentsLocale = "en-US" | "zh-CN";

export interface DeploymentsRuntimeConfig {
  activeLocales: DeploymentsLocale[];
  appApiBaseUrl: string;
  appbaseAppApiBaseUrl: string;
  backendApiBaseUrl: string;
  defaultLocale: DeploymentsLocale;
  deploymentProfile: "standalone" | "cloud";
  driveAppApiBaseUrl: string;
  environment: DeploymentsEnvironment;
  fallbackLocale: DeploymentsLocale;
  supportedLocales: DeploymentsLocale[];
}

export async function loadDeploymentsRuntimeConfig(fetcher: typeof fetch = fetch): Promise<DeploymentsRuntimeConfig> {
  const response = await fetcher("/runtime-env.json", { cache: "no-store", credentials: "same-origin" });
  if (!response.ok) throw new Error(`Runtime configuration failed with HTTP ${response.status}`);
  return parseDeploymentsRuntimeConfig(await response.json());
}

export function parseDeploymentsRuntimeConfig(value: unknown): DeploymentsRuntimeConfig {
  if (!record(value)) throw new Error("Runtime configuration must be an object");
  const environment = enumValue(value.environment, ["development", "test", "staging", "demo", "production"] as const, "environment");
  const supportedLocales = locales(value.supportedLocales, "supportedLocales");
  const activeLocales = locales(value.activeLocales, "activeLocales");
  const defaultLocale = enumValue(value.defaultLocale, ["en-US", "zh-CN"] as const, "defaultLocale");
  const fallbackLocale = enumValue(value.fallbackLocale, ["en-US", "zh-CN"] as const, "fallbackLocale");
  if (!supportedLocales.includes(defaultLocale) || !supportedLocales.includes(fallbackLocale) || activeLocales.some((locale) => !supportedLocales.includes(locale))) throw new Error("Locale configuration is inconsistent");
  return { activeLocales, appApiBaseUrl: url(value.appApiBaseUrl, "appApiBaseUrl", environment), appbaseAppApiBaseUrl: url(value.appbaseAppApiBaseUrl, "appbaseAppApiBaseUrl", environment), backendApiBaseUrl: url(value.backendApiBaseUrl, "backendApiBaseUrl", environment), defaultLocale, deploymentProfile: enumValue(value.deploymentProfile, ["standalone", "cloud"] as const, "deploymentProfile"), driveAppApiBaseUrl: url(value.driveAppApiBaseUrl, "driveAppApiBaseUrl", environment), environment, fallbackLocale, supportedLocales };
}

export function resolveDeploymentsLocale(config: DeploymentsRuntimeConfig, preferredLocales: readonly string[]): DeploymentsLocale {
  for (const preferred of preferredLocales) {
    const normalized = preferred.toLowerCase().startsWith("zh") ? "zh-CN" : preferred.toLowerCase().startsWith("en") ? "en-US" : undefined;
    if (normalized && config.activeLocales.includes(normalized)) return normalized;
  }
  return config.activeLocales.includes(config.defaultLocale) ? config.defaultLocale : config.fallbackLocale;
}

function url(value: unknown, field: string, environment: DeploymentsEnvironment): string { if (typeof value !== "string" || !value.trim()) throw new Error(`${field} is required`); const parsed = new URL(value); if (!["http:", "https:"].includes(parsed.protocol)) throw new Error(`${field} must use HTTP or HTTPS`); if (environment === "production" && ["127.0.0.1", "localhost", "::1"].includes(parsed.hostname)) throw new Error(`${field} cannot use a loopback host in production`); return parsed.toString().replace(/\/$/, ""); }
function locales(value: unknown, field: string): DeploymentsLocale[] { if (!Array.isArray(value) || value.length === 0) throw new Error(`${field} is required`); return [...new Set(value.map((locale) => enumValue(locale, ["en-US", "zh-CN"] as const, field)))]; }
function enumValue<const T extends readonly string[]>(value: unknown, allowed: T, field: string): T[number] { if (typeof value !== "string" || !allowed.includes(value)) throw new Error(`${field} is invalid`); return value as T[number]; }
function record(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
