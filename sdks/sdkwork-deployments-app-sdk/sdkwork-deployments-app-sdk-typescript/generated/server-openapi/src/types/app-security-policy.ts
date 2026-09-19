export interface AppSecurityPolicy {
  forceHttps?: boolean;
  denyDotFiles?: boolean;
  deniedPathPrefixes?: string[];
}
