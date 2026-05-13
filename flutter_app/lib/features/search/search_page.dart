import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;
import 'package:dio/dio.dart';

class SearchPage extends ConsumerStatefulWidget {
  const SearchPage({super.key});

  @override
  ConsumerState<SearchPage> createState() => _SearchPageState();
}

class _SearchPageState extends ConsumerState<SearchPage> {
  final _searchCtrl = TextEditingController();
  final _results = ValueNotifier<List<Map<String, dynamic>>>([]);
  bool _loading = false;
  bool _onlineMode = false;
  List<String> _searchHistory = [];

  @override
  void initState() {
    super.initState();
    _loadHistory();
  }

  Future<void> _loadHistory() async {
    final history = await loadSearchHistoryFromDisk();
    if (mounted) setState(() => _searchHistory = history);
  }

  Future<void> _addToHistory(String keyword) async {
    _searchHistory.remove(keyword);
    _searchHistory.insert(0, keyword);
    if (_searchHistory.length > 20) {
      _searchHistory = _searchHistory.sublist(0, 20);
    }
    await saveSearchHistoryToDisk(_searchHistory);
    if (mounted) setState(() {});
  }

  Future<void> _clearHistory() async {
    _searchHistory = [];
    await saveSearchHistoryToDisk([]);
    if (mounted) setState(() {});
  }

