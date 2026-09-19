export interface AppDeliveryPolicy {
  providerTimeoutMs?: number;
  metadataCacheTtlSeconds?: number;
  negativeCacheTtlSeconds?: number;
  staleWhileRevalidateSeconds?: number;
  maximumObjectBytes?: number;
}
