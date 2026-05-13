import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;

class ReplaceRulePage extends ConsumerStatefulWidget {
  const ReplaceRulePage({super.key});

  @override
  ConsumerState<ReplaceRulePage> createState() => _ReplaceRulePageState();
}

class _ReplaceRulePageState extends ConsumerState<ReplaceRulePage> {
  @override
  Widget build(BuildContext context) {
    final rulesAsync = ref.watch(allReplaceRulesProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('替换规则'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () => ref.invalidate(allReplaceRulesProvider),
          ),
        ],
      ),
      body: rulesAsync.when(
        data: (rules) => _buildRuleList(context, rules),
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败: $e')),
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _showAddRuleDialog(context),
        child: const Icon(Icons.add),
      ),
    );
  }

  Widget _buildRuleList(
      BuildContext context, List<Map<String, dynamic>> rules) {
    if (rules.isEmpty) {
      return const Center(child: Text('暂无替换规则，点击右下角添加'));
    }
    return ListView.builder(
      padding: const EdgeInsets.all(8),
      itemCount: rules.length,
      itemBuilder: (context, index) {
        final rule = rules[index];
        final enabled = rule['enabled'] == true;
        final scopeLabels = ['全局', '书源', '书籍'];
        final scope = (rule['scope'] as int?) ?? 0;
        final scopeLabel =
            scope >= 0 && scope < scopeLabels.length
                ? scopeLabels[scope]
                : '未知';
        return Card(
          child: ListTile(
            leading: Icon(
              enabled ? Icons.check_circle : Icons.cancel,
              color: enabled ? Colors.green : Colors.grey,
            ),
            title: Text(rule['name'] ?? '未命名规则'),
            subtitle: Text('${rule['pattern'] ?? ''} → ${rule['replacement'] ?? ''}  [$scopeLabel]'),
            trailing: Switch(
              value: enabled,
              onChanged: (val) =>
                  _toggleRule(rule['id'] as String, val),
            ),
            onTap: () => _showRuleActions(context, rule),
          ),
        );
      },
    );
  }

  Future<void> _toggleRule(String id, bool enabled) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.setReplaceRuleEnabled(
          dbPath: dbPath, id: id, enabled: enabled);
      ref.invalidate(allReplaceRulesProvider);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('操作失败: $e')),
        );
      }
    }
  }

  void _showAddRuleDialog(BuildContext context) {
    final nameCtrl = TextEditingController();
    final patternCtrl = TextEditingController();
    final replacementCtrl = TextEditingController();
    int scope = 0;

    showDialog(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) => AlertDialog(
          title: const Text('添加替换规则'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: nameCtrl,
                  decoration: const InputDecoration(labelText: '规则名称'),
                ),
                TextField(
                  controller: patternCtrl,
                  decoration: const InputDecoration(
                      labelText: '匹配模式 (正则)'),
                ),
                TextField(
                  controller: replacementCtrl,
                  decoration:
                      const InputDecoration(labelText: '替换文本'),
                ),
                DropdownButtonFormField<int>(
                  value: scope,
                  decoration:
                      const InputDecoration(labelText: '作用范围'),
                  items: const [
                    DropdownMenuItem(value: 0, child: Text('全局')),
                    DropdownMenuItem(value: 1, child: Text('书源')),
                    DropdownMenuItem(value: 2, child: Text('书籍')),
                  ],
                  onChanged: (val) {
                    setDialogState(() => scope = val ?? 0);
                  },
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () async {
                final name = nameCtrl.text.trim();
                final pattern = patternCtrl.text.trim();
                if (name.isEmpty || pattern.isEmpty) return;
                try {
                  await ref.read(dbInitializedProvider.future);
                  final dbPath = await ref.read(dbPathProvider.future);
                  final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
                  final id = '${now}_${Random().nextInt(99999)}';
                  final ruleJson = jsonEncode({
                    'id': id,
                    'name': name,
                    'pattern': pattern,
                    'replacement': replacementCtrl.text,
                    'enabled': true,
                    'scope': scope,
                    'sort_number': 0,
                    'created_at': now,
                    'updated_at': now,
                  });
                  await rust_api.saveReplaceRule(
                      dbPath: dbPath, ruleJson: ruleJson);
                  ref.invalidate(allReplaceRulesProvider);
                  if (ctx.mounted) Navigator.pop(ctx);
                } catch (e) {
                  if (ctx.mounted) {
                    ScaffoldMessenger.of(ctx).showSnackBar(
                      SnackBar(content: Text('添加失败: $e')),
                    );
                  }
                }
              },
              child: const Text('添加'),
            ),
          ],
        ),
      ),
    );
  }

  void _showRuleActions(
      BuildContext context, Map<String, dynamic> rule) {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(rule['name'] ?? '规则操作'),
        content: Text(rule['pattern'] ?? ''),
        actions: [
          TextButton(
            onPressed: () {
              Navigator.pop(ctx);
              _deleteRule(rule['id'] as String);
            },
            child:
                const Text('删除', style: TextStyle(color: Colors.red)),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('取消'),
          ),
        ],
      ),
    );
  }

  Future<void> _deleteRule(String id) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.deleteReplaceRule(dbPath: dbPath, id: id);
      ref.invalidate(allReplaceRulesProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('规则已删除')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('删除失败: $e')),
        );
      }
    }
  }
}
