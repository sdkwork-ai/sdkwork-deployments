import 'package:flutter/material.dart';

import 'bootstrap/sdkwork_tls_trust.dart';
import 'marketplace_page.dart';
import 'marketplace_service.dart';
import 'my_templates_page.dart';
import 'runtime_config.dart';

void main() {
  // The HTTPS trust layer must be installed before any HTTP client is
  // created, so it runs ahead of binding initialization and services.
  SdkworkTlsTrust.installForApp();

  const config = DeploymentsRuntimeConfig.dev();
  final port = SdkworkMarketplacePort.fromConfig(config);
  runApp(DeploymentsFlutterApp(port: port));
}

class DeploymentsFlutterApp extends StatelessWidget {
  final MarketplacePort port;

  const DeploymentsFlutterApp({super.key, required this.port});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'SDKWork Deployments',
      theme: ThemeData(colorSchemeSeed: const Color(0xFFE0AD31), useMaterial3: true),
      home: MarketplaceHome(port: port),
    );
  }
}

class MarketplaceHome extends StatefulWidget {
  final MarketplacePort port;

  const MarketplaceHome({super.key, required this.port});

  @override
  State<MarketplaceHome> createState() => _MarketplaceHomeState();
}

class _MarketplaceHomeState extends State<MarketplaceHome> {
  int _tab = 0;

  static const _tabs = <int, String>{
    0: '模板市场',
    1: '我的模板',
  };

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_tabs[_tab] ?? 'SDKWork Deployments'),
      ),
      body: _tab == 0
          ? MarketplacePage(port: widget.port)
          : MyTemplatesPage(port: widget.port),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (index) => setState(() => _tab = index),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.storefront_outlined), selectedIcon: Icon(Icons.storefront), label: '模板市场'),
          NavigationDestination(icon: Icon(Icons.widgets_outlined), selectedIcon: Icon(Icons.widgets), label: '我的模板'),
        ],
      ),
    );
  }
}
