import type { SdkworkMiniMarketplacePort, MyTemplate } from '../../services/marketplace';

interface MarketplaceGlobals {
  marketplace: SdkworkMiniMarketplacePort;
}

Page({
  data: {
    items: [] as MyTemplate[],
    loading: true,
    error: '',
  },

  onShow() {
    this.reload();
  },

  reload() {
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    this.setData({ loading: true, error: '' });
    globals.marketplace
      .myTemplates()
      .then((items) => this.setData({ items, loading: false }))
      .catch(() => this.setData({ loading: false, error: '模板列表加载失败，请重试。' }));
  },

  onSubmitTap(event: WechatMiniprogram.TouchEvent) {
    const uuid = event.currentTarget.dataset.uuid as string;
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    globals.marketplace
      .submit(uuid)
      .then(() => {
        wx.showToast({ title: '已提交审核' });
        this.reload();
      })
      .catch(() => wx.showToast({ title: '提交未完成，请稍后重试', icon: 'none' }));
  },

  onVersionsTap(event: WechatMiniprogram.TouchEvent) {
    const uuid = event.currentTarget.dataset.uuid as string;
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    void globals.marketplace.versions(uuid).then((versions) => {
      const lines = versions.map((version) => version.templateVersion + ' · ' + version.status);
      void wx.showModal({
        title: '版本列表',
        content: lines.length === 0 ? '还没有版本。' : lines.join(String.fromCharCode(10)),
        showCancel: false,
      });
    });
  },

  /**
   * Withdrawing deletes the listing (`appTemplates.delete`), so it is behind a
   * confirmation: the mini-program has no undo and the row disappears on the
   * reload that follows.
   */
  onWithdrawTap(event: WechatMiniprogram.TouchEvent) {
    const uuid = event.currentTarget.dataset.uuid as string;
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    void wx.showModal({
      title: '下架模板',
      content: '下架后该模板将不再出现在模板市场，且无法撤销。',
      confirmText: '确认下架',
      cancelText: '取消',
      success: (result) => {
        if (!result.confirm) return;
        globals.marketplace
          .withdraw(uuid)
          .then(() => {
            wx.showToast({ title: '已下架' });
            this.reload();
          })
          .catch(() => wx.showToast({ title: '下架未完成，请稍后重试', icon: 'none' }));
      },
    });
  },
});
