import { createSdkworkMessageCatalog, useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";

import type { ApplicationDetailMessages } from "../types/application-detail-messages";
import { en } from "./en-US/deployment/app-detail/messages";
import { zh } from "./zh-CN/deployment/app-detail/messages";

export type { ApplicationDetailMessages } from "../types/application-detail-messages";

/**
 * Copy catalog of the deployment-application detail surface.
 *
 * The surface owns its own namespace inside the `deployment` domain, so the
 * deployments console mounts it without adding a fragment, and a second console
 * (Cloud Router, the App Store) mounts it without a key collision. Locale
 * selection still comes from the consumer's global provider.
 */
export const DEPLOYMENTS_APP_DETAIL_I18N = createSdkworkMessageCatalog<ApplicationDetailMessages>({
  defaultLocale: "en-US",
  locales: {
    "en-US": en,
    "zh-CN": zh,
  },
  namespace: "deployments.appDetail",
});

/** The catalog of the current locale. */
export function useApplicationDetailMessages(): ApplicationDetailMessages {
  return useSdkworkModuleMessages(DEPLOYMENTS_APP_DETAIL_I18N);
}
