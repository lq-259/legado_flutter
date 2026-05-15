import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:flutter_tts/flutter_tts.dart';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/api/dto.dart';
import '../../core/download_runner.dart';
import '../../core/platform_webview_executor.dart';
import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;
import 'page/page_view.dart';
import 'page/page_view_controller.dart';
import 'change_source_dialog.dart';

class _LoadedChapter {
  final int index;
  final String title;
  final String content;
  late final List<String> paragraphs;

  _LoadedChapter(this.index, this.title, this.content) {
    paragraphs = content
        .split(RegExp(r'\n+'))
        .where((p) => p.trim().isNotEmpty)
        .toList();
  }
}

class _ContinuousItem {
  final int chapterIndex;
  final bool isTitle;
  final bool isDivider;
  final int? paragraphIndex;
  const _ContinuousItem({
    required this.chapterIndex,
    this.isTitle = false,
    this.isDivider = false,
    this.paragraphIndex,
  });
}

class ReaderPage extends ConsumerStatefulWidget {
  final String bookId;
  final int chapterIndex;

  const ReaderPage({
    super.key,
    required this.bookId,
    this.chapterIndex = 0,
  });

  @override
  ConsumerState<ReaderPage> createState() => _ReaderPageState();
}

class _ReaderPageState extends ConsumerState<ReaderPage> {
  int _currentIndex = 0;
  int _visibleChapterIndex = 0;
  String _chapterContent = '';
  bool _isLoadingContent = false;
  List<Map<String, dynamic>>? _cachedChapters;
  int _chapterRequestId = 0;
  final ScrollController _scrollController = ScrollController();
  bool _progressRestored = false;
  Timer? _scrollDebounceTimer;
  Timer? _visibleChapterTimer;
  double _accumulatedOverscroll = 0;
  DateTime? _lastAutoChapterTime;
  bool _controlsVisible = false;
  ReaderSettings _settings = const ReaderSettings();
  bool _readerSettingsLoaded = false;
  List<_LoadedChapter> _loadedChapters = [];
  final GlobalKey _listViewKey = GlobalKey();
  final Map<int, GlobalKey> _chapterTitleKeys = {};
  double _lastScrollOffset = 0;
  bool _isScrollingBackward = false;
  String _bookName = '';
  String _sourceName = '';
  String _sourceUrl = '';
  String _sourceId = '';
  String _chapterUrl = '';
  List<_ContinuousItem>? _cachedContinuousItems;
  bool _isAppendingChapter = false;
  bool _isPrependingChapter = false;
  List<Map<String, dynamic>> _bookmarks = [];
  bool _hasBookmarkForChapter = false;
  double? _sliderValue;
  final ValueNotifier<DateTime> _nowNotifier = ValueNotifier(DateTime.now());
  Timer? _clockTimer;
  bool _isSearching = false;
  final TextEditingController _searchController = TextEditingController();
  List<({int chapterIdx, int paragraphIdx})> _searchMatches = [];
  int _currentSearchMatchIndex = -1;
  bool _isAutoScrolling = false;
  Timer? _autoScrollTimer;
  FlutterTts _flutterTts = FlutterTts();
  bool _isSpeaking = false;
  bool _isPaused = false;
  int _ttsParagraphIndex = 0;
  PageViewController? _pageViewController;

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.chapterIndex;
    _scrollController.addListener(_onScroll);
    loadReaderSettingsFromDisk().then((s) {
      if (mounted) {
        _setReaderSettings(s, markLoaded: true);
      }
    });
    _loadBookmarks();
    _initTts();
    _fetchBookName();
    _pageViewController = PageViewController(
      settings: _settings,
      initialChapterIndex: widget.chapterIndex,
    );
    _clockTimer = Timer.periodic(const Duration(seconds: 30), (_) {
      _nowNotifier.value = DateTime.now();
    });
  }

  @override
  void dispose() {
    _scrollController.removeListener(_onScroll);
    _scrollController.dispose();
    _scrollDebounceTimer?.cancel();
    _visibleChapterTimer?.cancel();
    _clockTimer?.cancel();
    _autoScrollTimer?.cancel();
    _searchController.dispose();
    _flutterTts.stop();
    try {
      _flutterTts.setCompletionHandler(() {});
    } catch (_) {}
    _nowNotifier.dispose();
    if (_pageViewController != null) {
      try {
        _pageViewController!.dispose();
      } catch (_) {}
    }
    super.dispose();
  }

  @override
  void didUpdateWidget(ReaderPage oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.bookId != widget.bookId) {
      _chapterRequestId++;
      _cachedChapters = null;
      _chapterContent = '';
      _loadedChapters = [];
      _currentIndex = widget.chapterIndex;
      _isLoadingContent = false;
      _bookmarks = [];
      _hasBookmarkForChapter = false;
      _loadBookmarks();
    }
  }

  Future<void> _fetchBookName() async {
    try {
      final book = await ref.read(bookByIdProvider(widget.bookId).future);
      if (book != null && mounted) {
        setState(() => _bookName = book['name'] as String? ?? '');
      }
    } catch (_) {}
  }

  Future<void> _fetchSourceInfo() async {
    try {
      final book = await ref.read(bookByIdProvider(widget.bookId).future);
      if (book != null && mounted) {
        _sourceName = book['source_name'] as String? ?? '';
        _sourceUrl = book['source_url'] as String? ?? '';
        _sourceId = book['source_id'] as String? ?? '';
        if (_cachedChapters != null &&
            _currentIndex < _cachedChapters!.length) {
          _chapterUrl = _cachedChapters![_currentIndex]['url'] as String? ?? '';
        }
        setState(() {});
      }
    } catch (_) {}
  }

  Future<String> _loadChapterContent(
      int index, List<Map<String, dynamic>> chapters) async {
    final chContent = chapters[index]['content'] as String?;
    if (chContent != null && chContent.isNotEmpty) {
      String content = chContent;
      try {
        final dbPath2 = await ref.read(dbPathProvider.future);
        final rulesJson = await rust_api.getReplaceRules(dbPath: dbPath2);
        final List<dynamic> rules = jsonDecode(rulesJson);
        for (final rule in rules) {
          if (rule is Map<String, dynamic> && rule['enabled'] == true) {
            try {
              final pattern = rule['pattern'] as String? ?? '';
              final replacement = rule['replacement'] as String? ?? '';
              if (pattern.isNotEmpty) {
                content = content.replaceAll(RegExp(pattern), replacement);
              }
            } catch (_) {}
          }
        }
      } catch (_) {}
      return content;
    }
    final book = await ref.read(bookByIdProvider(widget.bookId).future);
    final dbPath = await ref.read(dbPathProvider.future);
    final sourceId = _sourceId.isNotEmpty
        ? _sourceId
        : (book?['source_id'] as String? ?? '');
    final chapterUrl = chapters[index]['url'] as String? ?? '';

    String content = '无法获取章节内容（缺少书源或章节链接）';
    if (sourceId.isNotEmpty && chapterUrl.isNotEmpty) {
      final json = await rust_api.getChapterContentWithSourceFromDb(
        dbPath: dbPath,
        sourceId: sourceId,
        chapterUrl: chapterUrl,
      );
      if (json.isNotEmpty && json != 'null') {
        try {
          final data = jsonDecode(json);
          if (data is Map<String, dynamic>) {
            final platformRequest = data['platform_request'];
            if (platformRequest is Map) {
              content = await _executePlatformRequest(
                PlatformRequest.fromJson(
                    Map<String, dynamic>.from(platformRequest)),
              );
              if (_isCacheablePlatformContent(content)) {
                try {
                  await rust_api.updateChapterContent(
                    dbPath: dbPath,
                    chapterId: chapters[index]['id'] as String? ?? '',
                    content: content,
                  );
                  chapters[index]['content'] = content;
                } catch (_) {}
              }
            } else {
              content = data['content'] as String? ?? '（无内容）';
            }
          } else {
            content = data.toString();
          }
        } catch (_) {
          content = json;
        }
      }
    }

    try {
      final rulesJson = await rust_api.getReplaceRules(dbPath: dbPath);
      final List<dynamic> rules = jsonDecode(rulesJson);
      for (final rule in rules) {
        if (rule is Map<String, dynamic> && rule['enabled'] == true) {
          try {
            final pattern = rule['pattern'] as String? ?? '';
            final replacement = rule['replacement'] as String? ?? '';
            if (pattern.isNotEmpty) {
              content = content.replaceAll(RegExp(pattern), replacement);
            }
          } catch (_) {}
        }
      }
    } catch (_) {}

    return content;
  }

  Future<void> _openChapter(
      int index, List<Map<String, dynamic>> chapters) async {
    if (chapters.isEmpty) {
      if (mounted) {
        setState(() {
          _chapterContent = '暂无章节';
          _isLoadingContent = false;
          _currentIndex = 0;
        });
      }
      return;
    }
    if (index < 0 || index >= chapters.length) {
      if (mounted) {
        setState(() {
          _chapterContent = '章节索引越界';
          _isLoadingContent = false;
          _currentIndex = index.clamp(0, chapters.length - 1);
        });
      }
      return;
    }
    _cachedChapters = chapters;
    final requestId = ++_chapterRequestId;
    setState(() {
      _isLoadingContent = true;
      _currentIndex = index;
    });
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      final content = await _loadChapterContent(index, chapters);
      if (!mounted || requestId != _chapterRequestId) return;
      await rust_api.saveReadingProgress(
        dbPath: dbPath,
        bookId: widget.bookId,
        chapterIndex: index,
        paragraphIndex: 0,
        offset: 0,
      );
      if (!mounted || requestId != _chapterRequestId) return;
      final title = index < chapters.length
          ? (chapters[index]['title'] as String? ?? '')
          : '';
      setState(() {
        _chapterContent = content;
        _isLoadingContent = false;
        if (_settings.pageMode == ReaderPageMode.continuousScroll) {
          _loadedChapters = [_LoadedChapter(index, title, content)];
          _cachedContinuousItems = null;
        } else if (_settings.pageMode == ReaderPageMode.page) {
          _pageViewController?.updateSettings(_settings);
          _pageViewController?.loadChapter(index, title, content);
        }
      });
      _preCacheNextChapter(index, chapters);
      _preloadAdjacentContent(index, chapters);
      _fetchSourceInfo();
    } catch (e) {
      if (!mounted || requestId != _chapterRequestId) return;
      setState(() {
        _chapterContent = '加载失败: $e';
        _isLoadingContent = false;
      });
    }
  }

  void _setReaderSettings(ReaderSettings settings,
      {bool persist = false, bool markLoaded = false}) {
    final oldPageMode = _settings.pageMode;
    setState(() {
      if (markLoaded) {
        _readerSettingsLoaded = true;
      }
      _settings = settings;
      if (oldPageMode != ReaderPageMode.continuousScroll &&
          settings.pageMode == ReaderPageMode.continuousScroll) {
        _ensureCurrentChapterInContinuous();
      } else if (oldPageMode != ReaderPageMode.page &&
          settings.pageMode == ReaderPageMode.page &&
          _chapterContent.isNotEmpty) {
        final title =
            _cachedChapters != null && _currentIndex < _cachedChapters!.length
                ? (_cachedChapters![_currentIndex]['title'] as String? ?? '')
                : '';
        _pageViewController?.loadChapter(_currentIndex, title, _chapterContent);
      }
    });
    ref.read(readerSettingsProvider.notifier).state = settings;
    _pageViewController?.updateSettings(settings);
    if (persist) {
      saveReaderSettingsToDisk(settings);
    }
  }

  void _ensureCurrentChapterInContinuous() {
    if (_chapterContent.isEmpty) return;
    final currentLoaded = _loadedChapters.length == 1 &&
        _loadedChapters.first.index == _currentIndex &&
        _loadedChapters.first.content == _chapterContent;
    if (currentLoaded) return;
    final title =
        _cachedChapters != null && _currentIndex < _cachedChapters!.length
            ? (_cachedChapters![_currentIndex]['title'] as String? ?? '')
            : '';
    _loadedChapters = [_LoadedChapter(_currentIndex, title, _chapterContent)];
    _visibleChapterIndex = _currentIndex;
    _cachedContinuousItems = null;
    _chapterTitleKeys.clear();
  }

  Future<void> _appendNextChapter() async {
    if (_cachedChapters == null || _isAppendingChapter || _isLoadingContent)
      return;
    final chapters = _cachedChapters!;
    final lastLoaded =
        _loadedChapters.isNotEmpty ? _loadedChapters.last.index : _currentIndex;
    final nextIndex = lastLoaded + 1;
    if (nextIndex >= chapters.length) return;
    _isAppendingChapter = true;
    setState(() {});
    try {
      final content = await _loadChapterContent(nextIndex, chapters);
      if (!mounted) return;
      final title = chapters[nextIndex]['title'] as String? ?? '';
      setState(() {
        _loadedChapters.add(_LoadedChapter(nextIndex, title, content));
        _visibleChapterIndex = nextIndex;
        _cachedContinuousItems = null;
        _isAppendingChapter = false;
      });
      _preCacheNextChapter(nextIndex, chapters);
    } catch (_) {
      if (mounted) setState(() => _isAppendingChapter = false);
    }
  }

  Future<void> _prependPrevChapter() async {
    if (_cachedChapters == null || _isPrependingChapter || _isLoadingContent)
      return;
    final chapters = _cachedChapters!;
    final firstLoaded = _loadedChapters.isNotEmpty
        ? _loadedChapters.first.index
        : _currentIndex;
    final prevIndex = firstLoaded - 1;
    if (prevIndex < 0) return;
    _isPrependingChapter = true;
    setState(() {});
    try {
      final content = await _loadChapterContent(prevIndex, chapters);
      if (!mounted) return;
      final title = chapters[prevIndex]['title'] as String? ?? '';
      final prevChapter = _LoadedChapter(prevIndex, title, content);
      final oldExtent = _scrollController.hasClients
          ? _scrollController.position.maxScrollExtent
          : 0.0;
      final oldOffset =
          _scrollController.hasClients ? _scrollController.offset : 0.0;
      setState(() {
        _loadedChapters.insert(0, prevChapter);
        _visibleChapterIndex = prevIndex;
        _cachedContinuousItems = null;
        _isPrependingChapter = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_scrollController.hasClients) {
          final newExtent = _scrollController.position.maxScrollExtent;
          final addedHeight = newExtent - oldExtent;
          if (addedHeight > 0) {
            _scrollController.jumpTo(oldOffset + addedHeight);
          }
        }
      });
    } catch (_) {
      if (mounted) setState(() => _isPrependingChapter = false);
    }
  }

  Future<void> _resetContinuousStream(int index) async {
    if (_cachedChapters == null) return;
    await _openChapter(index, _cachedChapters!);
  }

  Future<String> _executePlatformRequest(PlatformRequest request) async {
    if (request.type == 'web_view_content') {
      try {
        final result =
            await PlatformWebViewExecutor().execute(context, request);
        if (result.sourceRegexRequired) {
          if (result.sourceRegexMatched && result.resourceUrl != null) {
            return result.resourceUrl!;
          }
          return result.content.isEmpty
              ? 'WebView 已执行，但未嗅探到匹配 sourceRegex 的资源。'
              : '${result.content}\n\n（提示：该书源配置了 sourceRegex，但未嗅探到匹配资源，结果可能不完整。）';
        }
        return result.content.isEmpty
            ? _platformRequestMessage(request)
            : result.content;
      } catch (e) {
        return '${_platformRequestMessage(request)}\n\n执行失败: $e';
      }
    }
    return _platformRequestMessage(request);
  }

  String _platformRequestMessage(PlatformRequest request) {
    if (request.type == 'web_view_content') {
      return '本章需要 Android WebView 执行后才能解析。\n\n'
          'URL: ${request.url ?? ''}\n\n'
          '当前版本已接入 Android 原生 WebView 执行器，原生不可用时会回退到 Flutter WebView。';
    }
    return '本章需要平台能力执行: ${request.type}';
  }

  bool _isCacheablePlatformContent(String content) {
    if (content.isEmpty) return false;
    return !content.startsWith('本章需要') &&
        !content.startsWith('WebView 已执行') &&
        !content.startsWith('WEBVIEW_JS_ERROR:');
  }

  void _goToNextChapter() {
    if (_cachedChapters == null) return;
    final chapters = _cachedChapters!;
    if (_currentIndex < chapters.length - 1) {
      final target = _currentIndex + 1;
      _visibleChapterIndex = target;
      if (_settings.pageMode == ReaderPageMode.continuousScroll) {
        _resetContinuousStream(target);
      } else {
        _openChapter(target, chapters);
      }
    }
  }

  void _goToPrevChapter() {
    if (_cachedChapters == null) return;
    final chapters = _cachedChapters!;
    if (_currentIndex > 0) {
      final target = _currentIndex - 1;
      _visibleChapterIndex = target;
      if (_settings.pageMode == ReaderPageMode.continuousScroll) {
        _resetContinuousStream(target);
      } else {
        _openChapter(target, chapters);
      }
    }
  }

  bool _onOverscroll(OverscrollNotification notification) {
    if (_isLoadingContent) return false;
    final chapters = _cachedChapters;
    if (chapters == null || _currentIndex >= chapters.length - 1) return false;
    final now = DateTime.now();
    if (_lastAutoChapterTime != null &&
        now.difference(_lastAutoChapterTime!).inMilliseconds < 1000) {
      return false;
    }
    _accumulatedOverscroll += notification.overscroll;
    if (_accumulatedOverscroll > 80) {
      _lastAutoChapterTime = now;
      _accumulatedOverscroll = 0;
      _goToNextChapter();
    }
    return false;
  }

  Future<void> _startDownload(BuildContext context) async {
    try {
      final book = await ref.read(bookByIdProvider(widget.bookId).future);
      if (book == null) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('未找到书籍信息')),
          );
        }
        return;
      }

      final sourceId = book['source_id'] as String? ?? '';
      if (sourceId.isEmpty) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('该书未关联书源，无法缓存')),
          );
        }
        return;
      }

      final chapters = _cachedChapters ??
          (await ref.read(bookChaptersProvider(widget.bookId).future) ?? []);
      if (chapters.isEmpty) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('暂无章节可缓存')),
          );
        }
        return;
      }

      final dbPath = await ref.read(dbPathProvider.future);
      final downloadDir = await ref.read(downloadDirProvider.future);

      final existingJson = await rust_api.getDownloadTaskByBook(
        dbPath: dbPath,
        bookId: widget.bookId,
      );
      final List<dynamic> existingTasks = jsonDecode(existingJson);
      if (existingTasks.isNotEmpty) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('该书已有缓存任务')),
          );
        }
        return;
      }

      final sourceJson = await rust_api.getSourceForDownload(
        dbPath: dbPath,
        sourceId: sourceId,
      );

      final taskId = DateTime.now().microsecondsSinceEpoch.toRadixString(36);
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final task = {
        'id': taskId,
        'book_id': widget.bookId,
        'book_name': book['name'] ?? '',
        'cover_url': book['cover_url'],
        'total_chapters': chapters.length,
        'downloaded_chapters': 0,
        'status': 1,
        'total_size': 0,
        'downloaded_size': 0,
        'error_message': null,
        'created_at': now,
        'updated_at': now,
      };

      final chapterRecords = <Map<String, dynamic>>[];
      for (var i = 0; i < chapters.length; i++) {
        final ch = chapters[i];
        chapterRecords.add({
          'id': '${taskId}_$i',
          'task_id': taskId,
          'chapter_id': ch['id'] ?? '',
          'chapter_index': i,
          'chapter_title': ch['title'] ?? '',
          'status': 0,
          'file_path': null,
          'file_size': 0,
          'error_message': null,
          'created_at': now,
          'updated_at': now,
        });
      }

      await rust_api.createDownloadTaskWithChapters(
        dbPath: dbPath,
        taskJson: jsonEncode(task),
        chaptersJson: jsonEncode(chapterRecords),
      );

      final bookName = book['name'] as String? ?? '';

      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('开始缓存 $bookName (${chapters.length}章)')),
        );
      }

      DownloadRunner().enqueue(
        taskId: taskId,
        bookName: bookName,
        chapters: chapters,
        sourceJson: sourceJson,
        downloadDir: downloadDir,
        dbPath: dbPath,
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('缓存失败: $e')),
        );
      }
    }
  }

  void _onScroll() {
    if (_scrollDebounceTimer != null) return;
    _scrollDebounceTimer = Timer(const Duration(milliseconds: 500), () {
      _scrollDebounceTimer = null;
      _saveScrollPosition();
    });
    if (_visibleChapterTimer != null) return;
    _visibleChapterTimer = Timer(const Duration(milliseconds: 300), () {
      _visibleChapterTimer = null;
      _updateVisibleChapter();
    });

    if (_scrollController.hasClients) {
      final currentOffset = _scrollController.offset;
      _isScrollingBackward = currentOffset < _lastScrollOffset;
      _lastScrollOffset = currentOffset;
    }

    if (_settings.pageMode == ReaderPageMode.continuousScroll &&
        _scrollController.hasClients &&
        !_isAppendingChapter &&
        !_isPrependingChapter) {
      final maxScroll = _scrollController.position.maxScrollExtent;
      final currentScroll = _scrollController.position.pixels;
      if (maxScroll - currentScroll < 300) {
        _appendNextChapter();
      } else if (currentScroll < 300) {
        _prependPrevChapter();
      }
    }
  }

  void _updateVisibleChapter() {
    if (!_scrollController.hasClients) return;
    if (_loadedChapters.isEmpty) return;
    if (_settings.pageMode != ReaderPageMode.continuousScroll) return;
    final listBox =
        _listViewKey.currentContext?.findRenderObject() as RenderBox?;
    if (listBox == null || !listBox.hasSize) return;
    final listTop = listBox.localToGlobal(Offset.zero).dy;
    final viewportHeight = _scrollController.position.viewportDimension;
    final threshold = viewportHeight / 3;

    int? inZoneChapter;
    for (final ch in _loadedChapters) {
      final key = _chapterTitleKeys[ch.index];
      if (key == null) continue;
      final ctx = key.currentContext;
      if (ctx == null) continue;
      final box = ctx.findRenderObject() as RenderBox?;
      if (box == null || !box.hasSize) continue;
      final titleTop = box.localToGlobal(Offset.zero).dy - listTop;
      if (titleTop >= 0 && titleTop <= threshold) {
        inZoneChapter = ch.index;
        break;
      }
    }

    if (inZoneChapter != null) {
      if (_visibleChapterIndex != inZoneChapter) {
        _switchChapter(inZoneChapter);
      }
      return;
    }

    if (_isScrollingBackward) {
      final currentIdx =
          _loadedChapters.indexWhere((ch) => ch.index == _visibleChapterIndex);
      if (currentIdx > 0) {
        _switchChapter(_loadedChapters[currentIdx - 1].index);
      }
    }
  }

  void _switchChapter(int chapterIndex) {
    if (_visibleChapterIndex == chapterIndex) return;
    _visibleChapterIndex = chapterIndex;
    if (_cachedChapters != null && chapterIndex < _cachedChapters!.length) {
      _chapterUrl = _cachedChapters![chapterIndex]['url'] as String? ?? '';
    }
    setState(() {});
  }

  Future<void> _saveScrollPosition() async {
    if (!_scrollController.hasClients || _chapterContent.isEmpty) return;
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      final idx = _settings.pageMode == ReaderPageMode.continuousScroll
          ? _visibleChapterIndex
          : _currentIndex;
      await rust_api.saveReadingProgress(
        dbPath: dbPath,
        bookId: widget.bookId,
        chapterIndex: idx,
        paragraphIndex: 0,
        offset: _scrollController.offset.toInt(),
      );
    } catch (_) {}
  }

  Future<void> _restoreProgress(List<Map<String, dynamic>> chapters) async {
    if (chapters.isEmpty) return;
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      final json = await rust_api.getReadingProgress(
        dbPath: dbPath,
        bookId: widget.bookId,
      );
      if (json.isEmpty || json == 'null') return;
      final progress = jsonDecode(json);
      if (progress is! Map<String, dynamic>) return;
      final savedIndex = progress['chapter_index'] as int? ?? 0;
      final savedOffset = progress['offset'] as int? ?? 0;
      if (savedIndex >= 0 && savedIndex < chapters.length) {
        await _openChapter(savedIndex, chapters);
        if (savedOffset > 0) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (_scrollController.hasClients && mounted) {
              _scrollController.jumpTo(savedOffset.toDouble());
            }
          });
        }
      }
    } catch (_) {}
  }

  Future<void> _preCacheNextChapter(
      int index, List<Map<String, dynamic>> chapters) async {
    final nextIndex = index + 1;
    if (nextIndex >= chapters.length) return;
    final chContent = chapters[nextIndex]['content'] as String?;
    if (chContent != null && chContent.isNotEmpty) return;
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      final book = await ref.read(bookByIdProvider(widget.bookId).future);
      final sourceId = book?['source_id'] as String? ?? '';
      final chapterUrl = chapters[nextIndex]['url'] as String? ?? '';
      if (sourceId.isEmpty || chapterUrl.isEmpty) return;
      final json = await rust_api.getChapterContentWithSourceFromDb(
        dbPath: dbPath,
        sourceId: sourceId,
        chapterUrl: chapterUrl,
      );
      if (json.isEmpty || json == 'null') return;
      var content = '';
      try {
        final data = jsonDecode(json);
        if (data is Map<String, dynamic>) {
          content = data['content'] as String? ?? json;
        } else {
          content = data.toString();
        }
      } catch (_) {
        content = json;
      }
      if (content.isNotEmpty) {
        await rust_api.updateChapterContent(
          dbPath: dbPath,
          chapterId: chapters[nextIndex]['id'] as String? ?? '',
          content: content,
        );
      }
    } catch (_) {}
  }

  void _preloadAdjacentContent(
      int currentIndex, List<Map<String, dynamic>> chapters) {
    final prevIndex = currentIndex - 1;
    if (prevIndex >= 0) {
      final prevContent = chapters[prevIndex]['content'] as String?;
      if (prevContent == null || prevContent.isEmpty) {
        _loadChapterContent(prevIndex, chapters).then((content) {
          if (content.isNotEmpty) {
            chapters[prevIndex]['content'] = content;
          }
        }).catchError((_) {});
      }
    }
    final nextIndex = currentIndex + 1;
    if (nextIndex < chapters.length) {
      final nextContent = chapters[nextIndex]['content'] as String?;
      if (nextContent == null || nextContent.isEmpty) {
        _loadChapterContent(nextIndex, chapters).then((content) {
          if (content.isNotEmpty) {
            chapters[nextIndex]['content'] = content;
          }
        }).catchError((_) {});
      }
    }
  }

  Future<void> _refreshChapter() async {
    if (_cachedChapters == null || _cachedChapters!.isEmpty) return;
    final index = _currentIndex;
    final chapters = _cachedChapters!;
    final chapterId = chapters[index]['id'] as String?;
    if (chapterId != null) {
      try {
        final dbPath = await ref.read(dbPathProvider.future);
        await rust_api.updateChapterContent(
          dbPath: dbPath,
          chapterId: chapterId,
          content: '',
        );
        chapters[index]['content'] = null;
      } catch (_) {}
    }
    if (_settings.pageMode == ReaderPageMode.continuousScroll) {
      _loadedChapters = [];
    }
    await _openChapter(index, chapters);
  }

  Future<void> _loadBookmarks() async {
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      final json =
          await rust_api.getBookmarks(dbPath: dbPath, bookId: widget.bookId);
      if (json.isEmpty || json == 'null') return;
      final list = jsonDecode(json);
      if (list is List) {
        if (mounted) {
          setState(() => _bookmarks = list.cast<Map<String, dynamic>>());
          _checkBookmarkForChapter();
        }
      }
    } catch (_) {}
  }

  void _checkBookmarkForChapter() {
    _hasBookmarkForChapter =
        _bookmarks.any((b) => b['chapter_index'] == _currentIndex);
  }

  Future<void> _toggleBookmark() async {
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      if (_hasBookmarkForChapter) {
        final match = _bookmarks
            .where((b) => b['chapter_index'] == _currentIndex)
            .toList();
        for (final bm in match) {
          final bookmarkId = bm['id'] as String? ?? '';
          if (bookmarkId.isNotEmpty) {
            await rust_api.deleteBookmark(
                dbPath: dbPath, bookmarkId: bookmarkId);
            _bookmarks.removeWhere((b) => b['id'] == bookmarkId);
          }
        }
      } else {
        final content = _chapterContent.length > 50
            ? _chapterContent.substring(0, 50)
            : _chapterContent;
        final resultJson = await rust_api.addBookmark(
          dbPath: dbPath,
          bookId: widget.bookId,
          chapterIndex: _currentIndex,
          paragraphIndex: 0,
          content: content,
        );
        if (resultJson.isNotEmpty) {
          try {
            final result = jsonDecode(resultJson);
            if (result is Map<String, dynamic>) {
              _bookmarks.add(result);
            }
          } catch (_) {}
        }
      }
      if (mounted) {
        _checkBookmarkForChapter();
        setState(() {});
      }
    } catch (_) {}
  }

  Future<void> _deleteBookmark(
      String bookmarkId, int bookmarkListIndex, BuildContext ctx) async {
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.deleteBookmark(dbPath: dbPath, bookmarkId: bookmarkId);
      _bookmarks.removeAt(bookmarkListIndex);
      _checkBookmarkForChapter();
      if (mounted) {
        setState(() {});
        if (ctx.mounted) {
          ScaffoldMessenger.of(ctx).showSnackBar(
            const SnackBar(content: Text('书签已删除')),
          );
        }
      }
    } catch (e) {
      if (ctx.mounted) {
        ScaffoldMessenger.of(ctx).showSnackBar(
          SnackBar(content: Text('删除失败: $e')),
        );
      }
    }
  }

  void _toggleNightMode() {
    final s = _settings.copyWith(nightMode: !_settings.nightMode);
    _setReaderSettings(s, persist: true);
  }

  void _navigateToChapter(int index, List<Map<String, dynamic>> chapters) {
    if (_settings.pageMode == ReaderPageMode.continuousScroll) {
      _resetContinuousStream(index);
    } else {
      _openChapter(index, chapters);
    }
  }

  void _showDirectorySheet() {
    _toggleControls();
    final chapters = _cachedChapters ?? [];
    final fg = Color(_settings.effectiveTextColor);
    final bg = Color(_settings.effectiveBackgroundColor);
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: bg,
      builder: (ctx) => DefaultTabController(
        length: 2,
        child: SizedBox(
          height: MediaQuery.of(context).size.height * 0.7,
          child: Column(
            children: [
              TabBar(
                tabs: const [
                  Tab(text: '目录'),
                  Tab(text: '书签'),
                ],
                labelColor: fg,
                unselectedLabelColor: fg.withValues(alpha: 0.5),
                indicatorColor: fg,
              ),
              Expanded(
                child: TabBarView(
                  children: [
                    _buildDirectoryTab(chapters, ctx),
                    _buildBookmarkTab(ctx, chapters),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildDirectoryTab(
      List<Map<String, dynamic>> chapters, BuildContext ctx) {
    final fg = Color(_settings.effectiveTextColor);
    if (chapters.isEmpty) {
      return Center(
          child:
              Text('暂无目录', style: TextStyle(color: fg.withValues(alpha: 0.5))));
    }
    return ListView.builder(
      itemExtent: 48,
      itemCount: chapters.length,
      itemBuilder: (context, index) {
        final ch = chapters[index];
        final isCurrent = index == _currentIndex;
        return ListTile(
          dense: true,
          title: Text(
            ch['title'] as String? ?? '章节 ${index + 1}',
            style: TextStyle(
              color: isCurrent ? Theme.of(ctx).primaryColor : fg,
              fontWeight: isCurrent ? FontWeight.bold : FontWeight.normal,
              fontSize: 14,
            ),
          ),
          onTap: () {
            Navigator.of(ctx).pop();
            _navigateToChapter(index, chapters);
          },
        );
      },
    );
  }

  Widget _buildBookmarkTab(
      BuildContext ctx, List<Map<String, dynamic>> chapters) {
    final fg = Color(_settings.effectiveTextColor);
    if (_bookmarks.isEmpty) {
      return Center(
          child:
              Text('暂无书签', style: TextStyle(color: fg.withValues(alpha: 0.5))));
    }
    return ListView.builder(
      itemExtent: 64,
      itemCount: _bookmarks.length,
      itemBuilder: (context, index) {
        final bm = _bookmarks[index];
        final chapterIndex = bm['chapter_index'] as int? ?? 0;
        final chapterTitle = chapterIndex < chapters.length
            ? (chapters[chapterIndex]['title'] as String? ??
                '章节 ${chapterIndex + 1}')
            : '章节 ${chapterIndex + 1}';
        final content = bm['content'] as String? ?? '';
        return ListTile(
          dense: true,
          title: Text(chapterTitle,
              style: TextStyle(color: fg, fontSize: 14),
              maxLines: 1,
              overflow: TextOverflow.ellipsis),
          subtitle: content.isNotEmpty
              ? Text(content,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style:
                      TextStyle(color: fg.withValues(alpha: 0.5), fontSize: 12))
              : null,
          trailing: IconButton(
            icon: Icon(Icons.delete_outline,
                color: fg.withValues(alpha: 0.5), size: 20),
            onPressed: () =>
                _deleteBookmark(bm['id'] as String? ?? '', index, ctx),
          ),
          onTap: () {
            Navigator.of(ctx).pop();
            _navigateToChapter(chapterIndex, chapters);
          },
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final providerSettings = ref.watch(readerSettingsProvider);
    if (_readerSettingsLoaded && providerSettings != _settings) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && ref.read(readerSettingsProvider) != _settings) {
          _setReaderSettings(ref.read(readerSettingsProvider));
        }
      });
    }

    if (widget.bookId.isEmpty) {
      return Scaffold(
        appBar: AppBar(title: const Text('阅读器')),
        body: const Center(child: Text('未指定书籍')),
      );
    }

    final chaptersAsync = ref.watch(bookChaptersProvider(widget.bookId));

    return chaptersAsync.when(
      data: (chapters) {
        if (chapters.isNotEmpty && !_progressRestored) {
          _progressRestored = true;
          _cachedChapters = chapters;
          Future.microtask(() => _restoreProgress(chapters));
        }
        if (chapters.isEmpty) {
          return Scaffold(
            appBar: AppBar(title: const Text('阅读器')),
            body: const Center(child: Text('暂无章节')),
          );
        }
        if (_isLoadingContent || _chapterContent.isNotEmpty) {
          return _buildReaderView();
        }
        return _buildChapterList(chapters);
      },
      loading: () => Scaffold(
        appBar: AppBar(title: const Text('阅读器')),
        body: const Center(child: CircularProgressIndicator()),
      ),
      error: (e, _) => Scaffold(
        appBar: AppBar(title: const Text('阅读器')),
        body: Center(child: Text('加载章节失败: $e')),
      ),
    );
  }

  Widget _buildChapterList(List<Map<String, dynamic>> chapters) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('目录'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () => context.pop(),
        ),
      ),
      body: ListView.builder(
        itemExtent: 56,
        itemCount: chapters.length,
        itemBuilder: (context, index) {
          final ch = chapters[index];
          return ListTile(
            title: Text(ch['title'] as String? ?? '章节 ${index + 1}'),
            onTap: () => _openChapter(index, chapters),
          );
        },
      ),
    );
  }

  Widget _buildReaderView() {
    final chapters = _cachedChapters!;
    final hasPrev = _currentIndex > 0;
    final hasNext = _currentIndex < chapters.length - 1;
    final settings = _settings;
    final isTapMode = settings.pageMode == ReaderPageMode.tapChapter;
    final isContinuous = settings.pageMode == ReaderPageMode.continuousScroll;
    final isPage = settings.pageMode == ReaderPageMode.page;
    final bgColor = Color(settings.effectiveBackgroundColor);
    final contentLocked = _controlsVisible || _isSearching;

    final showInfoBars = settings.showReadingInfo;

    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: SystemUiOverlayStyle(
        statusBarIconBrightness:
            _settings.nightMode ? Brightness.light : Brightness.dark,
        statusBarColor: Colors.transparent,
      ),
      child: Container(
        decoration: BoxDecoration(
          color: bgColor,
          image: settings.backgroundImagePath != null
              ? DecorationImage(
                  image: FileImage(File(settings.backgroundImagePath!)),
                  fit: BoxFit.cover,
                )
              : null,
        ),
        child: SafeArea(
          child: Column(
            children: [
              if (showInfoBars) _buildReadingInfoHeader(settings),
              Expanded(
                child: Stack(
                  children: [
                    IgnorePointer(
                      ignoring: contentLocked,
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTapUp: (details) {
                          final width = MediaQuery.of(context).size.width;
                          if (isPage) {
                            final pvc = _pageViewController;
                            if (pvc != null) {
                              if (details.localPosition.dx < width / 3) {
                                if (!pvc.goToPrevPage()) {
                                  _onPageChapterBoundary(PageDirection.prev);
                                }
                                return;
                              } else if (details.localPosition.dx >
                                  width * 2 / 3) {
                                if (!pvc.goToNextPage()) {
                                  _onPageChapterBoundary(PageDirection.next);
                                }
                                return;
                              }
                            }
                            _toggleControls();
                            return;
                          }
                          if (isTapMode) {
                            if (details.localPosition.dx < width / 3) {
                              _goToPrevChapter();
                              return;
                            } else if (details.localPosition.dx >
                                width * 2 / 3) {
                              _goToNextChapter();
                              return;
                            }
                          }
                          _toggleControls();
                        },
                        child: NotificationListener<OverscrollNotification>(
                          onNotification:
                              isContinuous ? (_) => true : _onOverscroll,
                          child: isPage
                              ? _buildPageBody(settings)
                              : isContinuous
                                  ? _buildContinuousBody(settings)
                                  : _buildSingleChapterBody(settings),
                        ),
                      ),
                    ),
                    if (contentLocked)
                      Positioned.fill(
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: _controlsVisible ? _toggleControls : () {},
                        ),
                      ),
                    if (_controlsVisible)
                      _buildTopControls(context, chapters, hasPrev, hasNext),
                    if (_controlsVisible)
                      _buildBottomControls(
                          context, chapters, hasPrev, hasNext, settings),
                    if (_isSpeaking) _buildTtsBar(),
                    if (_isSearching) _buildSearchBar(),
                  ],
                ),
              ),
              if (showInfoBars) _buildReadingInfoFooter(chapters, settings),
            ],
          ),
        ),
      ),
    );
  }

  List<_ContinuousItem> _buildContinuousItemList() {
    if (_cachedContinuousItems != null) return _cachedContinuousItems!;
    final items = <_ContinuousItem>[];
    for (var i = 0; i < _loadedChapters.length; i++) {
      final chapter = _loadedChapters[i];
      if (chapter.title.isNotEmpty) {
        items.add(_ContinuousItem(chapterIndex: i, isTitle: true));
      }
      for (var j = 0; j < chapter.paragraphs.length; j++) {
        items.add(_ContinuousItem(chapterIndex: i, paragraphIndex: j));
      }
      if (i < _loadedChapters.length - 1) {
        items.add(_ContinuousItem(chapterIndex: i, isDivider: true));
      }
    }
    _cachedContinuousItems = items;
    return items;
  }

  Widget _buildContinuousBody(ReaderSettings settings) {
    final textStyle = TextStyle(
      fontSize: settings.fontSize,
      fontWeight:
          FontWeight.values[((settings.fontWeight ~/ 100) - 1).clamp(0, 8)],
      fontFamily: settings.fontFamily,
      color: Color(settings.effectiveTextColor),
      letterSpacing: settings.letterSpacing,
      height: settings.lineHeight,
      decoration: TextDecoration.none,
    );
    final titleStyle = textStyle.copyWith(
      fontWeight: FontWeight.bold,
      fontSize: settings.fontSize * 1.15,
    );
    final dividerColor =
        Color(settings.effectiveTextColor).withValues(alpha: 0.15);
    final items = _buildContinuousItemList();

    return ListView.builder(
      key: _listViewKey,
      controller: _scrollController,
      physics: const BouncingScrollPhysics(),
      padding: EdgeInsets.symmetric(
        horizontal: settings.horizontalPadding,
        vertical: settings.verticalPadding,
      ),
      itemCount: items.length + (_isAppendingChapter ? 1 : 0),
      itemBuilder: (context, index) {
        if (index >= items.length) {
          return const Padding(
            padding: EdgeInsets.all(24),
            child: Center(
                child: SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2))),
          );
        }
        final item = items[index];
        if (item.isTitle) {
          final globalChIndex = _loadedChapters[item.chapterIndex].index;
          return Padding(
            key:
                _chapterTitleKeys.putIfAbsent(globalChIndex, () => GlobalKey()),
            padding: EdgeInsets.only(bottom: settings.verticalPadding),
            child: Text(_loadedChapters[item.chapterIndex].title,
                style: titleStyle),
          );
        }
        if (item.isDivider) {
          return Padding(
            padding: const EdgeInsets.symmetric(vertical: 12),
            child: Divider(color: dividerColor),
          );
        }
        final chapter = _loadedChapters[item.chapterIndex];
        final p = chapter.paragraphs[item.paragraphIndex!];
        return Padding(
          padding: EdgeInsets.only(bottom: settings.paragraphSpacing),
          child: Text('${settings.paragraphIndent}$p', style: textStyle),
        );
      },
    );
  }

  Widget _buildPageBody(ReaderSettings settings) {
    if (_chapterContent.isEmpty && _isLoadingContent) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    if (_pageViewController == null) {
      return const Center(child: Text('加载中...'));
    }
    return PageViewWidget(
      controller: _pageViewController!,
      settings: settings,
      pageAnim: settings.pageAnim,
      onChapterBoundary: _onPageChapterBoundary,
    );
  }

  Widget _buildSingleChapterBody(ReaderSettings settings) {
    final textStyle = TextStyle(
      fontSize: settings.fontSize,
      fontWeight:
          FontWeight.values[((settings.fontWeight ~/ 100) - 1).clamp(0, 8)],
      fontFamily: settings.fontFamily,
      color: Color(settings.effectiveTextColor),
      letterSpacing: settings.letterSpacing,
      height: settings.lineHeight,
      decoration: TextDecoration.none,
    );
    if (_chapterContent.isEmpty && _isLoadingContent) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    final paragraphs = _chapterContent
        .split(RegExp(r'\n+'))
        .where((p) => p.trim().isNotEmpty)
        .toList();
    return ListView.builder(
      controller: _scrollController,
      physics: const BouncingScrollPhysics(),
      padding: EdgeInsets.symmetric(
        horizontal: settings.horizontalPadding,
        vertical: settings.verticalPadding,
      ),
      itemCount: paragraphs.length,
      itemBuilder: (context, i) {
        return Padding(
          padding: EdgeInsets.only(
              bottom:
                  i < paragraphs.length - 1 ? settings.paragraphSpacing : 0),
          child: Text(
            '${settings.paragraphIndent}${paragraphs[i]}',
            style: textStyle,
          ),
        );
      },
    );
  }

  Future<void> _loadPageModeChapter(int targetIndex) async {
    if (_cachedChapters == null ||
        targetIndex < 0 ||
        targetIndex >= _cachedChapters!.length) return;
    final chapters = _cachedChapters!;
    final isPrev = targetIndex < _currentIndex;

    setState(() {
      _isLoadingContent = true;
      _currentIndex = targetIndex;
    });

    try {
      final content = await _loadChapterContent(targetIndex, chapters);
      if (!mounted) return;
      final title = chapters[targetIndex]['title'] as String? ?? '';
      final dbPath = await ref.read(dbPathProvider.future);
      await rust_api.saveReadingProgress(
        dbPath: dbPath,
        bookId: widget.bookId,
        chapterIndex: targetIndex,
        paragraphIndex: 0,
        offset: 0,
      );
      setState(() {
        _chapterContent = content;
        _isLoadingContent = false;
      });
      _pageViewController?.updateSettings(_settings);
      _pageViewController?.loadChapter(targetIndex, title, content,
          jumpToLast: isPrev);
      _preloadAdjacentContent(targetIndex, chapters);
    } catch (e) {
      if (mounted) setState(() => _isLoadingContent = false);
    }
  }

  void _onPageChapterBoundary(PageDirection dir) {
    if (_cachedChapters == null) return;
    if (dir == PageDirection.next &&
        _currentIndex < _cachedChapters!.length - 1) {
      _loadPageModeChapter(_currentIndex + 1);
    } else if (dir == PageDirection.prev && _currentIndex > 0) {
      _loadPageModeChapter(_currentIndex - 1);
    }
  }

  void _showChangeSourceDialog(BuildContext context) async {
    final dbPath = await ref.read(dbPathProvider.future);
    if (!mounted) return;
    final bookName = _bookName;
    final currentSourceId = _sourceId;
    final currentSourceName = _sourceName.isNotEmpty ? _sourceName : '书源';

    final result = await showDialog<ChangeSourceResult>(
      context: context,
      barrierDismissible: true,
      builder: (_) => ChangeSourceDialog(
        dbPath: dbPath,
        bookName: bookName,
        bookAuthor: '',
        currentSourceId: currentSourceId,
        currentSourceName: currentSourceName,
      ),
    );

    if (result != null && mounted) {
      await _replaceBookSource(result);
    }
  }

  Future<void> _replaceBookSource(ChangeSourceResult result) async {
    if (!mounted) return;
    final savedIndex = _currentIndex;
    final book = await ref.read(bookByIdProvider(widget.bookId).future);
    if (!mounted || book == null) {
      return;
    }

    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final dbPath = await ref.read(dbPathProvider.future);
    if (!mounted) return;

    try {
      final oldChaptersJson = await rust_api.getBookChapters(
        dbPath: dbPath,
        bookId: widget.bookId,
      );
      final List<dynamic> oldChapters = jsonDecode(oldChaptersJson);
      for (final ch in oldChapters) {
        final chMap = ch as Map<String, dynamic>;
        final chapterId = chMap['id'] as String?;
        if (chapterId != null) {
          await rust_api.deleteChapter(dbPath: dbPath, id: chapterId);
        }
      }
    } catch (_) {}

    final chapters = result.chapters;
    int savedCount = 0;
    final List<Map<String, dynamic>> cachedChaptersList = [];
    for (var i = 0; i < chapters.length; i++) {
      final ch = chapters[i];
      final chapterKey = '${widget.bookId}|$i|${ch['url'] ?? ''}';
      final chapterId = base64Url
          .encode(sha256.convert(utf8.encode(chapterKey)).bytes)
          .replaceAll('=', '');
      final chapterData = {
        'id': chapterId,
        'book_id': widget.bookId,
        'index_num': i,
        'title': ch['title'] ?? '未知章节',
        'url': ch['url'] ?? '',
        'content': null,
        'is_volume': false,
        'is_checked': false,
        'start': 0,
        'end': 0,
        'created_at': now,
        'updated_at': now,
      };
      await rust_api.saveChapter(
        dbPath: dbPath,
        chapterJson: jsonEncode(chapterData),
      );
      savedCount++;
      cachedChaptersList.add({
        'id': chapterId,
        'url': ch['url'] ?? '',
        'title': ch['title'] ?? '未知章节',
        'content': null,
      });
    }

    int targetIndex = savedIndex;
    if (targetIndex < 0 || targetIndex >= chapters.length) {
      targetIndex = 0;
    }

    final updatedBook = Map<String, dynamic>.from(book);
    updatedBook['source_id'] = result.sourceId;
    updatedBook['source_name'] = result.sourceName;
    updatedBook['book_url'] = result.bookUrl;
    updatedBook['chapter_count'] = savedCount;
    if (chapters.isNotEmpty) {
      updatedBook['latest_chapter_title'] = chapters.last['title'];
    }
    if (result.bookInfo != null) {
      updatedBook['cover_url'] =
          result.bookInfo!['cover_url'] ?? book['cover_url'];
      updatedBook['intro'] = result.bookInfo!['intro'] ?? book['intro'];
      updatedBook['author'] = result.bookInfo!['author'] ?? book['author'];
    }
    updatedBook['updated_at'] = now;

    await rust_api.saveBook(dbPath: dbPath, bookJson: jsonEncode(updatedBook));

    ref.invalidate(allBooksProvider);
    ref.invalidate(bookByIdProvider(widget.bookId));
    ref.invalidate(bookChaptersProvider(widget.bookId));

    _chapterRequestId++;
    final requestId = _chapterRequestId;
    setState(() {
      _isLoadingContent = true;
      _currentIndex = targetIndex;
      _cachedChapters = cachedChaptersList;
      _sourceName = result.sourceName;
      _sourceUrl = result.bookUrl;
    });

    try {
      final chapterUrl = chapters[targetIndex]['url'] as String? ?? '';
      final json = await rust_api.getChapterContentWithSourceFromDb(
        dbPath: dbPath,
        sourceId: result.sourceId,
        chapterUrl: chapterUrl,
      );
      if (!mounted || requestId != _chapterRequestId) return;

      var content = '';
      if (json.isNotEmpty && json != 'null') {
        try {
          final data = jsonDecode(json);
          if (data is Map<String, dynamic>) {
            final platformRequest = data['platform_request'];
            if (platformRequest is Map) {
              content = await _executePlatformRequest(
                PlatformRequest.fromJson(
                    Map<String, dynamic>.from(platformRequest)),
              );
            } else {
              content = data['content'] as String? ?? '（无内容）';
            }
          } else {
            content = data.toString();
          }
        } catch (_) {
          content = json;
        }
      } else {
        content = '（无内容）';
      }

      try {
        final rulesJson = await rust_api.getReplaceRules(dbPath: dbPath);
        final List<dynamic> rules = jsonDecode(rulesJson);
        for (final rule in rules) {
          if (rule is Map<String, dynamic> && rule['enabled'] == true) {
            try {
              final pattern = rule['pattern'] as String? ?? '';
              final replacement = rule['replacement'] as String? ?? '';
              if (pattern.isNotEmpty) {
                content = content.replaceAll(RegExp(pattern), replacement);
              }
            } catch (_) {}
          }
        }
      } catch (_) {}

      await rust_api.saveReadingProgress(
        dbPath: dbPath,
        bookId: widget.bookId,
        chapterIndex: targetIndex,
        paragraphIndex: 0,
        offset: 0,
      );

      final title = chapters[targetIndex]['title'] as String? ?? '';
      setState(() {
        _chapterContent = content;
        _isLoadingContent = false;
        if (_settings.pageMode == ReaderPageMode.continuousScroll) {
          _loadedChapters = [_LoadedChapter(targetIndex, title, content)];
          _cachedContinuousItems = null;
        } else if (_settings.pageMode == ReaderPageMode.page) {
          _pageViewController?.updateSettings(_settings);
          _pageViewController?.loadChapter(targetIndex, title, content);
        }
      });
    } catch (e) {
      if (mounted && requestId == _chapterRequestId) {
        setState(() {
          _chapterContent = '换源后加载失败: $e';
          _isLoadingContent = false;
        });
      }
    }
  }

  void _startSearch() {
    setState(() {
      _isSearching = true;
      _controlsVisible = false;
    });
  }

  void _closeSearch() {
    setState(() {
      _isSearching = false;
      _searchMatches = [];
      _currentSearchMatchIndex = -1;
      _searchController.clear();
    });
  }

  void _performSearch(String keyword) {
    _searchMatches = [];
    _currentSearchMatchIndex = -1;
    if (keyword.isEmpty) return;
    final k = keyword.toLowerCase();
    final chapters = _loadedChapters.isNotEmpty
        ? _loadedChapters
        : [_LoadedChapter(_currentIndex, _chapterContent, _chapterContent)];
    for (var ci = 0; ci < chapters.length; ci++) {
      final ps = chapters[ci].paragraphs;
      for (var pi = 0; pi < ps.length; pi++) {
        if (ps[pi].toLowerCase().contains(k)) {
          _searchMatches.add((chapterIdx: ci, paragraphIdx: pi));
        }
      }
    }
    if (_searchMatches.isNotEmpty) {
      _currentSearchMatchIndex = 0;
      _scrollToSearchMatch();
    }
  }

  void _goToNextSearchMatch() {
    if (_searchMatches.isEmpty) return;
    _currentSearchMatchIndex =
        (_currentSearchMatchIndex + 1) % _searchMatches.length;
    _scrollToSearchMatch();
  }

  void _goToPrevSearchMatch() {
    if (_searchMatches.isEmpty) return;
    _currentSearchMatchIndex =
        (_currentSearchMatchIndex - 1 + _searchMatches.length) %
            _searchMatches.length;
    _scrollToSearchMatch();
  }

  void _scrollToSearchMatch() {
    if (_currentSearchMatchIndex < 0 ||
        _currentSearchMatchIndex >= _searchMatches.length) return;
    if (!_scrollController.hasClients) return;
    final match = _searchMatches[_currentSearchMatchIndex];
    final estimatedOffset =
        200.0 * (match.chapterIdx * 50 + match.paragraphIdx);
    _scrollController.animateTo(
      estimatedOffset,
      duration: const Duration(milliseconds: 200),
      curve: Curves.easeInOut,
    );
    setState(() {});
  }

  void _toggleAutoScroll() {
    if (_isAutoScrolling) {
      _autoScrollTimer?.cancel();
      setState(() => _isAutoScrolling = false);
    } else {
      setState(() => _isAutoScrolling = true);
      _autoScrollStep();
    }
  }

  void _autoScrollStep() {
    if (!_isAutoScrolling || !_scrollController.hasClients) return;
    final maxScroll = _scrollController.position.maxScrollExtent;
    final currentScroll = _scrollController.position.pixels;
    if (currentScroll >= maxScroll) {
      setState(() => _isAutoScrolling = false);
      return;
    }
    _scrollController.jumpTo(currentScroll + 1.0);
    _autoScrollTimer = Timer(const Duration(milliseconds: 50), _autoScrollStep);
  }

  Future<void> _initTts() async {
    try {
      debugPrint('[TTS] _initTts: starting...');
      bool langOk = false;
      for (final code in ['zh-CN', 'zh', 'cmn', 'zh-Hans']) {
        final result = await _flutterTts.setLanguage(code);
        debugPrint('[TTS] setLanguage("$code") returned: $result');
        if (result == 1) {
          debugPrint('[TTS] Language set to $code successfully');
          langOk = true;
          break;
        }
      }
      if (!langOk) {
        debugPrint(
            '[TTS] No Chinese language matched. Using default engine voice.');
      }
      final rateResult = await _flutterTts.setSpeechRate(_settings.ttsSpeed);
      debugPrint('[TTS] setSpeechRate returned: $rateResult');
      _flutterTts.setCompletionHandler(() {
        if (mounted && _isSpeaking) {
          _ttsNextParagraph();
        }
      });
      debugPrint('[TTS] _initTts: done');
    } catch (e, st) {
      debugPrint('[TTS] _initTts FAILED: $e\n$st');
    }
  }

  Future<void> _startTts() async {
    try {
      _isSpeaking = true;
      _isPaused = false;
      _ttsParagraphIndex = 0;
      setState(() {});
      debugPrint('[TTS] _startTts: stopping previous');
      await _flutterTts.stop();
      debugPrint('[TTS] _startTts: speaking first paragraph');
      await _speakCurrentParagraph();
    } catch (e, st) {
      debugPrint('[TTS] _startTts FAILED: $e\n$st');
    }
  }

  Future<void> _speakCurrentParagraph() async {
    if (!_isSpeaking) return;
    final paragraphs = _chapterContent
        .split(RegExp(r'\n+'))
        .where((p) => p.trim().isNotEmpty)
        .toList();
    if (_ttsParagraphIndex >= paragraphs.length) {
      _goToNextChapter();
      _ttsParagraphIndex = 0;
      WidgetsBinding.instance
          .addPostFrameCallback((_) => _speakCurrentParagraph());
      return;
    }
    final text = paragraphs[_ttsParagraphIndex];
    debugPrint(
        '[TTS] Speaking paragraph $_ttsParagraphIndex: "${text.length > 30 ? text.substring(0, 30) : text}..."');
    final result = await _flutterTts.speak(text);
    debugPrint('[TTS] speak() result: $result');
  }

  void _pauseTts() {
    _isPaused = true;
    _flutterTts.pause();
    setState(() {});
  }

  void _resumeTts() {
    if (!_isSpeaking) {
      _startTts();
      return;
    }
    _isPaused = false;
    _speakCurrentParagraph();
    setState(() {});
  }

  void _stopTts() {
    _isSpeaking = false;
    _isPaused = false;
    _flutterTts.stop();
    setState(() {});
  }

  void _cycleTtsSpeed() {
    const speeds = [0.3, 0.5, 0.7, 0.8, 0.9, 1.0];
    final idx = speeds.indexOf(_settings.ttsSpeed);
    final next =
        speeds[(idx < 0 ? speeds.length - 1 : (idx + 1) % speeds.length)];
    final s = _settings.copyWith(ttsSpeed: next);
    _setReaderSettings(s, persist: true);
    if (_isSpeaking) {
      _flutterTts.setSpeechRate(next);
    }
  }

  void _ttsNextParagraph() {
    _ttsParagraphIndex++;
    setState(() {});
    _speakCurrentParagraph();
  }

  void _ttsPrevParagraph() {
    if (_ttsParagraphIndex > 0) {
      _ttsParagraphIndex--;
      setState(() {});
      _flutterTts.stop();
      _speakCurrentParagraph();
    }
  }

  void _toggleControls() {
    setState(() => _controlsVisible = !_controlsVisible);
    if (_controlsVisible) _saveScrollPosition();
  }

  Widget _buildReadingInfoHeader(ReaderSettings settings) {
    final fg = Color(settings.effectiveTextColor);
    final infoStyle = TextStyle(
        color: fg.withValues(alpha: 0.5),
        fontSize: 12,
        decoration: TextDecoration.none);
    if (!settings.showChapterTitle) return const SizedBox.shrink();
    final chapters = _cachedChapters;
    final effIdx = _settings.pageMode == ReaderPageMode.continuousScroll
        ? _visibleChapterIndex
        : _currentIndex;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.only(top: 6, bottom: 4),
      child: Text(
        chapters != null && effIdx < chapters.length
            ? (chapters[effIdx]['title'] as String? ?? '')
            : '',
        style: infoStyle,
        textAlign: TextAlign.center,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
    );
  }

  Widget _buildReadingInfoFooter(
      List<Map<String, dynamic>> chapters, ReaderSettings settings) {
    final fg = Color(settings.effectiveTextColor);
    final infoStyle = TextStyle(
        color: fg.withValues(alpha: 0.5),
        fontSize: 12,
        decoration: TextDecoration.none);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8, left: 16, right: 16),
      child: Row(
        children: [
          if (settings.showProgress)
            Text('${_visibleChapterIndex + 1}/${chapters.length}',
                style: infoStyle),
          const Spacer(),
          if (settings.showClock)
            ValueListenableBuilder<DateTime>(
              valueListenable: _nowNotifier,
              builder: (context, v, _) => Text(
                '${v.hour.toString().padLeft(2, '0')}:${v.minute.toString().padLeft(2, '0')}',
                style: infoStyle,
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildSearchBar() {
    final bgColor =
        Color(_settings.effectiveBackgroundColor).withValues(alpha: 0.95);
    final fgColor = Color(_settings.effectiveTextColor);
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          color: bgColor,
          child: SafeArea(
            bottom: false,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              child: Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _searchController,
                      autofocus: true,
                      spellCheckConfiguration:
                          const SpellCheckConfiguration.disabled(),
                      style: TextStyle(color: fgColor, fontSize: 14),
                      decoration: InputDecoration(
                        hintText: '搜索...',
                        hintStyle:
                            TextStyle(color: fgColor.withValues(alpha: 0.4)),
                        border: InputBorder.none,
                        isDense: true,
                        contentPadding: const EdgeInsets.symmetric(
                            horizontal: 8, vertical: 10),
                      ),
                      onChanged: (v) {
                        _performSearch(v);
                        setState(() {});
                      },
                    ),
                  ),
                  if (_searchMatches.isNotEmpty)
                    Text(
                      '${_currentSearchMatchIndex + 1}/${_searchMatches.length}',
                      style: TextStyle(
                          color: fgColor.withValues(alpha: 0.6), fontSize: 12),
                    ),
                  IconButton(
                    icon: Icon(Icons.chevron_left, color: fgColor, size: 20),
                    onPressed:
                        _searchMatches.isNotEmpty ? _goToPrevSearchMatch : null,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                  IconButton(
                    icon: Icon(Icons.chevron_right, color: fgColor, size: 20),
                    onPressed:
                        _searchMatches.isNotEmpty ? _goToNextSearchMatch : null,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                  IconButton(
                    icon: Icon(Icons.close, color: fgColor, size: 20),
                    onPressed: _closeSearch,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildTtsBar() {
    final bgColor =
        Color(_settings.effectiveBackgroundColor).withValues(alpha: 0.95);
    final fgColor = Color(_settings.effectiveTextColor);
    final paragraphs = _chapterContent
        .split(RegExp(r'\n+'))
        .where((p) => p.trim().isNotEmpty)
        .toList();
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          color: bgColor,
          child: SafeArea(
            bottom: false,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 4),
              child: Row(
                children: [
                  IconButton(
                    icon: Icon(Icons.skip_previous, color: fgColor, size: 20),
                    onPressed: _ttsPrevParagraph,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                  IconButton(
                    icon: Icon(
                        _isSpeaking && !_isPaused
                            ? Icons.pause
                            : Icons.play_arrow,
                        color: fgColor,
                        size: 20),
                    onPressed:
                        _isSpeaking && !_isPaused ? _pauseTts : _resumeTts,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                  IconButton(
                    icon: Icon(Icons.skip_next, color: fgColor, size: 20),
                    onPressed: _ttsNextParagraph,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                  Expanded(
                    child: Text(
                      '朗读中 ${_ttsParagraphIndex + 1}/${paragraphs.length}',
                      style: TextStyle(
                          color: fgColor.withValues(alpha: 0.6),
                          fontSize: 12,
                          decoration: TextDecoration.none),
                      textAlign: TextAlign.center,
                    ),
                  ),
                  GestureDetector(
                    onTap: _cycleTtsSpeed,
                    child: Container(
                      margin: const EdgeInsets.symmetric(horizontal: 2),
                      padding: const EdgeInsets.symmetric(
                          horizontal: 6, vertical: 2),
                      decoration: BoxDecoration(
                        border:
                            Border.all(color: fgColor.withValues(alpha: 0.3)),
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Text(
                        'x${_settings.ttsSpeed.toStringAsFixed(1)}',
                        style: TextStyle(color: fgColor, fontSize: 11),
                      ),
                    ),
                  ),
                  const SizedBox(width: 2),
                  IconButton(
                    icon: Icon(Icons.settings, color: fgColor, size: 18),
                    onPressed: _showReaderSettings,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 28, minHeight: 28),
                  ),
                  IconButton(
                    icon: Icon(Icons.stop, color: fgColor, size: 20),
                    onPressed: _stopTts,
                    padding: EdgeInsets.zero,
                    constraints:
                        const BoxConstraints(minWidth: 32, minHeight: 32),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildTopControls(BuildContext context,
      List<Map<String, dynamic>> chapters, bool hasPrev, bool hasNext) {
    final bgColor =
        Color(_settings.effectiveBackgroundColor).withValues(alpha: 0.85);
    final fgColor = Color(_settings.effectiveTextColor);
    final titleText = _bookName.isNotEmpty
        ? _bookName
        : (_currentIndex < chapters.length
            ? chapters[_currentIndex]['title'] as String? ?? '阅读'
            : '阅读');
    final titleStyle = TextStyle(
        color: fgColor, fontSize: 14, decoration: TextDecoration.none);
    final chapterName = _currentIndex < chapters.length
        ? (chapters[_currentIndex]['title'] as String? ?? '')
        : '';
    final sourceName = _sourceName.isNotEmpty ? _sourceName : '书源';
    final sourceUrl = _sourceUrl.isNotEmpty ? _sourceUrl : '';
    final chapterUrl = _chapterUrl.isNotEmpty ? _chapterUrl : '';
    final infoSmall = TextStyle(
        color: fgColor.withValues(alpha: 0.6),
        fontSize: 11,
        decoration: TextDecoration.none);
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          color: bgColor,
          child: SafeArea(
            bottom: false,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    IconButton(
                        icon: Icon(Icons.arrow_back, color: fgColor),
                        onPressed: () => context.pop()),
                    const SizedBox(width: 4),
                    Expanded(
                      child: SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        child: Text(titleText, style: titleStyle, maxLines: 1),
                      ),
                    ),
                    const SizedBox(width: 4),
                    IconButton(
                      icon: Icon(Icons.swap_horiz, color: fgColor),
                      onPressed: () {
                        _toggleControls();
                        _showChangeSourceDialog(context);
                      },
                      tooltip: '换源',
                      padding: EdgeInsets.zero,
                      constraints:
                          const BoxConstraints(minWidth: 36, minHeight: 36),
                    ),
                    IconButton(
                      icon: Icon(Icons.refresh, color: fgColor),
                      onPressed: _refreshChapter,
                      tooltip: '刷新',
                      padding: EdgeInsets.zero,
                      constraints:
                          const BoxConstraints(minWidth: 36, minHeight: 36),
                    ),
                    IconButton(
                      icon: Icon(Icons.download, color: fgColor),
                      onPressed: () {
                        _toggleControls();
                        _startDownload(context);
                      },
                      tooltip: '缓存',
                      padding: EdgeInsets.zero,
                      constraints:
                          const BoxConstraints(minWidth: 36, minHeight: 36),
                    ),
                    PopupMenuButton<String>(
                      icon: Icon(Icons.more_vert, color: fgColor),
                      padding: EdgeInsets.zero,
                      constraints:
                          const BoxConstraints(minWidth: 36, minHeight: 36),
                      onSelected: (v) {
                        if (v == 'bookmark') {
                          _toggleControls();
                          _toggleBookmark();
                        }
                      },
                      itemBuilder: (_) => [
                        PopupMenuItem(
                          value: 'bookmark',
                          child: Row(
                            children: [
                              Icon(
                                  _hasBookmarkForChapter
                                      ? Icons.bookmark
                                      : Icons.bookmark_border,
                                  color: fgColor,
                                  size: 20),
                              const SizedBox(width: 8),
                              Text('书签',
                                  style: TextStyle(
                                      color: fgColor,
                                      decoration: TextDecoration.none)),
                            ],
                          ),
                        ),
                        const PopupMenuItem(
                          value: 'more',
                          enabled: false,
                          child: Text('更多设置…',
                              style: TextStyle(color: Colors.grey)),
                        ),
                      ],
                    ),
                  ],
                ),
                Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 2),
                  child: Row(
                    children: [
                      Expanded(
                        child: GestureDetector(
                          onTap: () {
                            if (_sourceUrl.isNotEmpty ||
                                chapterUrl.isNotEmpty) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(content: Text('打开原网页 — 功能开发中')),
                              );
                            }
                          },
                          onLongPress: () {
                            showMenu<String>(
                              context: context,
                              position:
                                  RelativeRect.fromLTRB(100, 300, 100, 300),
                              items: [
                                const PopupMenuItem(
                                  value: 'login',
                                  child: Text('登录书源 — 开发中',
                                      style: TextStyle(color: Colors.grey)),
                                ),
                                const PopupMenuItem(
                                  value: 'edit',
                                  child: Text('编辑书源 — 开发中',
                                      style: TextStyle(color: Colors.grey)),
                                ),
                              ],
                            );
                          },
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                chapterName.isNotEmpty ? chapterName : '...',
                                style: infoSmall,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                              Text(
                                sourceUrl.isNotEmpty
                                    ? sourceUrl
                                    : chapterUrl.isNotEmpty
                                        ? chapterUrl
                                        : '',
                                style: infoSmall.copyWith(fontSize: 10),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ],
                          ),
                        ),
                      ),
                      GestureDetector(
                        onTap: () {
                          if (chapterUrl.isNotEmpty) {
                            ScaffoldMessenger.of(context).showSnackBar(
                              const SnackBar(content: Text('打开原网页 — 功能开发中')),
                            );
                          }
                        },
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 8, vertical: 2),
                          child: Text(
                            sourceName,
                            style: infoSmall.copyWith(
                                color: fgColor, fontWeight: FontWeight.w500),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildBottomControls(
      BuildContext context,
      List<Map<String, dynamic>> chapters,
      bool hasPrev,
      bool hasNext,
      ReaderSettings settings) {
    final bgColor =
        Color(settings.effectiveBackgroundColor).withValues(alpha: 0.85);
    final fgColor = Color(settings.effectiveTextColor);
    final maxChapter = (chapters.length - 1).toDouble();
    final maxChapterClamped = maxChapter < 0 ? 0.0 : maxChapter;
    final smallLabelStyle = TextStyle(
        color: fgColor.withValues(alpha: 0.7),
        fontSize: 10,
        decoration: TextDecoration.none);
    return Positioned(
      bottom: 0,
      left: 0,
      right: 0,
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          color: bgColor,
          child: SafeArea(
            top: false,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      _fabButton(Icons.search, '搜索', fgColor, smallLabelStyle,
                          _startSearch),
                      _fabButton(
                          _isAutoScrolling ? Icons.pause : Icons.play_arrow,
                          _isAutoScrolling ? '暂停' : '自动',
                          fgColor,
                          smallLabelStyle, () {
                        _toggleControls();
                        _toggleAutoScroll();
                      }),
                      _fabButton(
                          _settings.nightMode
                              ? Icons.wb_sunny
                              : Icons.nightlight_round,
                          _settings.nightMode ? '日间' : '夜间',
                          fgColor,
                          smallLabelStyle,
                          _toggleNightMode),
                      _fabButton(
                          Icons.find_replace, '替换', fgColor, smallLabelStyle,
                          () {
                        _toggleControls();
                        context.push('/replace-rules');
                      }),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Row(
                    children: [
                      IconButton(
                          icon: Icon(Icons.chevron_left, color: fgColor),
                          onPressed: hasPrev ? _goToPrevChapter : null,
                          padding: EdgeInsets.zero,
                          constraints: const BoxConstraints(
                              minWidth: 36, minHeight: 36)),
                      Expanded(
                        child: Slider(
                          value: (_sliderValue ?? _currentIndex.toDouble())
                              .clamp(0, maxChapterClamped),
                          min: 0,
                          max: maxChapterClamped,
                          divisions:
                              chapters.length > 1 ? chapters.length - 1 : 1,
                          activeColor: fgColor,
                          inactiveColor: fgColor.withValues(alpha: 0.25),
                          thumbColor: fgColor,
                          onChanged: (v) => setState(() => _sliderValue = v),
                          onChangeEnd: (v) {
                            final targetIndex =
                                v.round().clamp(0, chapters.length - 1);
                            setState(() {
                              _sliderValue = null;
                              _currentIndex = targetIndex;
                            });
                            _navigateToChapter(targetIndex, chapters);
                          },
                        ),
                      ),
                      SizedBox(
                        width: 44,
                        child: Text(
                          '${_currentIndex + 1}/${chapters.length}',
                          style: TextStyle(
                              color: fgColor,
                              fontSize: 11,
                              decoration: TextDecoration.none),
                          textAlign: TextAlign.center,
                        ),
                      ),
                      IconButton(
                          icon: Icon(Icons.chevron_right, color: fgColor),
                          onPressed: hasNext ? _goToNextChapter : null,
                          padding: EdgeInsets.zero,
                          constraints: const BoxConstraints(
                              minWidth: 36, minHeight: 36)),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      _toolbarButton(Icons.list, '目录', fgColor, smallLabelStyle,
                          _showDirectorySheet),
                      _toolbarButton(Icons.record_voice_over, '朗读', fgColor,
                          smallLabelStyle, () {
                        _toggleControls();
                        _startTts();
                      }),
                      _toolbarButton(Icons.tune, '界面', fgColor, smallLabelStyle,
                          _showReaderSettings),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _fabButton(IconData icon, String label, Color fgColor,
      TextStyle labelStyle, VoidCallback onTap) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, color: fgColor, size: 22),
            const SizedBox(height: 2),
            Text(label, style: labelStyle),
          ],
        ),
      ),
    );
  }

  Widget _toolbarButton(IconData icon, String label, Color fgColor,
      TextStyle labelStyle, VoidCallback onTap) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, color: fgColor, size: 20),
            const SizedBox(height: 1),
            Text(label, style: labelStyle),
          ],
        ),
      ),
    );
  }

  void _showReaderSettings() {
    _toggleControls();
    showModalBottomSheet(
      context: context,
      backgroundColor: Color(_settings.effectiveBackgroundColor),
      isScrollControlled: true,
      constraints: BoxConstraints(
        maxHeight: MediaQuery.of(context).size.height / 3,
      ),
      builder: (ctx) => _ReaderSettingsSheet(
        settings: _settings,
        onChanged: (s) {
          _setReaderSettings(s, persist: true);
        },
      ),
    );
  }
}

class _ReaderSettingsSheet extends StatefulWidget {
  final ReaderSettings settings;
  final ValueChanged<ReaderSettings> onChanged;
  const _ReaderSettingsSheet({required this.settings, required this.onChanged});
  @override
  State<_ReaderSettingsSheet> createState() => _ReaderSettingsSheetState();
}

class _ReaderSettingsSheetState extends State<_ReaderSettingsSheet> {
  late ReaderSettings _s;

  @override
  void initState() {
    super.initState();
    _s = widget.settings;
  }

  void _update(ReaderSettings s) {
    setState(() => _s = s);
    widget.onChanged(s);
  }

  String _pageModeLabel(ReaderPageMode m) {
    switch (m) {
      case ReaderPageMode.continuousScroll:
        return '连续滚动';
      case ReaderPageMode.tapChapter:
        return '点击翻章';
      case ReaderPageMode.page:
        return '分页';
    }
  }

  Future<void> _pickBackgroundImage() async {
    try {
      final result = await FilePicker.pickFiles(type: FileType.image);
      if (result == null || result.files.isEmpty) return;
      final sourcePath = result.files.single.path;
      if (sourcePath == null) return;
      final dir = Platform.isAndroid
          ? (await getApplicationDocumentsDirectory()).path
          : (await getApplicationSupportDirectory()).path;
      final bgDir = Directory('$dir/reader_backgrounds');
      if (!await bgDir.exists()) await bgDir.create(recursive: true);
      final filename = 'bg_${DateTime.now().millisecondsSinceEpoch}.jpg';
      final destPath = '${bgDir.path}/$filename';
      await File(sourcePath).copy(destPath);
      _update(_s.copyWith(backgroundImagePath: destPath));
    } catch (_) {}
  }

  void _clearBackgroundImage() {
    _update(_s.copyWith(backgroundImagePath: null));
  }

  @override
  Widget build(BuildContext ctx) {
    final fg = Color(_s.effectiveTextColor);
    final label = TextStyle(color: fg, fontSize: 14);
    final chipStyle = const TextStyle(fontSize: 12);
    return Container(
      padding: const EdgeInsets.fromLTRB(20, 12, 20, 20),
      child: SingleChildScrollView(
        child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('字号: ${_s.fontSize.round()}', style: label),
              Slider(
                  value: _s.fontSize,
                  min: 12,
                  max: 30,
                  divisions: 18,
                  onChanged: (v) => _update(_s.copyWith(fontSize: v))),
              const SizedBox(height: 12),
              Text('字重', style: label),
              SegmentedButton<int>(
                segments: const [
                  ButtonSegment(
                      value: 0,
                      label: Text('细', style: TextStyle(fontSize: 12))),
                  ButtonSegment(
                      value: 1,
                      label: Text('正常', style: TextStyle(fontSize: 12))),
                  ButtonSegment(
                      value: 2,
                      label: Text('粗', style: TextStyle(fontSize: 12))),
                ],
                selected: {_s.fontWeightIndex},
                onSelectionChanged: (v) =>
                    _update(_s.copyWith(fontWeightIndex: v.first)),
              ),
              const SizedBox(height: 12),
              Text('字距: ${_s.letterSpacing.toStringAsFixed(1)}', style: label),
              Slider(
                  value: _s.letterSpacing,
                  min: -1,
                  max: 5,
                  divisions: 60,
                  onChanged: (v) => _update(_s.copyWith(letterSpacing: v))),
              Text('行距: ${_s.lineHeight.toStringAsFixed(1)}', style: label),
              Slider(
                  value: _s.lineHeight,
                  min: 1.0,
                  max: 3.5,
                  divisions: 25,
                  onChanged: (v) => _update(_s.copyWith(lineHeight: v))),
              Text('段距: ${_s.paragraphSpacing.round()}', style: label),
              Slider(
                  value: _s.paragraphSpacing,
                  min: 0,
                  max: 30,
                  divisions: 30,
                  onChanged: (v) => _update(_s.copyWith(paragraphSpacing: v))),
              Text('左右边距: ${_s.horizontalPadding.round()}', style: label),
              Slider(
                  value: _s.horizontalPadding,
                  min: 0,
                  max: 60,
                  divisions: 30,
                  onChanged: (v) => _update(_s.copyWith(horizontalPadding: v))),
              Text('上下边距: ${_s.verticalPadding.round()}', style: label),
              Slider(
                  value: _s.verticalPadding,
                  min: 0,
                  max: 60,
                  divisions: 30,
                  onChanged: (v) => _update(_s.copyWith(verticalPadding: v))),
              const SizedBox(height: 12),
              Text('段首缩进', style: label),
              const SizedBox(height: 4),
              Row(children: [
                ChoiceChip(
                    label: Text('无', style: chipStyle),
                    selected: _s.paragraphIndent.isEmpty,
                    onSelected: (_) =>
                        _update(_s.copyWith(paragraphIndent: ''))),
                const SizedBox(width: 8),
                ChoiceChip(
                    label: Text('2全角', style: chipStyle),
                    selected: _s.paragraphIndent == '\u3000\u3000',
                    onSelected: (_) =>
                        _update(_s.copyWith(paragraphIndent: '\u3000\u3000'))),
                const SizedBox(width: 8),
                ChoiceChip(
                    label: Text('4半角', style: chipStyle),
                    selected: _s.paragraphIndent == '    ',
                    onSelected: (_) =>
                        _update(_s.copyWith(paragraphIndent: '    '))),
              ]),
              const SizedBox(height: 12),
              Text('阅读信息', style: label),
              SwitchListTile(
                title: Text('显示阅读信息', style: label),
                value: _s.showReadingInfo,
                dense: true,
                onChanged: (v) => _update(_s.copyWith(showReadingInfo: v)),
              ),
              SwitchListTile(
                title: Text('章节标题', style: label),
                value: _s.showChapterTitle,
                dense: true,
                onChanged: (v) => _update(_s.copyWith(showChapterTitle: v)),
              ),
              SwitchListTile(
                title: Text('时间', style: label),
                value: _s.showClock,
                dense: true,
                onChanged: (v) => _update(_s.copyWith(showClock: v)),
              ),
              SwitchListTile(
                title: Text('进度', style: label),
                value: _s.showProgress,
                dense: true,
                onChanged: (v) => _update(_s.copyWith(showProgress: v)),
              ),
              const SizedBox(height: 12),
              Text('背景图片', style: label),
              const SizedBox(height: 4),
              Row(children: [
                ElevatedButton.icon(
                  onPressed: _pickBackgroundImage,
                  icon: const Icon(Icons.image, size: 18),
                  label: const Text('选择'),
                ),
                const SizedBox(width: 8),
                if (_s.backgroundImagePath != null)
                  OutlinedButton(
                    onPressed: _clearBackgroundImage,
                    child: const Text('清除'),
                  ),
              ]),
              const SizedBox(height: 12),
              Text('背景色', style: label),
              const SizedBox(height: 4),
              Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: ReaderSettings.presetColors
                      .map((c) => GestureDetector(
                            onTap: () => _update(_s.copyWith(
                                backgroundColor: c.toARGB32(),
                                backgroundImagePath: null)),
                            child: Container(
                              width: 36,
                              height: 36,
                              decoration: BoxDecoration(
                                color: c,
                                borderRadius: BorderRadius.circular(8),
                                border: Border.all(
                                    color: _s.backgroundColor == c.toARGB32() &&
                                            _s.backgroundImagePath == null
                                        ? Theme.of(ctx).primaryColor
                                        : Colors.grey.shade400,
                                    width: 2),
                              ),
                            ),
                          ))
                      .toList()),
              const SizedBox(height: 12),
              Text('翻页方式', style: label),
              RadioGroup<ReaderPageMode>(
                groupValue: _s.pageMode,
                onChanged: (v) {
                  if (v != null) {
                    _update(_s.copyWith(pageMode: v));
                  }
                },
                child: Column(
                  children: ReaderPageMode.values
                      .map((m) => RadioListTile<ReaderPageMode>(
                            title: Text(_pageModeLabel(m), style: label),
                            value: m,
                            activeColor: Theme.of(ctx).primaryColor,
                            dense: true,
                            contentPadding: EdgeInsets.zero,
                            toggleable: m != ReaderPageMode.page,
                          ))
                      .toList(),
                ),
              ),
              if (_s.pageMode == ReaderPageMode.page)
                Padding(
                    padding: const EdgeInsets.only(left: 32, top: 4),
                    child: Text('翻页动画',
                        style: TextStyle(
                            color: Theme.of(ctx).primaryColor,
                            fontSize: 13,
                            fontWeight: FontWeight.w600))),
              if (_s.pageMode == ReaderPageMode.page)
                SegmentedButton<int>(
                  segments: const [
                    ButtonSegment(
                        value: 0,
                        label: Text('无', style: TextStyle(fontSize: 12))),
                    ButtonSegment(
                        value: 2,
                        label: Text('覆盖', style: TextStyle(fontSize: 12))),
                    ButtonSegment(
                        value: 3,
                        label: Text('平移', style: TextStyle(fontSize: 12))),
                  ],
                  selected: {_s.pageAnim},
                  onSelectionChanged: (v) =>
                      _update(_s.copyWith(pageAnim: v.first)),
                ),
            ]),
      ),
    );
  }
}
