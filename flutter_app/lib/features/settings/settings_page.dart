import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/notification_service.dart';
import '../../core/providers.dart';

class SettingsPage extends ConsumerStatefulWidget {
  const SettingsPage({super.key});

  @override
  ConsumerState<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends ConsumerState<SettingsPage>
    with WidgetsBindingObserver {
  bool _notificationPermissionGranted = false;
  bool _isCheckingPermission = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _checkNotificationPermission();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _checkNotificationPermission();
      clearPendingRoute();
    }
  }

  Future<void> _checkNotificationPermission() async {
    final granted = await NotificationService.hasPermission();
    if (!mounted) return;
    setState(() {
      _notificationPermissionGranted = granted;
      _isCheckingPermission = false;
    });
  }

  Future<void> _onNotificationSwitchToggled(bool value) async {
    if (value) {
      final granted = await NotificationService.requestPermission();
      if (!mounted) return;
      setState(() => _notificationPermissionGranted = granted);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(granted ? '通知权限已开启' : '通知权限请求被拒绝'),
            duration: const Duration(seconds: 2),
          ),
        );
      }
    } else {
      _showDisableNotificationDialog();
    }
  }

  void _showDisableNotificationDialog() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('关闭通知'),
        content: const Text('请在系统设置中关闭通知权限'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () {
              Navigator.pop(context);
              savePendingRoute('/settings');
              NotificationService.openNotificationSettings();
            },
            child: const Text('去设置'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final themeMode = ref.watch(themeModeProvider);
    final colorScheme = Theme.of(context).colorScheme;

    return Scaffold(
      appBar: AppBar(title: const Text('设置')),
      body: ListView(
        children: [
          _SectionHeader(title: '通知'),
          ListTile(
            leading: Icon(
              _notificationPermissionGranted
                  ? Icons.notifications_active
                  : Icons.notifications_off_outlined,
              color: _notificationPermissionGranted
                  ? colorScheme.primary
                  : colorScheme.onSurfaceVariant,
            ),
            title: const Text('通知权限'),
            subtitle: Text(
              _isCheckingPermission
                  ? '检查中...'
                  : _notificationPermissionGranted
                      ? '已授权'
                      : '未授权（点击开启）',
            ),
            trailing: Switch(
              value: _notificationPermissionGranted,
              onChanged:
                  _isCheckingPermission ? null : _onNotificationSwitchToggled,
            ),
          ),
          const Divider(indent: 16, endIndent: 16),
          _SectionHeader(title: '显示'),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(
              children: [
                const Icon(Icons.format_size),
                const SizedBox(width: 12),
                const Text('字体大小'),
                const SizedBox(width: 8),
                Text(
                  '${ref.watch(fontSizeProvider).round()}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
                Expanded(
                  child: Slider(
                    value: ref.watch(fontSizeProvider),
                    min: 14,
                    max: 28,
                    divisions: 14,
                    label: '${ref.watch(fontSizeProvider).round()}',
                    onChanged: (value) {
                      ref.read(fontSizeProvider.notifier).state = value;
                      saveFontSizeToDisk(value);
                    },
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 8),
          RadioGroup<ThemeMode>(
            groupValue: themeMode,
            onChanged: (value) {
              if (value != null) {
                ref.read(themeModeProvider.notifier).state = value;
                saveThemeModeToDisk(value);
              }
            },
            child: Column(
              children: ThemeMode.values
                  .map(
                    (mode) => RadioListTile<ThemeMode>(
                      title: Text(_themeModeLabel(mode)),
                      value: mode,
                      secondary: Icon(_themeModeIcon(mode)),
                    ),
                  )
                  .toList(),
            ),
          ),
          const Divider(indent: 16, endIndent: 16),
          _SectionHeader(title: '工具'),
          ListTile(
            leading: const Icon(Icons.rule),
            title: const Text('替换规则'),
            subtitle: const Text('管理正则替换规则'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => context.push('/replace-rules'),
          ),
          const Divider(indent: 16, endIndent: 16),
          _SectionHeader(title: '关于'),
          const ListTile(
            leading: Icon(Icons.info_outline),
            title: Text('Legado Reader'),
            subtitle: Text('版本 0.1.0'),
          ),
          const ListTile(
            leading: Icon(Icons.code),
            title: Text('技术栈'),
            subtitle: Text('Flutter + Rust (flutter_rust_bridge)'),
          ),
        ],
      ),
    );
  }

  String _themeModeLabel(ThemeMode mode) {
    switch (mode) {
      case ThemeMode.system:
        return '跟随系统';
      case ThemeMode.light:
        return '浅色模式';
      case ThemeMode.dark:
        return '深色模式';
    }
  }

  IconData _themeModeIcon(ThemeMode mode) {
    switch (mode) {
      case ThemeMode.system:
        return Icons.brightness_auto;
      case ThemeMode.light:
        return Icons.light_mode;
      case ThemeMode.dark:
        return Icons.dark_mode;
    }
  }
}

class _SectionHeader extends StatelessWidget {
  final String title;

  const _SectionHeader({required this.title});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
      child: Text(
        title,
        style: Theme.of(context).textTheme.titleSmall?.copyWith(
              color: Theme.of(context).colorScheme.primary,
              fontWeight: FontWeight.bold,
            ),
      ),
    );
  }
}
