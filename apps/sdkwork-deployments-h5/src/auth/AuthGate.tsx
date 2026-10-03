import { useSdkworkAuthControllerState, type SdkworkAuthController } from "@sdkwork/auth-pc-react";
import { useEffect, useState, type ReactNode } from "react";
import { Navigate, useLocation } from "react-router-dom";

export interface DeploymentsH5AuthGateProps {
  readonly authRoutes: ReactNode;
  readonly children: ReactNode;
  readonly controller: SdkworkAuthController;
}

/** Session bootstrap gate: identical flow to the PC app, mobile markup. */
export function DeploymentsH5AuthGate({ authRoutes, children, controller }: DeploymentsH5AuthGateProps) {
  const location = useLocation();
  const state = useSdkworkAuthControllerState(controller);
  const authRoute = location.pathname === "/auth" || location.pathname.startsWith("/auth/");
  const [complete, setComplete] = useState(state.isBootstrapped);
  useEffect(() => {
    if (authRoute || state.isBootstrapped) {
      setComplete(true);
      return;
    }
    let active = true;
    const timeout = setTimeout(() => {
      if (active) setComplete(true);
    }, 6_000);
    void controller.bootstrap().finally(() => {
      clearTimeout(timeout);
      if (active) setComplete(true);
    });
    return () => {
      active = false;
      clearTimeout(timeout);
    };
  }, [authRoute, controller, state.isBootstrapped]);
  if (authRoute) return <>{authRoutes}</>;
  if (!complete) return <div className="h5-bootstrap-state">SDKWork Deployments</div>;
  if (!state.isAuthenticated) {
    return <Navigate to={`/auth/login?redirect=${encodeURIComponent(location.pathname + location.search)}`} replace />;
  }
  return <>{children}</>;
}
