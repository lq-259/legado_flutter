import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;

class SourcePage extends ConsumerStatefulWidget {
  const SourcePage({super.key});

  @override
  ConsumerState<SourcePage> createState() => _SourcePageState();
}

class _SourcePageState extends ConsumerState<SourcePage> {
  bool _selectMode = false;
  final Set<String> _selectedIds = {};

  @override
  Widget build(BuildContext context) {
    final sourcesAsync = ref.watch(allSourcesProvider);

    return Scaffold(
      appBar: AppBar(
        title: _selectMode ? Text('已选 ${_selectedIds.length} 项') : const Text('书源管理'),
        leading: _selectMode
            ? IconButton(
                icon: const Icon(Icons.close),
                onPressed: _exitSelectMode,
              )
            : null,
        actions: _selectMode
            ? [
                IconButton(
                  icon: const Icon(Icons.select_all),
                  tooltip: '全选',
                  onPressed: _selectAll,
                ),
                IconButton(
                  icon: const Icon(Icons.deselect),
                  tooltip: '取消全选',
                  onPressed: () => setState(() => _selectedIds.clear()),
                ),
                IconButton(
                  icon: const Icon(Icons.delete),
                  tooltip: '删除选中',
                  onPressed: _selectedIds.isEmpty ? null : () => _deleteSelected(context),
                ),
              ]
            : [
                IconButton(
                  icon: const Icon(Icons.file_upload_outlined),
                  tooltip: '导出书源 JSON',
                  onPressed: () => _showExportDialog(context),
                ),
                IconButton(
                  icon: const Icon(Icons.file_download_outlined),
                  tooltip: '粘贴 JSON 导入',
                  onPressed: () => _showImportDialog(context),
                ),
                IconButton(
                  icon: const Icon(Icons.folder_open),
                  tooltip: '从文件导入',
                  onPressed: () => _importFromFile(context),
                ),
                IconButton(
                  icon: const Icon(Icons.refresh),
                  onPressed: () => ref.invalidate(allSourcesProvider),
                ),
              ],
      ),
      body: sourcesAsync.when(
        data: (sources) => _buildSourceList(context, sources),
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败: $e')),
      ),
      floatingActionButton: _selectMode
          ? null
          : FloatingActionButton(
              onPressed: () => _showAddSourceDialog(context),
              child: const Icon(Icons.add),
            ),
    );
  }

  Widget _buildSourceList(BuildContext context, List<Map<String, dynamic>> sources) {
    if (sources.isEmpty) {
      return const Center(child: Text('暂无书源，点击右下角添加'));
    }
    return ListView.builder(
      padding: const EdgeInsets.all(8),
      itemCount: sources.length,
      itemBuilder: (context, index) {
        final source = sources[index];
        final id = source['id'] as String;
        final enabled = source['enabled'] == true;
        final hasRules = source['rule_search'] != null || source['rule_toc'] != null || source['rule_content'] != null;
        return Card(
          margin: EdgeInsets.zero,
          child: ListTile(
            dense: true,
            leading: _selectMode
                ? Checkbox(
                    value: _selectedIds.contains(id),
                    onChanged: (_) => _toggleSelect(id),
                  )
                : Icon(
                    enabled ? Icons.check_circle : Icons.cancel,
                    color: enabled ? Colors.green : Colors.grey,
                  ),
            title: Text(source['name'] ?? '未知书源'),
            subtitle: Text(hasRules ? '${source['url'] ?? ''} (含规则)' : (source['url'] ?? '')),
            trailing: _selectMode
                ? null
                : Switch(
                    value: enabled,
                    onChanged: (val) => _toggleSource(id, val),
                  ),
            onTap: _selectMode
                ? () => _toggleSelect(id)
                : () => _showSourceActions(context, source),
            onLongPress: _selectMode ? null : () => _enterSelectMode(id),
          ),
        );
      },
    );
  }

