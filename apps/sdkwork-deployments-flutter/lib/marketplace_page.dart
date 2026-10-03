import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

import 'marketplace_service.dart';
import 'order_center_client.dart';

/// H5/PC-parity storefront: category chips and keyword search over `PUBLIC` +
/// `PUBLISHED` listings and a detail bottom sheet. Acquiring a template is an
/// order-center trade: a FREE listing comes back settled and the entitlement is
/// live, a PAID listing comes back with the cashier URL / provider payment
/// payload the buyer still has to settle, which the sheet surfaces instead of
/// refusing the purchase. Ownership is read from the order center's
/// `status == 'paid'`, never from a local purchase record.
class MarketplacePage extends StatefulWidget {
  final MarketplacePort port;

  const MarketplacePage({super.key, required this.port});

  @override
  State<MarketplacePage> createState() => _MarketplacePageState();
}

class _MarketplacePageState extends State<MarketplacePage> {
  static const int _pageSize = 20;

  final TextEditingController _keyword = TextEditingController();
  List<TemplateCategoryResponse> _categories = [];
  List<AppTemplateSummaryResponse> _items = [];
  List<TemplateOrderSummary> _orders = [];
  Set<String> _entitled = <String>{};
  String? _categoryUuid;
  String? _templateType;
  String? _pricingModel;
  String _sort = 'NEWEST';
  int _page = 1;
  bool _hasMore = false;
  bool _loading = true;
  bool _loadingMore = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  @override
  void dispose() {
    _keyword.dispose();
    super.dispose();
  }

  static const _templateTypes = <String, String>{
    'APP': '应用',
    'PPT': 'PPT',
    'VIDEO': '视频',
  };

  static const _pricingModels = <String, String>{
    'FREE': '免费',
    'PAID': '付费',
  };

  static const _sorts = <String, String>{
    'NEWEST': '最新',
    'POPULAR': '热门',
  };

  Iterable<Widget> _typeChips() sync* {
    yield Padding(
      padding: const EdgeInsets.only(right: 8),
      child: ChoiceChip(
        label: const Text('全部类型'),
        selected: _templateType == null,
        onSelected: (_) {
          setState(() => _templateType = null);
          _reload();
        },
      ),
    );
    for (final entry in _templateTypes.entries) {
      yield Padding(
        padding: const EdgeInsets.only(right: 8),
        child: ChoiceChip(
          label: Text(entry.value),
          selected: _templateType == entry.key,
          onSelected: (_) {
            setState(() => _templateType = entry.key);
            _reload();
          },
        ),
      );
    }
  }

  Iterable<Widget> _pricingChips() sync* {
    yield Padding(
      padding: const EdgeInsets.only(right: 8),
      child: ChoiceChip(
        label: const Text('免费与付费'),
        selected: _pricingModel == null,
        onSelected: (_) {
          setState(() => _pricingModel = null);
          _reload();
        },
      ),
    );
    for (final entry in _pricingModels.entries) {
      yield Padding(
        padding: const EdgeInsets.only(right: 8),
        child: ChoiceChip(
          label: Text(entry.value),
          selected: _pricingModel == entry.key,
          onSelected: (_) {
            setState(() => _pricingModel = entry.key);
            _reload();
          },
        ),
      );
    }
  }

  Iterable<Widget> _sortChips() sync* {
    for (final entry in _sorts.entries) {
      yield Padding(
        padding: const EdgeInsets.only(right: 8),
        child: ChoiceChip(
          label: Text(entry.value),
          selected: _sort == entry.key,
          onSelected: (_) {
            setState(() => _sort = entry.key);
            _reload();
          },
        ),
      );
    }
  }

