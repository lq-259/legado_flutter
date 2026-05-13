import 'dart:async';
import 'dart:convert';

import '../src/rust/api.dart' as rust_api;
import 'notification_service.dart';

class _QueuedDownload {
  final String taskId;
  final String bookName;
  final List<Map<String, dynamic>> chapters;
  final String sourceJson;
  final String downloadDir;
  final String dbPath;

  const _QueuedDownload({
    required this.taskId,
    required this.bookName,
    required this.chapters,
    required this.sourceJson,
    required this.downloadDir,
    required this.dbPath,
  });
}

class DownloadRunner {
  static final DownloadRunner _instance = DownloadRunner._();
  factory DownloadRunner() => _instance;
  DownloadRunner._();

  bool _isRunning = false;
  bool get isRunning => _isRunning;

  final List<_QueuedDownload> _queue = [];
  final _completionController = StreamController<String>.broadcast();

  Stream<String> get onTaskCompleted => _completionController.stream;

  void enqueue({
    required String taskId,
    required String bookName,
    required List<Map<String, dynamic>> chapters,
    required String sourceJson,
    required String downloadDir,
    required String dbPath,
  }) {
    _queue.add(_QueuedDownload(
      taskId: taskId,
      bookName: bookName,
      chapters: chapters,
      sourceJson: sourceJson,
      downloadDir: downloadDir,
      dbPath: dbPath,
    ));
    if (!_isRunning) {
      _processQueue();
    }
  }

  Future<void> _processQueue() async {
    _isRunning = true;
    while (_queue.isNotEmpty) {
      final task = _queue.removeAt(0);
      await _download(task);
    }
    _isRunning = false;
  }

  Future<void> _download(_QueuedDownload task) async {
    int successCount = 0;
    int failCount = 0;
    int skipCount = 0;
    final totalChapters = task.chapters.length;
    final notificationId = task.taskId.hashCode.abs();

    try {
      await rust_api.updateDownloadTaskStatus(
        dbPath: task.dbPath,
        taskId: task.taskId,
        status: 1,
      );
    } catch (_) {}

    await NotificationService.showDownloadProgress(
      id: notificationId,
      title: task.bookName,
      current: 0,
      total: totalChapters,
    );

    for (var i = 0; i < task.chapters.length; i++) {
      final ch = task.chapters[i];
      final chapterId = '${task.taskId}_$i';
      final chapterUrl = ch['url'] as String? ?? '';
      if (chapterUrl.isEmpty) {
        skipCount++;
        try {
          await rust_api.updateDownloadChapterStatus(
            dbPath: task.dbPath,
            chapterId: chapterId,
            status: 3,
            fileSize: 0,
            errorMessage: '章节链接为空',
          );
        } catch (_) {}
        final processed = successCount + failCount + skipCount;
        await NotificationService.showDownloadProgress(
          id: notificationId,
          title: task.bookName,
          current: processed,
          total: totalChapters,
        );
        continue;
      }
      try {
        await rust_api.downloadAndSaveChapter(
          dbPath: task.dbPath,
          taskId: task.taskId,
          downloadChapterId: chapterId,
          sourceJson: task.sourceJson,
          chapterUrl: chapterUrl,
          downloadDir: task.downloadDir,
        );
        successCount++;
      } catch (_) {
        failCount++;
        try {
          await rust_api.updateDownloadChapterStatus(
            dbPath: task.dbPath,
            chapterId: chapterId,
            status: 3,
            fileSize: 0,
            errorMessage: '下载失败',
          );
        } catch (_) {}
      }
      final processed = successCount + failCount + skipCount;
      await NotificationService.showDownloadProgress(
        id: notificationId,
        title: task.bookName,
        current: processed,
        total: totalChapters,
      );
    }

    if (failCount > 0 || skipCount > 0) {
      try {
        await rust_api.updateDownloadTaskStatus(
          dbPath: task.dbPath,
          taskId: task.taskId,
          status: 4,
          errorMessage:
              '部分章节下载失败 (成功: $successCount, 失败: $failCount, 跳过: $skipCount)',
        );
      } catch (_) {}
    }

    await NotificationService.showDownloadComplete(
      id: notificationId,
      title: task.bookName,
      successCount: successCount,
      failCount: failCount,
      skipCount: skipCount,
    );

    _completionController.add(task.taskId);
  }

  void dispose() {
    _completionController.close();
  }

  static Future<void> resetInterruptedTasks(String dbPath) async {
    try {
      final json = await rust_api.getDownloadTasks(dbPath: dbPath);
      final List<dynamic> tasks = jsonDecode(json);
      for (final task in tasks) {
        if (task is Map<String, dynamic> && task['status'] == 1) {
          await rust_api.updateDownloadTaskStatus(
            dbPath: dbPath,
            taskId: task['id'] as String,
            status: 4,
            errorMessage: '应用意外关闭，下载中断',
          );
        }
      }
    } catch (_) {}
  }
}