  Future<void> _toggleSource(String id, bool enabled) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.setSourceEnabled(dbPath: dbPath, id: id, enabled: enabled);
      ref.invalidate(allSourcesProvider);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('操作失败: $e')),
        );
      }
    }
  }

  void _showAddSourceDialog(BuildContext context) {
    final nameCtrl = TextEditingController();
    final urlCtrl = TextEditingController();

    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('添加书源'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(controller: nameCtrl, decoration: const InputDecoration(labelText: '书源名称')),
            TextField(controller: urlCtrl, decoration: const InputDecoration(labelText: '书源 URL')),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(
            onPressed: () async {
              final name = nameCtrl.text.trim();
              final url = urlCtrl.text.trim();
              if (name.isEmpty || url.isEmpty) return;
              try {
                await ref.read(dbInitializedProvider.future);
                final dbPath = await ref.read(dbPathProvider.future);
                await rust_api.createSource(dbPath: dbPath, name: name, url: url);
                ref.invalidate(allSourcesProvider);
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
    );
  }

  void _showImportDialog(BuildContext context) {
    final jsonCtrl = TextEditingController();
    bool importing = false;

    showDialog(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialogState) => AlertDialog(
          title: const Text('导入书源 JSON'),
          content: SizedBox(
            width: double.maxFinite,
            child: TextField(
              controller: jsonCtrl,
              maxLines: 8,
              enabled: !importing,
              decoration: const InputDecoration(
                hintText: '粘贴书源 JSON 数组 [...]',
                border: OutlineInputBorder(),
                alignLabelWithHint: true,
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: importing ? null : () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: importing
                  ? null
                  : () async {
                      final json = jsonCtrl.text.trim();
                      if (json.isEmpty) return;
                      setDialogState(() => importing = true);
                      try {
                        await ref.read(dbInitializedProvider.future);
                        final dbPath = await ref.read(dbPathProvider.future);
                        final count = await rust_api.importSourcesFromJson(
                          dbPath: dbPath,
                          json: json,
                        );
                        ref.invalidate(allSourcesProvider);
                        if (ctx.mounted) Navigator.pop(ctx);
                        if (mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                            SnackBar(content: Text('成功导入 $count 个书源')),
                          );
                        }
                      } catch (e) {
                        if (ctx.mounted) {
                          setDialogState(() => importing = false);
                        }
                        if (ctx.mounted) {
                          ScaffoldMessenger.of(ctx).showSnackBar(
                            SnackBar(content: Text('导入失败: $e')),
                          );
                        }
                      }
                    },
              child: importing
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('导入'),
            ),
          ],
        ),
      ),
    );
  }

  void _showSourceActions(BuildContext context, Map<String, dynamic> source) {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(source['name'] ?? '书源操作'),
        content: Text(source['url'] ?? ''),
        actions: [
          TextButton.icon(
            onPressed: () {
              Navigator.pop(ctx);
              _showValidateDialog(context, source);
            },
            icon: const Icon(Icons.checklist, size: 18),
            label: const Text('校验规则'),
          ),
          TextButton(
            onPressed: () {
              Navigator.pop(ctx);
              _deleteSource(source['id'] as String);
            },
            child: const Text('删除', style: TextStyle(color: Colors.red)),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('取消'),
          ),
        ],
      ),
    );
  }

  Future<void> _showValidateDialog(BuildContext context, Map<String, dynamic> source) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      final resultJson = await rust_api.validateSourceFromDb(
        dbPath: dbPath,
        sourceId: source['id'] as String,
      );
      final List<dynamic> issues = const JsonDecoder().convert(resultJson);
      if (!mounted) return;
      if (issues.isEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('书源规则校验通过，未发现问题')),
        );
        return;
      }
      showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text('${source['name'] ?? '书源'} 校验结果'),
          content: SizedBox(
            width: double.maxFinite,
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: issues.length,
              itemBuilder: (_, i) {
                final issue = issues[i] as Map<String, dynamic>;
                final severity = issue['severity'] as String;
                final Color color = severity == 'error'
                    ? Colors.red
                    : severity == 'warning'
                        ? Colors.orange
                        : Colors.blue;
                final IconData icon = severity == 'error'
                    ? Icons.error
                    : severity == 'warning'
                        ? Icons.warning
                        : Icons.info;
                return ListTile(
                  leading: Icon(icon, color: color, size: 20),
                  title: Text(issue['field'] as String,
                      style: TextStyle(fontSize: 12, color: Colors.grey[600])),
                  subtitle: Text(issue['message'] as String,
                      style: const TextStyle(fontSize: 13)),
                  dense: true,
                );
              },
            ),
          ),
          actions: [
            FilledButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('确定'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('校验失败: $e')),
        );
      }
    }
  }

  Future<void> _deleteSource(String id) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.deleteSource(dbPath: dbPath, id: id);
      ref.invalidate(allSourcesProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('书源已删除')),
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

  Future<void> _showExportDialog(BuildContext context) async {
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      final json = await rust_api.exportAllSources(dbPath: dbPath);
      await Clipboard.setData(ClipboardData(text: json));
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('已复制所有书源 JSON 到剪贴板')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('导出失败: $e')),
        );
      }
    }
  }

  Future<void> _importFromFile(BuildContext context) async {
    try {
      final result = await FilePicker.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['json'],
      );
      if (result == null || result.files.isEmpty) return;

      final file = File(result.files.single.path!);
      final json = await file.readAsString();
      if (json.trim().isEmpty) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('文件内容为空')),
          );
        }
        return;
      }

      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      final count = await rust_api.importSourcesFromJson(
        dbPath: dbPath,
        json: json,
      );
      ref.invalidate(allSourcesProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('成功导入 $count 个书源')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('文件导入失败: $e')),
        );
      }
    }
  }

  void _enterSelectMode(String id) {
    setState(() {
      _selectMode = true;
      _selectedIds.add(id);
    });
  }

  void _exitSelectMode() {
    setState(() {
      _selectMode = false;
      _selectedIds.clear();
    });
  }

  void _toggleSelect(String id) {
    setState(() {
      if (_selectedIds.contains(id)) {
        _selectedIds.remove(id);
      } else {
        _selectedIds.add(id);
      }
    });
  }

  void _selectAll() {
    final sources = ref.read(allSourcesProvider).valueOrNull ?? [];
    setState(() {
      for (final s in sources) {
        _selectedIds.add(s['id'] as String);
      }
    });
  }

  Future<void> _deleteSelected(BuildContext context) async {
    final count = _selectedIds.length;
    if (count == 0) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('批量删除书源'),
        content: Text('确定要删除选中的 $count 个书源吗？'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('删除', style: TextStyle(color: Colors.red)),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await ref.read(dbInitializedProvider.future);
      final dbPath = await ref.read(dbPathProvider.future);
      for (final id in _selectedIds) {
        await rust_api.deleteSource(dbPath: dbPath, id: id);
      }
      _exitSelectMode();
      ref.invalidate(allSourcesProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('已删除 $count 个书源')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('批量删除失败: $e')),
        );
      }
    }
  }
}
