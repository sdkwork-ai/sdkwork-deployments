// SDKWork deployments mini-program entry. The marketplace port is built once
// here; pages read it through getApp<T>().globalData.marketplace.
import { createWxRequester, SdkworkMiniMarketplacePort } from './services/marketplace';

App({
  globalData: {
    marketplace: new SdkworkMiniMarketplacePort(createWxRequester(), {
      // Standalone development endpoint; per-profile values are substituted by
      // the release materialization for other lanes.
      appApiBaseUrl: 'http://127.0.0.1:3900',
    }),
  },
});