  Future<void> _saveResultToBookshelf(Map<String, dynamic> result) async {
    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final bookUrl = result['book_url'] as String?;
    final sourceId = result['source_id'] as String?;

    // Generate stable book ID. Online search results from the parser carry
    // random UUIDs that change every search; ignore result['id'] and derive
    // a stable ID from source_id + book_url + name + author.
    final isOnlineResult = bookUrl != null && bookUrl.isNotEmpty;
    final String bookId;
    if (isOnlineResult) {
      final stableSource = [sourceId, bookUrl, result['name'], result['author']]
          .where((o) => o != null && o.toString().isNotEmpty)
          .join('|');
      final hashInput = stableSource.isNotEmpty
          ? stableSource
          : '${result['name'] ?? 'unknown'}|$now';
      bookId = base64Url.encode(sha256.convert(utf8.encode(hashInput)).bytes).replaceAll('=', '');
    } else {
      final rawId = result['id'] as String?;
      if (rawId != null && rawId.trim().isNotEmpty) {
        bookId = rawId.trim();
      } else {
        final fallback = [sourceId, result['name'], result['author']]
            .where((o) => o != null && o.toString().isNotEmpty)
            .join('|');
        final hashInput = fallback.isNotEmpty ? fallback : 'unknown|$now';
        bookId = base64Url.encode(sha256.convert(utf8.encode(hashInput)).bytes).replaceAll('=', '');
      }
    }
    final bookData = <String, dynamic>{
      'id': bookId,
      'source_id': sourceId ?? '',
      'source_name': result['source_name'],
      'name': result['name'] ?? '未知',
      'author': result['author'],
      'cover_url': result['cover_url'],
      'chapter_count': result['chapter_count'] ?? 0,
      'latest_chapter_title': result['latest_chapter_title'],
      'intro': result['intro'],
      'kind': result['kind'],
      'book_url': result['book_url'],
      'last_check_time': result['last_check_time'],
      'last_check_count': result['last_check_count'] ?? 0,
      'total_word_count': result['total_word_count'] ?? 0,
      'can_update': result['can_update'] ?? true,
      'order_time': result['order_time'] ?? now,
      'latest_chapter_time': result['latest_chapter_time'],
      'custom_cover_path': result['custom_cover_path'],
      'custom_info_json': result['custom_info_json'],
      'created_at': result['created_at'] ?? now,
      'updated_at': now,
    };
    var savedBook = false;
    var savedChapterCount = 0;
    try {
      final dbPath = await ref.read(dbPathProvider.future);
      if (!mounted) return;
      await rust_api.saveBook(dbPath: dbPath, bookJson: jsonEncode(bookData));
      if (!mounted) return;
      savedBook = true;
      final coverUrl = result['cover_url'] as String?;
      if (coverUrl != null && coverUrl.isNotEmpty) {
        unawaited(_downloadAndCacheCover(coverUrl, dbPath).then((localPath) async {
          if (localPath != null) {
            bookData['custom_cover_path'] = localPath;
            try {
              await rust_api.saveBook(dbPath: dbPath, bookJson: jsonEncode(bookData));
            } catch (_) {}
          }
        }));
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('添加失败: $e')),
        );
      }
      return;
    }

    if (bookUrl != null && bookUrl.isNotEmpty && sourceId != null && sourceId.isNotEmpty) {
      try {
        final dbPath = await ref.read(dbPathProvider.future);
        if (!mounted) return;
        final sourceJson = await rust_api.getSourceForDownload(
          dbPath: dbPath,
          sourceId: result['source_id'] as String,
        );
        if (!mounted) return;
        final chaptersJson = await rust_api.getChapterListOnline(
          sourceJson: sourceJson,
          bookUrl: bookUrl,
        );
        if (!mounted) return;
        final List<dynamic> chapters = jsonDecode(chaptersJson);
        for (var i = 0; i < chapters.length; i++) {
          final ch = chapters[i] as Map<String, dynamic>;
          final chapterKey = '${bookData['id']}|$i|${ch['url'] ?? ''}';
          final chapterId = base64Url
              .encode(sha256.convert(utf8.encode(chapterKey)).bytes)
              .replaceAll('=', '');
          final chapterData = {
            'id': chapterId,
            'book_id': bookData['id'],
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
          savedChapterCount++;
        }
        if (savedChapterCount > 0) {
          bookData['chapter_count'] = savedChapterCount;
          if (chapters.isNotEmpty) {
            bookData['latest_chapter_title'] = (chapters.last as Map<String, dynamic>)['title'];
          }
          await rust_api.saveBook(dbPath: dbPath, bookJson: jsonEncode(bookData));
        }
      } catch (e) {
        debugPrint('拉取章节失败: $e');
      }
    }

    if (!mounted || !savedBook) return;
    ref.invalidate(allBooksProvider);
    if (savedChapterCount > 0) {
      ref.invalidate(bookChaptersProvider(bookId));
    }
    final String snackMsg;
    if (savedChapterCount > 0) {
      snackMsg = '已添加: ${bookData['name']} ($savedChapterCount章)';
    } else if (bookUrl != null && bookUrl.isNotEmpty) {
      if (sourceId != null && sourceId.isNotEmpty) {
        snackMsg = '已添加: ${bookData['name']}（章节加载失败或目录为空）';
      } else {
        snackMsg = '已添加: ${bookData['name']}（无有效书源，未能加载章节）';
      }
    } else {
      snackMsg = '已添加: ${bookData['name']}';
    }
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(snackMsg)));
  }

  Future<String?> _downloadAndCacheCover(String coverUrl, String dbPath) async {
    try {
      final dir = await getApplicationDocumentsDirectory();
      final coversDir = Directory('${dir.path}/covers');
      if (!coversDir.existsSync()) {
        coversDir.createSync(recursive: true);
      }
      final hashBytes = md5.convert(utf8.encode(coverUrl)).bytes;
      final hash = hashBytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
      final ext = coverUrl.split('.').last.split('?').first;
      final safeExt = ext.length <= 5 ? ext : 'jpg';
      final filePath = '${coversDir.path}/$hash.$safeExt';
      if (File(filePath).existsSync()) {
        return filePath;
      }
      await Dio().download(coverUrl, filePath);
      return filePath;
    } catch (e) {
      debugPrint('封面下载失败: $e');
      return null;
    }
  }

  @override
  void dispose() {
    _searchCtrl.dispose();
    _results.dispose();
    super.dispose();
  }

  Future<void> _doSearch() async {
    final keyword = _searchCtrl.text.trim();
    if (keyword.isEmpty) return;
    setState(() => _loading = true);
    try {
      if (_onlineMode) {
        final dbPath = await ref.read(dbPathProvider.future);
        final sourcesJson = await rust_api.getEnabledSources(dbPath: dbPath);
        final List<dynamic> sources = jsonDecode(sourcesJson);
        if (sources.isEmpty) {
          if (!mounted) return;
          _results.value = [];
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('没有启用的书源，请先在书源管理中启用书源')),
          );
          return;
        }
        final futures = <Future<List<Map<String, dynamic>>>>[];
        for (final source in sources) {
          if (source == null) continue;
          futures.add(
            _searchWithSource(dbPath, source, keyword)
              .timeout(const Duration(seconds: 15), onTimeout: () {
                debugPrint('书源 ${source['name']} 搜索超时');
                return <Map<String, dynamic>>[];
              }),
          );
        }
        final allResults = await Future.wait(futures);
        if (!mounted) return;
        final flatResults = allResults.expand((r) => r).toList();
        final seen = <String>{};
        final deduped = <Map<String, dynamic>>[];
        for (final r in flatResults) {
          final key = '${r['name']}_${r['author']}';
          if (seen.add(key)) {
            deduped.add(r);
          }
        }
        _results.value = deduped;
      } else {
        await ref.read(dbInitializedProvider.future);
        if (!mounted) return;
        final dbPath = await ref.read(dbPathProvider.future);
        if (!mounted) return;
        final offlineJson = await rust_api.searchBooksOffline(dbPath: dbPath, keyword: keyword);
        final List<dynamic> offlineList = jsonDecode(offlineJson);
        if (!mounted) return;
        _results.value = offlineList.cast<Map<String, dynamic>>();
      }
      _addToHistory(keyword);
    } catch (e) {
      if (!mounted) return;
      _results.value = [];
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('搜索失败: $e')),
      );
    } finally {
      if (!mounted) return;
      setState(() => _loading = false);
    }
  }

  Future<List<Map<String, dynamic>>> _searchWithSource(
    String dbPath, dynamic source, String keyword) async {
    try {
      final onlineJson = await rust_api.searchWithSourceFromDb(
        dbPath: dbPath,
        sourceId: source['id'] as String,
        keyword: keyword,
      );
      final List<dynamic> sourceResults = jsonDecode(onlineJson);
      return sourceResults.map<Map<String, dynamic>>((r) {
        final m = Map<String, dynamic>.from(r as Map);
        m['source_name'] = source['name'] ?? '未知书源';
        m['source_id'] = source['id'];
        return m;
      }).toList();
    } catch (e) {
      debugPrint('书源 ${source['name']} 搜索失败: $e');
      return <Map<String, dynamic>>[];
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('搜索')),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: TextField(
              controller: _searchCtrl,
              decoration: InputDecoration(
                hintText: '输入书名或作者',
                prefixIcon: const Icon(Icons.search),
                suffixIcon: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _buildOnlineToggle(),
                    if (_loading)
                      const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 2))
                    else
                      IconButton(icon: const Icon(Icons.send), onPressed: _doSearch),
                  ],
                ),
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(12)),
              ),
              onSubmitted: (_) => _doSearch(),
            ),
          ),
          Expanded(
            child: ValueListenableBuilder<List<Map<String, dynamic>>>(
              valueListenable: _results,
              builder: (context, results, _) {
                if (results.isEmpty && !_loading) {
                  if (_loading) return const SizedBox.shrink();
                  return _buildSearchHistory();
                }
                return ListView.builder(
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  itemCount: results.length,
                  itemBuilder: (context, index) {
                    final book = results[index];
                    final coverUrl = book['cover_url'] as String?;
                    final name = book['name'] as String? ?? '未知';
                    final author = book['author'] as String? ?? '';
                    final kind = book['kind'] as String?;
                    final intro = book['intro'] as String?;
                    final sourceName = book['source_name'] as String?;
                    final latestChapter = book['last_chapter'] as String? ??
                        book['latest_chapter_title'] as String?;

                    final subtitleParts = <String>[
                      if (author.isNotEmpty) author,
                      if (kind != null && kind.isNotEmpty) kind,
                      if (latestChapter != null && latestChapter.isNotEmpty) latestChapter,
                      if (sourceName != null && sourceName.isNotEmpty) '来源: $sourceName',
                    ];

                    return Card(
                      margin: const EdgeInsets.symmetric(vertical: 4),
                      child: InkWell(
                        borderRadius: BorderRadius.circular(12),
                        onTap: () => _showBookDetail(context, book),
                        child: Padding(
                          padding: const EdgeInsets.all(8),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              _buildCover(coverUrl),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      name,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: Theme.of(context).textTheme.titleSmall?.copyWith(
                                            fontWeight: FontWeight.w600,
                                          ),
                                    ),
                                    if (subtitleParts.isNotEmpty) ...[
                                      const SizedBox(height: 2),
                                      Text(
                                        subtitleParts.join(' · '),
                                        maxLines: 2,
                                        overflow: TextOverflow.ellipsis,
                                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                                              color: Colors.grey[600],
                                            ),
                                      ),
                                    ],
                                    if (intro != null && intro.isNotEmpty) ...[
                                      const SizedBox(height: 4),
                                      Text(
                                        intro,
                                        maxLines: 2,
                                        overflow: TextOverflow.ellipsis,
                                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                                              color: Colors.grey[500],
                                              fontSize: 11,
                                            ),
                                      ),
                                    ],
                                  ],
                                ),
                              ),
                              const SizedBox(width: 4),
                              IconButton(
                                icon: const Icon(Icons.add_circle_outline),
                                color: Theme.of(context).colorScheme.primary,
                                tooltip: '加入书架',
                                onPressed: () => _saveResultToBookshelf(book),
                              ),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildOnlineToggle() {
    return GestureDetector(
      onTap: () => setState(() => _onlineMode = !_onlineMode),
      child: Tooltip(
        message: _onlineMode ? '当前：在线搜索' : '当前：离线搜索',
        child: Container(
          width: 32,
          height: 32,
          alignment: Alignment.center,
          child: Icon(
            _onlineMode ? Icons.cloud : Icons.phone_android,
            size: 18,
            color: _onlineMode ? Colors.blue : Colors.grey,
          ),
        ),
      ),
    );
  }

  Widget _buildSearchHistory() {
    if (_searchHistory.isEmpty) {
      return const Center(child: Text('输入关键词搜索书籍'));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
          child: Row(
            children: [
              Text(
                '最近搜索',
                style: Theme.of(context).textTheme.labelMedium?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
              ),
              const Spacer(),
              GestureDetector(
                onTap: _clearHistory,
                child: Text(
                  '清除',
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: Theme.of(context).colorScheme.primary,
                      ),
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            itemCount: _searchHistory.length,
            itemBuilder: (context, index) {
              final term = _searchHistory[index];
              return ListTile(
                dense: true,
                leading: const Icon(Icons.history, size: 20),
                title: Text(term),
                onTap: () {
                  _searchCtrl.text = term;
                  _doSearch();
                },
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _buildCover(String? coverUrl) {
    const double w = 56;
    const double h = 78;
    if (coverUrl == null || coverUrl.isEmpty) {
      return Container(
        width: w,
        height: h,
        decoration: BoxDecoration(
          color: Colors.grey.shade200,
          borderRadius: BorderRadius.circular(4),
        ),
        child: Icon(Icons.book, size: 28, color: Colors.grey.shade400),
      );
    }
    return ClipRRect(
      borderRadius: BorderRadius.circular(4),
      child: CachedNetworkImage(
        imageUrl: coverUrl,
        width: w,
        height: h,
        fit: BoxFit.cover,
        placeholder: (_, __) => Container(
          width: w,
          height: h,
          color: Colors.grey.shade100,
          child: const Center(
            child: SizedBox(
              width: 16,
              height: 16,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
          ),
        ),
        errorWidget: (_, __, ___) => Container(
          width: w,
          height: h,
          color: Colors.grey.shade200,
          child: Icon(Icons.broken_image, size: 24, color: Colors.grey.shade400),
        ),
      ),
    );
  }

  void _showBookDetail(BuildContext context, Map<String, dynamic> book) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (ctx) => DraggableScrollableSheet(
        initialChildSize: 0.5,
        minChildSize: 0.3,
        maxChildSize: 0.85,
        expand: false,
        builder: (ctx, scrollController) => SingleChildScrollView(
          controller: scrollController,
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Center(
                  child: Container(
                    width: 40,
                    height: 4,
                    decoration: BoxDecoration(
                      color: Colors.grey.shade300,
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                ),
                const SizedBox(height: 16),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    ClipRRect(
                      borderRadius: BorderRadius.circular(6),
                      child: SizedBox(
                        width: 80,
                        height: 108,
                        child: _buildCover(book['cover_url'] as String?),
                      ),
                    ),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            book['name'] as String? ?? '未知',
                            style: Theme.of(ctx).textTheme.titleMedium?.copyWith(
                                  fontWeight: FontWeight.bold,
                                ),
                          ),
                          const SizedBox(height: 4),
                          if (book['author'] != null)
                            Text('作者: ${book['author']}',
                                style: Theme.of(ctx).textTheme.bodySmall),
                          if (book['kind'] != null)
                            Text('分类: ${book['kind']}',
                                style: Theme.of(ctx).textTheme.bodySmall),
                          if (book['last_chapter'] != null)
                            Text('最新章节: ${book['last_chapter']}',
                                style: Theme.of(ctx).textTheme.bodySmall),
                          if (book['source_name'] != null)
                            Text('来源: ${book['source_name']}',
                                style: Theme.of(ctx).textTheme.bodySmall?.copyWith(
                                      color: Colors.grey,
                                    )),
                        ],
                      ),
                    ),
                  ],
                ),
                if (book['intro'] != null) ...[
                  const SizedBox(height: 14),
                  const Divider(),
                  const SizedBox(height: 8),
                  Text('简介', style: Theme.of(ctx).textTheme.labelLarge),
                  const SizedBox(height: 6),
                  Text(
                    book['intro'] as String? ?? '',
                    style: Theme.of(ctx).textTheme.bodySmall?.copyWith(
                          color: Colors.grey[700],
                          height: 1.5,
                        ),
                  ),
                ],
                const SizedBox(height: 16),
                SizedBox(
                  width: double.infinity,
                  child: FilledButton.icon(
                    onPressed: () {
                      Navigator.pop(ctx);
                      _saveResultToBookshelf(book);
                    },
                    icon: const Icon(Icons.add),
                    label: const Text('加入书架'),
                  ),
                ),
                SizedBox(height: MediaQuery.of(ctx).padding.bottom + 60),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
