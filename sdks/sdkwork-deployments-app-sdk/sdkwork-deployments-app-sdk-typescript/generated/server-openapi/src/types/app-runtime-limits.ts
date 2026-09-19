export interface AppRuntimeLimits {
  maximumBindings?: number;
  maximumVariants?: number;
  maximumVariantRules?: number;
  maximumResources?: number;
  maximumMounts?: number;
  maximumIndexFilesPerMount?: number;
  maximumPathBytes?: number;
  maximumPathSegments?: number;
}
