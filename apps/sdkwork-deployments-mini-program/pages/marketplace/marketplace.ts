import type { SdkworkMiniMarketplacePort, TemplateCategory, TemplateOrder, TemplateSummary } from '../../services/marketplace';
import { paymentRequirement } from '../../services/marketplace';

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
    ]).then(([categories, listings, orders]) => {
      // The entitlement is the order center's settled order for the listing;
      // a `pending_payment` row is a started purchase, not an entitlement.
      const entitledMap: Record<string, boolean> = {};
      for (const order of orders) {
        if (order.status === 'paid') entitledMap[order.templateUuid] = true;
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
    const entitled = this.data.entitledMap[uuid] === true;
    wx.showLoading({ title: '加载中' });
    globals.marketplace
      .retrieve(uuid)
      .then((detail) => {
        wx.hideLoading();
        const lines = [detail.summary, detail.description].filter((part) => part.trim() !== '');
        if (detail.pricingModel === 'PAID') {
          lines.push('价格：' + detail.priceMinor + ' ' + detail.currency);
        }
        void wx.showModal({
          title: detail.displayName,
          content: lines.join(String.fromCharCode(10) + String.fromCharCode(10)),
          confirmText: entitled ? '知道了' : detail.pricingModel === 'PAID' ? '购买模板' : '获取模板',
          cancelText: '关闭',
          showCancel: !entitled,
          success: (result) => {
            if (!result.confirm || entitled) return;
            this.acquireTemplate(uuid);
          },
        });
      })
      .catch(() => {
        wx.hideLoading();
        wx.showToast({ title: '详情加载失败，请重试', icon: 'none' });
      });
  },

  /**
   * Starts the order-center trade through the port. A FREE listing settles in
   * the order-center transaction (`status: "paid"`); a PAID listing answers
   * `pending_payment` and the buyer still has to pay, so the requirement is
   * surfaced instead of the purchase being refused here.
   */
  acquireTemplate(uuid: string) {
    const globals = getApp<{ globalData: MarketplaceGlobals }>().globalData;
    wx.showLoading({ title: '下单中' });
    globals.marketplace
      .acquire(uuid)
      .then((order) => {
        wx.hideLoading();
        if (order.status === 'paid') {
          wx.showToast({ title: '已获取', icon: 'success' });
          this.reload();
          return;
        }
        if (order.status === 'pending_payment') {
          this.showPaymentRequirement(order);
          return;
        }
        wx.showToast({ title: '订单已关闭，请重试', icon: 'none' });
      })
      .catch(() => {
        wx.hideLoading();
        wx.showToast({ title: '下单失败，请重试', icon: 'none' });
      });
  },

  /** The order center's payment requirement, on the page's dialog surface. */
  showPaymentRequirement(order: TemplateOrder) {
    const requirement = paymentRequirement(order);
    const isNativePayload = order.qrCodeType === 'provider_native' && requirement !== '';
    const lines = [
      '订单号：' + order.orderNo,
      '应付金额：' + order.amount + ' ' + order.currencyCode,
      requirement === ''
        ? '请到订单中心完成支付。'
        : (isNativePayload ? '支付凭证：' : '支付链接：') + requirement,
    ];
    void wx.showModal({
      title: '订单已创建，请完成支付',
      content: lines.join(String.fromCharCode(10) + String.fromCharCode(10)),
      confirmText: requirement === '' ? '知道了' : (isNativePayload ? '复制凭证' : '复制链接'),
      cancelText: '稍后支付',
      showCancel: requirement !== '',
      success: (result) => {
        if (result.confirm && requirement !== '') {
          wx.setClipboardData({ data: requirement });
        }
        this.reload();
      },
    });
  },
});
