import type { SdkworkMiniMarketplacePort, TemplateCategory, TemplateSummary } from '../../services/marketplace';

interface MarketplaceGlobals {
  marketplace: SdkworkMiniMarketplacePort;
}

Page({
  data: {
    categories: [] as TemplateCategory[],
    items: [] as TemplateSummary[],
    entitled: new Set<string>(),
    activeCategory: '',
    activeType: '',
    keyword: '',
    loading: true,
    error: '',
  },

  // 模板类型是一级 facet：APP / PPT / VIDEO（对话式项目创作形态）。
  templateTypes: [
    { key: '', label: '全部类型' },
    { key: 'APP', label: '应用' },
    { key: 'PPT', label: 'PPT' },
    { key: 'VIDEO', label: '视频' },
  ],

  onLoad() {
    this.reload();
  },

  reload() {
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    this.setData({ loading: true, error: '' });
    Promise.all([
      globals.marketplace.categories(),
      globals.marketplace.browse({
        page: 1,
        pageSize: 20,
        keyword: this.data.keyword,
        ...(this.data.activeCategory === '' ? {} : { categoryUuid: this.data.activeCategory }),
        ...(this.data.activeType === '' ? {} : { templateType: this.data.activeType }),
      }),
      globals.marketplace.myPurchases(),
    ])
      .then(([categories, items, purchases]) => {
        this.setData({
          categories,
          items,
          entitled: new Set(purchases.filter((purchase) => purchase.status === 'ACTIVE').map((purchase) => purchase.templateUuid)),
          loading: false,
        });
      })
      .catch(() => this.setData({ loading: false, error: '模板市场加载失败，请重试。' }));
  },

  onKeywordInput(input: WechatMiniprogram.Input) {
    this.setData({ keyword: input.detail.value });
  },

  onSearch() {
    this.reload();
  },

  onTypeTap(event: WechatMiniprogram.TouchEvent) {
    const key = event.currentTarget.dataset.key as string | undefined;
    this.setData({ activeType: key ?? '' });
    this.reload();
  },

  onCategoryTap(event: WechatMiniprogram.TouchEvent) {
    const key = event.currentTarget.dataset.key as string | undefined;
    this.setData({ activeCategory: key ?? '' });
    this.reload();
  },

  onCardTap(event: WechatMiniprogram.TouchEvent) {
    const uuid = event.currentTarget.dataset.uuid as string;
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    wx.showLoading({ title: '加载中' });
    globals.marketplace
      .retrieve(uuid)
      .then((detail) => {
        wx.hideLoading();
        void wx.showModal({
          title: detail.displayName,
          content: [detail.summary, detail.description].filter((part) => part.trim() !== '').join(String.fromCharCode(10) + String.fromCharCode(10)),
          confirmText: detail.pricingModel === 'PAID' ? '知道了' : '获取模板',
          cancelText: '关闭',
          showCancel: detail.pricingModel !== 'PAID',
          success: (result) => {
            if (!result.confirm || detail.pricingModel === 'PAID') return;
            globals.marketplace
              .acquire(uuid)
              .then(() => this.reload())
              .catch(() => wx.showToast({ title: '获取失败，请重试', icon: 'none' }));
          },
        });
      })
      .catch(() => {
        wx.hideLoading();
        wx.showToast({ title: '详情加载失败，请重试', icon: 'none' });
      });
  },
});
