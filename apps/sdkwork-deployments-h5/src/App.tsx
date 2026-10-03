import { useSdkworkAuthControllerState } from "@sdkwork/auth-pc-react";
import { useState, type ReactElement } from "react";
import { Route, Routes, useNavigate } from "react-router-dom";
import { Boxes, Store, Tags } from "lucide-react";

import type { BootstrappedDeploymentsH5Runtime } from "./bootstrap/runtime.ts";
import { DeploymentsH5AuthGate } from "./auth/AuthGate.tsx";
import { DeploymentsH5AuthRoutes } from "./auth/AuthRoutes.tsx";
import { MarketplaceView } from "./marketplace/MarketplaceView.tsx";
import { MyTemplatesView } from "./marketplace/MyTemplatesView.tsx";
import { translateH5, type DeploymentsH5MessageKey } from "./marketplace/i18n.ts";

type H5Tab = "marketplace" | "myTemplates";

const TABS: readonly { id: H5Tab; icon: typeof Store; titleKey: DeploymentsH5MessageKey }[] = [
  { id: "marketplace", icon: Store, titleKey: "tab.marketplace" },
  { id: "myTemplates", icon: Tags, titleKey: "tab.myTemplates" },
] as const;

export function App({ runtime }: { runtime: BootstrappedDeploymentsH5Runtime }): ReactElement {
  return (
    <Routes>
      <Route
        path="/auth/*"
        element={<DeploymentsH5AuthRoutes controller={runtime.authController} />}
      />
      <Route
        path="/*"
        element={
          <DeploymentsH5AuthGate authRoutes={<DeploymentsH5AuthRoutes controller={runtime.authController} />} controller={runtime.authController}>
            <Workspace runtime={runtime} />
          </DeploymentsH5AuthGate>
        }
      />
    </Routes>
  );
}

function Workspace({ runtime }: { runtime: BootstrappedDeploymentsH5Runtime }): ReactElement {
  const navigate = useNavigate();
  const state = useSdkworkAuthControllerState(runtime.authController);
  const [tab, setTab] = useState<H5Tab>("marketplace");
  const t = (key: DeploymentsH5MessageKey) => translateH5(runtime.locale, key);
  const userLabel = state.user?.displayName || state.user?.email || "";
  return (
    <div className="h5-shell">
      <header className="h5-header">
        <span className="h5-brand">
          <Boxes size={18} />
          {t("app.title")}
        </span>
        {state.isAuthenticated && (
          <button
            className="h5-signout"
            type="button"
            onClick={() => {
              void runtime.authController.signOut().then(() => navigate("/auth/login", { replace: true }));
            }}
          >
            {userLabel}
            <em>{t("auth.signOut")}</em>
          </button>
        )}
      </header>
      <main className="h5-main">
        {tab === "marketplace" ? <MarketplaceView runtime={runtime} /> : <MyTemplatesView runtime={runtime} />}
      </main>
      <nav className="h5-tabbar" aria-label={t("tab.marketplace")}>
        {TABS.map((entry) => {
          const Icon = entry.icon;
          const active = tab === entry.id;
          return (
            <button
              className={active ? "h5-tab h5-tab-active" : "h5-tab"}
              key={entry.id}
              type="button"
              onClick={() => setTab(entry.id)}
            >
              <Icon size={19} />
              <span>{t(entry.titleKey)}</span>
            </button>
          );
        })}
      </nav>
    </div>
  );
}
