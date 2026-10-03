import type { SdkworkMiniMarketplacePort, TemplateCategory, TemplateSummary } from '../../services/marketplace';

interface MarketplaceGlobals {
  marketplace: SdkworkMiniMarketplacePort;
}

const PAGE_SIZE = 20;

Page({
  data: {
    categories: [] as TemplateCategory[],
    items: [] as TemplateSummary[],
    // A plain lookup object, not a Set: page data crosses into WXML as JSON, so
    // a Set would arrive as `{}` and every `entitledMap[item.id]` test would be
    // falsy — the acquired badge would never render.
    entitledMap: {} as Record<string, boolean>,
    activeCategory: '',
    activeType: '',
    activePricing: '',
    activeSort: 'NEWEST',
    keyword: '',
    page: 1,
    hasMore: false,
    total: 0,
    loading: true,
    loadingMore: false,
    error: '',
  },

  // 模板类型是一级 facet：APP / PPT / VIDEO（对话式项目创作形态）。
  templateTypes: [
    { key: '', label: '全部类型' },
    { key: 'APP', label: '应用' },
    { key: 'PPT', label: 'PPT' },
    { key: 'VIDEO', label: '视频' },
  ],

  pricingModels: [
    { key: '', label: '免费与付费' },
    { key: 'FREE', label: '免费' },
    { key: 'PAID', label: '付费' },
  ],

  sorts: [
    { key: 'NEWEST', label: '最新' },
    { key: 'POPULAR', label: '热门' },
  ],

  onLoad() {
    this.reload();
  },

  /** First page for the current facets: replaces the list instead of appending. */
  reload() {
    this.setData({ loading: true, loadingMore: false, error: '', page: 1 });
    this.fetch(1).catch(() => this.setData({ loading: false, error: '模板市场加载失败，请重试。' }));
  },

  /** Next page: appends to the loaded rows and keeps the facet state. */
  loadMore() {
    if (!this.data.hasMore || this.data.loadingMore) return;
    const next = this.data.page + 1;
    this.setData({ loadingMore: true });
    this.fetch(next).catch(() => {
      this.setData({ loadingMore: false });
      wx.showToast({ title: '加载失败，请重试', icon: 'none' });
    });
  },

  fetch(page: number): Promise<void> {
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    return Promise.all([
      globals.marketplace.categories(),
      globals.marketplace.browse({
        page,
        pageSize: PAGE_SIZE,
        keyword: this.data.keyword,
        ...(this.data.activeCategory === '' ? {} : { categoryUuid: this.data.activeCategory }),
        ...(this.data.activeType === '' ? {} : { templateType: this.data.activeType }),
        ...(this.data.activePricing === '' ? {} : { pricingModel: this.data.activePricing }),
        sort: this.data.activeSort,
      }),
      globals.marketplace.myPurchases(),
    ]).then(([categories, listings, purchases]) => {
      const entitledMap: Record<string, boolean> = {};
      for (const purchase of purchases) {
        if (purchase.status === 'ACTIVE') entitledMap[purchase.templateUuid] = true;
      }
      this.setData({
        categories,
        items: page === 1 ? listings.items : this.data.items.concat(listings.items),
        entitledMap,
        page,
        hasMore: listings.hasMore,
        total: listings.total,
        loading: false,
        loadingMore: false,
      });
    });
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

  onPricingTap(event: WechatMiniprogram.TouchEvent) {
    const key = event.currentTarget.dataset.key as string | undefined;
    this.setData({ activePricing: key ?? '' });
    this.reload();
  },

  onSortTap(event: WechatMiniprogram.TouchEvent) {
    const key = event.currentTarget.dataset.key as string | undefined;
    this.setData({ activeSort: key === 'POPULAR' ? 'POPULAR' : 'NEWEST' });
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
