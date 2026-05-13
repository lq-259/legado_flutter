import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;

class DownloadPage extends ConsumerWidget {
  const DownloadPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tasksAsync = ref.watch(downloadTasksProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('下载管理'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () => ref.invalidate(downloadTasksProvider),
          ),
        ],
      ),
      body: tasksAsync.when(
        data: (tasks) => _buildTaskList(context, ref, tasks),
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败: $e')),
      ),
    );
  }

  Widget _buildTaskList(BuildContext context, WidgetRef ref, List<Map<String, dynamic>> tasks) {
    if (tasks.isEmpty) {
      return const Center(child: Text('暂无下载任务，在阅读器中点击下载按钮开始'));
    }
    return ListView.builder(
      padding: const EdgeInsets.all(8),
      itemCount: tasks.length,
      itemBuilder: (context, index) {
        final task = tasks[index];
        return _buildTaskCard(context, ref, task);
      },
    );
  }

  Widget _buildTaskCard(BuildContext context, WidgetRef ref, Map<String, dynamic> task) {
    final status = task['status'] as int? ?? 0;
    final totalChapters = task['total_chapters'] as int? ?? 0;
    final downloadedChapters = task['downloaded_chapters'] as int? ?? 0;
    final progress = totalChapters > 0 ? downloadedChapters / totalChapters : 0.0;
    final errorMsg = task['error_message'] as String?;

    String statusText;
    Color statusColor;
    IconData statusIcon;
    switch (status) {
      case 1:
        statusText = '下载中';
        statusColor = Colors.blue;
        statusIcon = Icons.downloading;
        break;
      case 2:
        statusText = '已暂停';
        statusColor = Colors.orange;
        statusIcon = Icons.pause_circle;
        break;
      case 3:
        statusText = '已完成';
        statusColor = Colors.green;
        statusIcon = Icons.check_circle;
        break;
      case 4:
        statusText = '失败';
        statusColor = Colors.red;
        statusIcon = Icons.error;
        break;
      default:
        statusText = '等待中';
        statusColor = Colors.grey;
        statusIcon = Icons.hourglass_empty;
    }

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    task['book_name'] as String? ?? '未知书籍',
                    style: Theme.of(context).textTheme.titleMedium,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Icon(statusIcon, color: statusColor, size: 20),
                const SizedBox(width: 4),
                Text(statusText, style: TextStyle(color: statusColor, fontSize: 12)),
              ],
            ),
            const SizedBox(height: 8),
            LinearProgressIndicator(value: progress),
            const SizedBox(height: 4),
            Text(
              '$downloadedChapters / $totalChapters 章',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            if (errorMsg != null && errorMsg.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Text(
                  errorMsg,
                  style: TextStyle(color: Colors.red.shade300, fontSize: 12),
                ),
              ),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton.icon(
                  icon: const Icon(Icons.delete_outline, size: 18),
                  label: const Text('删除'),
                  onPressed: () => _deleteTask(context, ref, task['id'] as String),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _deleteTask(BuildContext context, WidgetRef ref, String taskId) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('确认删除'),
        content: const Text('确定要删除这个下载任务吗？'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('删除')),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.deleteDownloadTask(dbPath: dbPath, taskId: taskId);
      ref.invalidate(downloadTasksProvider);
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('删除失败: $e')),
        );
      }
    }
  }
}
