import { SdkworkAuthPage, type SdkworkAuthController, type SdkworkAuthHeaderSlotProps } from "@sdkwork/auth-pc-react";
import { Boxes } from "lucide-react";

export function DeploymentsH5AuthRoutes({ controller }: { controller: SdkworkAuthController }) {
  return <SdkworkAuthPage appearance={{ slots: { Header } }} basePath="/auth" controller={controller} homePath="/" />;
}

function Header({ description, title }: SdkworkAuthHeaderSlotProps) {
  return (
    <header className="h5-auth-header">
      <div className="h5-auth-brand">
        <Boxes size={18} />
        <strong>SDKWork Deployments</strong>
      </div>
      <h1>{title}</h1>
      <p>{description}</p>
    </header>
  );
}
