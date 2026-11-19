export interface InstallAppTemplateRequest {
  name?: string;
  slug?: string;
  /** Settled order that owns a paid listing. Required for PAID templates; the server verifies buyer, payment state and template before install. */
  orderId?: string;
}
