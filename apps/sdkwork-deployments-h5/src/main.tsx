import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";

import { App } from "./App.tsx";
import { bootstrapDeploymentsH5Runtime } from "./bootstrap/runtime.ts";

async function main(): Promise<void> {
  const runtime = await bootstrapDeploymentsH5Runtime();
  const container = document.getElementById("root");
  if (!container) throw new Error("sdkwork-deployments-h5: #root container is missing");
  createRoot(container).render(
    <StrictMode>
      <BrowserRouter>
        <App runtime={runtime} />
      </BrowserRouter>
    </StrictMode>,
  );
}

void main();