  /// Loads page 1 for the current facets, replacing the list.
  Future<void> _reload() async {
    setState(() {
      _loading = true;
      _loadingMore = false;
      _error = null;
      _page = 1;
    });
    try {
      final categories = await widget.port.categories();
      final listings = await widget.port.browse(
        page: 1,
        pageSize: _pageSize,
        keyword: _keyword.text,
        categoryUuid: _categoryUuid,
        templateType: _templateType,
        pricingModel: _pricingModel,
        sort: _sort,
      );
      final orders = await widget.port.myPurchases();
      if (!mounted) return;
      setState(() {
        _categories = categories;
        _items = listings.items;
        _orders = orders;
        _hasMore = listings.hasMore;
        // Only a settled order is the install entitlement; a started but
        // unpaid order is not.
        _entitled = orders
            .where((order) => order.isEntitlement)
            .map((order) => order.templateUuid)
            .toSet();
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = '模板市场加载失败，请重试。';
        _loading = false;
      });
    }
  }

  /// Appends the next page; the facet state and the loaded rows are kept.
  Future<void> _loadMore() async {
    if (!_hasMore || _loadingMore) return;
    setState(() => _loadingMore = true);
    final next = _page + 1;
    try {
      final listings = await widget.port.browse(
        page: next,
        pageSize: _pageSize,
        keyword: _keyword.text,
        categoryUuid: _categoryUuid,
        templateType: _templateType,
        pricingModel: _pricingModel,
        sort: _sort,
      );
      if (!mounted) return;
      setState(() {
        _items = <AppTemplateSummaryResponse>[..._items, ...listings.items];
        _hasMore = listings.hasMore;
        _page = next;
        _loadingMore = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() => _loadingMore = false);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('加载失败，请重试。')),
      );
    }
  }

  Future<void> _openDetail(AppTemplateSummaryResponse summary) async {
    try {
      final detail = await widget.port.retrieve(summary.id);
      if (!mounted) return;
      await showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        builder: (sheetContext) => _DetailSheet(
          port: widget.port,
          detail: detail,
          entitled: _entitled.contains(summary.id),
          onAcquired: () {
            Navigator.of(sheetContext).pop();
            return _reload();
          },
        ),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('模板详情加载失败，请重试。')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return RefreshIndicator(
      onRefresh: _reload,
      child: CustomScrollView(
        physics: const AlwaysScrollableScrollPhysics(),
        slivers: [
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
              child: TextField(
                controller: _keyword,
                decoration: const InputDecoration(
                  prefixIcon: Icon(Icons.search),
                  hintText: '搜索模板',
                  border: OutlineInputBorder(),
                  isDense: true,
                ),
                onSubmitted: (_) => _reload(),
              ),
            ),
          ),
          SliverToBoxAdapter(
            child: Column(
              children: [
                SizedBox(
                  height: 44,
                  child: ListView(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    children: [
                      // 模板类型是一级 facet：APP / PPT / VIDEO（对话式项目创作形态）。
                      ..._typeChips(),
                      Padding(
                        padding: const EdgeInsets.only(right: 8),
                        child: ChoiceChip(
                          label: const Text('全部'),
                          selected: _categoryUuid == null,
                          onSelected: (_) {
                            setState(() => _categoryUuid = null);
                            _reload();
                          },
                        ),
                      ),
                      ..._categories.map(
                        (category) => Padding(
                          padding: const EdgeInsets.only(right: 8),
                          child: ChoiceChip(
                            label: Text(category.displayName),
                            selected: _categoryUuid == category.id,
                            onSelected: (_) {
                              setState(() => _categoryUuid = category.id);
                              _reload();
                            },
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                SizedBox(
                  height: 44,
                  child: ListView(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    children: [..._pricingChips(), ..._sortChips()],
                  ),
                ),
              ],
            ),
          ),
          if (_error != null)
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(_error!, style: const TextStyle(color: Color(0xFF842A24))),
              ),
            )
          else if (_loading)
            const SliverFillRemaining(
              hasScrollBody: false,
              child: Center(child: CircularProgressIndicator()),
            )
          else if (_items.isEmpty)
            const SliverFillRemaining(
              hasScrollBody: false,
              child: Center(child: Text('暂无符合条件的模板。')),
            )
          else
            SliverList.builder(
              itemCount: _items.length,
              itemBuilder: (context, index) {
                final item = _items[index];
                return Card(
                  margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 5),
                  child: ListTile(
                    title: Text(item.displayName),
                    subtitle: Text(
                      item.summary,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    onTap: () => _openDetail(item),
                    trailing: _entitled.contains(item.id)
                        ? const Chip(
                            label: Text('已获取', style: TextStyle(fontSize: 11)),
                            visualDensity: VisualDensity.compact,
                          )
                        : null,
                  ),
                );
              },
            ),
          if (_hasMore)
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Center(
                  child: _loadingMore
                      ? const Padding(
                          padding: EdgeInsets.all(8),
                          child: SizedBox(
                            width: 20,
                            height: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                        )
                      : TextButton(onPressed: _loadMore, child: const Text('加载更多')),
                ),
              ),
            ),
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
              child: Text('我的模板订单', style: Theme.of(context).textTheme.titleMedium),
            ),
          ),
          if (_orders.isEmpty)
            const SliverToBoxAdapter(
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Text('还没有模板订单。'),
              ),
            )
          else
            SliverList.builder(
              itemCount: _orders.length,
              itemBuilder: (context, index) {
                final order = _orders[index];
                return ListTile(
                  dense: true,
                  title: Text(order.templateName.isEmpty ? order.templateUuid : order.templateName),
                  subtitle: Text('${order.orderNo} · ${order.amount} ${order.currencyCode} · ${order.fulfillmentStatus}'),
                  trailing: Chip(
                    label: Text(order.status, style: const TextStyle(fontSize: 11)),
                    visualDensity: VisualDensity.compact,
                  ),
                );
              },
            ),
        ],
      ),
    );
  }
}

