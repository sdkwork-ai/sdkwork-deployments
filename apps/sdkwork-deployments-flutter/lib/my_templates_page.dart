import 'package:flutter/material.dart';

import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

import 'marketplace_service.dart';

/// Author workbench, mobile shape: the caller's listings with review state,
/// version history, and the submit/withdraw commands. Publishing a NEW
/// listing stays on the PC console for v1 — it is a Drive-packaged artifact
/// flow, and the mobile cut only manages listings that already exist.
class MyTemplatesPage extends StatefulWidget {
  final MarketplacePort port;

  const MyTemplatesPage({super.key, required this.port});

  @override
  State<MyTemplatesPage> createState() => _MyTemplatesPageState();
}

class _MyTemplatesPageState extends State<MyTemplatesPage> {
  List<AppTemplateResponse> _items = [];
  bool _loading = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final listings = await widget.port.myTemplates();
      if (!mounted) return;
      setState(() {
        _items = listings;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = '模板列表加载失败，请重试。';
        _loading = false;
      });
    }
  }

  Future<void> _openVersions(AppTemplateResponse item) async {
    try {
      final versions = await widget.port.versions(item.id);
      if (!mounted) return;
      await showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        builder: (sheetContext) => _VersionsSheet(versions: versions),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('版本列表加载失败，请重试。')),
      );
    }
  }

  Future<void> _submit(AppTemplateResponse item) async {
    try {
      await widget.port.submit(item.id);
      await _reload();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('已提交审核。')),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('提交未完成，请稍后重试。')),
      );
    }
  }

  /// Withdrawing deletes the listing, so it is behind a confirmation: the row
  /// disappears on the reload that follows and there is no undo.
  Future<void> _withdraw(AppTemplateResponse item) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('下架模板'),
        content: const Text('下架后该模板将不再出现在模板市场，且无法撤销。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('确认下架'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    try {
      await widget.port.withdraw(item.id);
      await _reload();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('已下架。')),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('下架未完成，请稍后重试。')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(_error!, style: const TextStyle(color: Color(0xFF842A24))),
            const SizedBox(height: 8),
            FilledButton.tonal(onPressed: _reload, child: const Text('重试')),
          ],
        ),
      );
    }
    if (_items.isEmpty) {
      return const Center(child: Text('还没有发布过模板。'));
    }
    return RefreshIndicator(
      onRefresh: _reload,
      child: ListView.builder(
        itemCount: _items.length,
        itemBuilder: (context, index) {
          final item = _items[index];
          return Card(
            margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 5),
            child: ListTile(
              title: Text(item.displayName),
              subtitle: Text('${item.templateKey} · ${item.status} · ${item.visibility}'),
              onTap: () => _openVersions(item),
              trailing: PopupMenuButton<String>(
                onSelected: (action) {
                  if (action == 'submit') _submit(item);
                  if (action == 'withdraw') _withdraw(item);
                },
                itemBuilder: (context) => const [
                  PopupMenuItem(value: 'submit', child: Text('提交审核')),
                  PopupMenuItem(value: 'withdraw', child: Text('下架模板')),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

class _VersionsSheet extends StatelessWidget {
  final List<AppTemplateVersionResponse> versions;

  const _VersionsSheet({required this.versions});

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('版本列表', style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 8),
            if (versions.isEmpty)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 12),
                child: Text('还没有版本。'),
              )
            else
              ...versions.map(
                (version) => ListTile(
                  contentPadding: EdgeInsets.zero,
                  dense: true,
                  leading: Chip(
                    label: Text(
                      version.status,
                      style: const TextStyle(fontSize: 11),
                    ),
                    visualDensity: VisualDensity.compact,
                  ),
                  title: Text(version.templateVersion),
                  subtitle: version.changelog.isEmpty ? null : Text(version.changelog),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
