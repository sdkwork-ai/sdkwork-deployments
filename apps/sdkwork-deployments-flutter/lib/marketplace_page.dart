import 'package:flutter/material.dart';

import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

import 'marketplace_service.dart';

/// H5/PC-parity storefront: category chips and keyword search over `PUBLIC` +
/// `PUBLISHED` listings, a detail bottom sheet, and the idempotent acquire
/// command. FREE listings grant their entitlement through this surface; PAID
/// listings disable acquire and route the buyer to the commerce checkout, so
/// the button never implies the entitlement was minted here.
class MarketplacePage extends StatefulWidget {
  final MarketplacePort port;

  const MarketplacePage({super.key, required this.port});

  @override
  State<MarketplacePage> createState() => _MarketplacePageState();
}

class _MarketplacePageState extends State<MarketplacePage> {
  final TextEditingController _keyword = TextEditingController();
  List<TemplateCategoryResponse> _categories = [];
  List<AppTemplateSummaryResponse> _items = [];
  Set<String> _entitled = <String>{};
  String? _categoryUuid;
  String? _templateType;
  bool _loading = true;
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

  Future<void> _reload() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final categories = await widget.port.categories();
      final listings = await widget.port.browse(
        keyword: _keyword.text,
        categoryUuid: _categoryUuid,
        templateType: _templateType,
      );
      final purchases = await widget.port.myPurchases();
      if (!mounted) return;
      setState(() {
        _categories = categories;
        _items = listings;
        _entitled = purchases
            .where((purchase) => purchase.status == 'ACTIVE')
            .map((purchase) => purchase.templateUuid)
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
            child: SizedBox(
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
      await port.acquire(detail.id);
      await onAcquired();
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('获取模板失败，请重试。')),
        );
      }
    }
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
                  // PAID 模板的购买走 commerce checkout（订单履约回写权益），
                  // 这里的直接获取只授予 FREE 模板。
                  onPressed: entitled || detail.pricingModel == 'PAID' ? null : () => _acquire(context),
                  child: Text(entitled ? '已获取' : (detail.pricingModel == 'PAID' ? '通过订单流程购买' : '获取模板')),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