class _DetailSheet extends StatelessWidget {
  final MarketplacePort port;
  final AppTemplateResponse detail;
  final bool entitled;
  final Future<void> Function() onAcquired;

  const _DetailSheet({
    required this.port,
    required this.detail,
    required this.entitled,
    required this.onAcquired,
  });

  Future<void> _acquire(BuildContext context) async {
    try {
      final order = await port.acquire(detail.id);
      if (!context.mounted) return;
      if (!order.isPaid) {
        await _showPaymentRequirement(context, order);
      }
      await onAcquired();
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('下单失败，请重试。')),
        );
      }
    }
  }

  /// A PAID listing answers `pending_payment`: the order exists and the buyer
  /// still has to settle it, so the requirement the order center returned is
  /// shown here (and can be copied) instead of the purchase being refused.
  Future<void> _showPaymentRequirement(BuildContext context, TemplateOrder order) async {
    final requirement = order.paymentRequirement;
    final isNativePayload = order.qrCodeType == TemplateOrder.qrCodeTypeProviderNative;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        scrollable: true,
        title: const Text('订单已创建，请完成支付'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('订单号：${order.orderNo}'),
            const SizedBox(height: 4),
            Text('应付金额：${order.amount} ${order.currencyCode}'),
            const SizedBox(height: 8),
            if (requirement == null)
              const Text('请到订单中心完成支付。')
            else
              SelectableText(
                (isNativePayload ? '支付凭证：' : '支付链接：') + requirement,
              ),
          ],
        ),
        actions: [
          if (requirement != null)
            TextButton(
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: requirement));
                if (dialogContext.mounted) Navigator.of(dialogContext).pop();
              },
              child: Text(isNativePayload ? '复制凭证' : '复制链接'),
            ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text(requirement == null ? '知道了' : '稍后支付'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final pricing = detail.pricingModel == 'PAID'
        ? '付费 ${detail.priceMinor} ${detail.currency}'
        : '免费';
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(detail.displayName, style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 4),
            Text(detail.summary, style: Theme.of(context).textTheme.bodyMedium),
            const SizedBox(height: 8),
            Text(pricing, style: Theme.of(context).textTheme.titleMedium),
            if (detail.description.trim().isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(detail.description),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('关闭'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  // 免费模板下单即结算；付费模板下单后回执收银台链接/支付凭证，
                  // 由买家在订单中心完成支付，权益以订单 `paid` 为准。
                  onPressed: entitled ? null : () => _acquire(context),
                  child: Text(entitled ? '已获取' : (detail.pricingModel == 'PAID' ? '购买模板' : '获取模板')),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
