/**
 * The platform catalog for domain-ownership verification files.
 *
 * Mirrors the server's single source
 * (`sdkwork-deployments/crates/sdkwork-deploy-contract/src/verification_file_catalog.rs`).
 * Every platform issues a file with a fixed shape — WeChat `MP_verify_<token>.txt`,
 * Alipay/ByteDance `<token>.txt`, Google `<token>.html`, Apple's fixed association
 * file — so the console can prefill the name the platform will actually fetch
 * instead of letting an operator register one it never will.
 */

import type { DeliveryMessageKey } from "./i18n.ts";

export type VerificationPlatform =
  | "WECHAT_OFFICIAL_ACCOUNT"
  | "WECHAT_MINI_PROGRAM"
  | "ALIPAY"
  | "BYTEDANCE"
  | "APPLE"
  | "GOOGLE"
  | "CUSTOM";

export type VerificationFileExtension = "txt" | "html" | "xml";

export interface VerificationPlatformSpec {
  platform: VerificationPlatform;
  labelKey: DeliveryMessageKey;
  /** File name the platform issues, e.g. `MP_verify_<token>.txt`. */
  fileNameHint: string;
  fixedFileName?: string;
  tokenPrefix?: string;
  extension: VerificationFileExtension;
  /** Where the operator obtains the file. */
  issuedByKey: DeliveryMessageKey;
}

export const VERIFICATION_PLATFORMS: readonly VerificationPlatformSpec[] = [
  {
    platform: "WECHAT_OFFICIAL_ACCOUNT",
    labelKey: "verificationFilePlatformWECHAT_OFFICIAL_ACCOUNT",
    fileNameHint: "MP_verify_<token>.txt",
    tokenPrefix: "MP_verify_",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByWECHAT_OFFICIAL_ACCOUNT",
  },
  {
    platform: "WECHAT_MINI_PROGRAM",
    labelKey: "verificationFilePlatformWECHAT_MINI_PROGRAM",
    fileNameHint: "MP_verify_<token>.txt",
    tokenPrefix: "MP_verify_",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByWECHAT_MINI_PROGRAM",
  },
  {
    platform: "ALIPAY",
    labelKey: "verificationFilePlatformALIPAY",
    fileNameHint: "<token>.txt",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByALIPAY",
  },
  {
    platform: "BYTEDANCE",
    labelKey: "verificationFilePlatformBYTEDANCE",
    fileNameHint: "<token>.txt",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByBYTEDANCE",
  },
  {
    platform: "APPLE",
    labelKey: "verificationFilePlatformAPPLE",
    fileNameHint: "apple-developer-domain-association.txt",
    fixedFileName: "apple-developer-domain-association.txt",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByAPPLE",
  },
  {
    platform: "GOOGLE",
    labelKey: "verificationFilePlatformGOOGLE",
    fileNameHint: "<token>.html",
    extension: "html",
    issuedByKey: "verificationFileIssuedByGOOGLE",
  },
  {
    platform: "CUSTOM",
    labelKey: "verificationFilePlatformCUSTOM",
    fileNameHint: "<file>.txt | <file>.html | <file>.xml",
    extension: "txt",
    issuedByKey: "verificationFileIssuedByCUSTOM",
  },
];

export const VERIFICATION_FILE_EXTENSIONS: readonly VerificationFileExtension[] = [
  "txt",
  "html",
  "xml",
];

const NAME_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*\.(txt|html|xml)$/;

export function verificationPlatform(code: string): VerificationPlatformSpec | undefined {
  return VERIFICATION_PLATFORMS.find((spec) => spec.platform === code);
}

/** Whether the edge will serve `fileName` (single ASCII segment, known extension). */
export function isServableVerificationFileName(fileName: string): boolean {
  const trimmed = fileName.trim();
  return NAME_PATTERN.test(trimmed) && trimmed.length <= 128;
}

export function verificationFileExtension(fileName: string): string {
  const trimmed = fileName.trim();
  const dot = trimmed.lastIndexOf(".");
  return dot < 0 ? "" : trimmed.slice(dot + 1).toLowerCase();
}

/** Whether the platform's own file can have `extension`. */
export function platformAcceptsExtension(platform: string, extension: string): boolean {
  const spec = verificationPlatform(platform);
  if (!spec) return false;
  if (spec.platform === "CUSTOM") {
    return (VERIFICATION_FILE_EXTENSIONS as readonly string[]).includes(extension);
  }
  return spec.extension === extension;
}

/** The file name the platform will fetch for the token it issued. */
export function suggestVerificationFileName(
  platform: string,
  token: string,
): string | undefined {
  const spec = verificationPlatform(platform);
  if (!spec) return undefined;
  if (spec.fixedFileName) return spec.fixedFileName;
  const trimmed = token.trim();
  if (!trimmed) return undefined;
  const extension = verificationFileExtension(trimmed);
  if (isServableVerificationFileName(trimmed) && platformAcceptsExtension(platform, extension)) {
    return trimmed;
  }
  const stem = spec.tokenPrefix ? `${spec.tokenPrefix}${trimmed}` : trimmed;
  return `${stem}.${spec.extension}`;
}

/** The public URL the platform (or a browser) fetches the file from. */
export function verificationFilePublicUrl(apexHostname: string, fileName: string): string {
  return `https://${apexHostname}/${fileName}`;
}
