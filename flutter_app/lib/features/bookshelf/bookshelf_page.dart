import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:cached_network_image/cached_network_image.dart';
import 'dart:io';

import 'package:go_router/go_router.dart';

import '../../core/providers.dart';
import '../../src/rust/api.dart' as rust_api;

class BookshelfPage extends ConsumerStatefulWidget {
  const BookshelfPage({super.key});

  @override
  ConsumerState<BookshelfPage> createState() => _BookshelfPageState();
}

class _BookshelfPageState extends ConsumerState<BookshelfPage> {
  bool _isGridView = false;

  @override
  void initState() {
    super.initState();
    loadBookshelfGridViewFromDisk().then((value) {
      if (mounted) setState(() => _isGridView = value);
    });
  }

  @override
  Widget build(BuildContext context) {
    final booksAsync = ref.watch(allBooksProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('书架'),
        actions: [
          IconButton(
            icon: Icon(_isGridView ? Icons.view_list : Icons.grid_view),
            tooltip: _isGridView ? '列表视图' : '网格视图',
            onPressed: () {
              setState(() => _isGridView = !_isGridView);
              saveBookshelfGridViewToDisk(_isGridView);
            },
          ),
        ],
      ),
      body: booksAsync.when(
        data: (books) => _buildBookList(context, books),
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败: $e')),
      ),
    );
  }

  Widget _buildBookList(BuildContext context, List<Map<String, dynamic>> books) {
    if (books.isEmpty) {
      return const Center(child: Text('书架为空，去搜索添加书籍吧'));
    }
    if (_isGridView) {
      return _buildGridView(context, books);
    }
    return _buildListView(context, books);
  }

  Widget _buildListView(BuildContext context, List<Map<String, dynamic>> books) {
    return ListView.builder(
      padding: const EdgeInsets.all(8),
      itemExtent: 72,
      itemCount: books.length,
      itemBuilder: (context, index) {
        final book = books[index];
        return GestureDetector(
          onLongPress: () => _deleteBook(context, book),
          child: Card(
            child: ListTile(
              leading: _buildCover(book),
              title: Text(book['name'] ?? '未知书名'),
              subtitle: Text(book['author'] ?? '未知作者'),
              trailing: Text('${book['chapter_count'] ?? 0}章'),
              onTap: () => context.push(
                Uri(path: '/reader', queryParameters: {
                  'bookId': book['id'] as String? ?? '',
                }).toString(),
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildGridView(BuildContext context, List<Map<String, dynamic>> books) {
    return GridView.builder(
      padding: const EdgeInsets.all(8),
      gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: 3,
        childAspectRatio: 0.65,
        crossAxisSpacing: 8,
        mainAxisSpacing: 8,
      ),
      itemCount: books.length,
      itemBuilder: (context, index) {
        final book = books[index];
        return Card(
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: () => context.push(
              Uri(path: '/reader', queryParameters: {
                'bookId': book['id'] as String? ?? '',
              }).toString(),
            ),
            onLongPress: () => _deleteBook(context, book),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: _buildCover(book),
                ),
                Padding(
                  padding: const EdgeInsets.all(6),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        book['name'] ?? '未知书名',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                              fontWeight: FontWeight.w600,
                            ),
                      ),
                      Text(
                        book['author'] ?? '',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.labelSmall,
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _buildCover(Map<String, dynamic> book) {
    final localPath = book['custom_cover_path'] as String?;
    if (localPath != null && localPath.isNotEmpty) {
      return Image.file(
        File(localPath),
        fit: BoxFit.cover,
        cacheWidth: 100,
        cacheHeight: 150,
        errorBuilder: (_, __, ___) => _buildNetworkCover(book['cover_url'] as String?),
      );
    }
    return _buildNetworkCover(book['cover_url'] as String?);
  }

  Widget _buildNetworkCover(String? coverUrl) {
    if (coverUrl == null || coverUrl.isEmpty) {
      return Container(
        color: Colors.grey.shade200,
        child: Icon(Icons.book, size: 40, color: Colors.grey.shade500),
      );
    }
    return CachedNetworkImage(
      imageUrl: coverUrl,
      fit: BoxFit.cover,
      memCacheWidth: 100,
      memCacheHeight: 150,
      placeholder: (context, url) => const Center(
        child: CircularProgressIndicator(strokeWidth: 2),
      ),
      errorWidget: (context, url, error) => Container(
        color: Colors.grey.shade200,
        child: Icon(Icons.broken_image, size: 32, color: Colors.grey.shade500),
      ),
    );
  }

  Future<void> _deleteBook(BuildContext context, Map<String, dynamic> book) async {
    final name = book['name'] as String? ?? '未知';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('确认删除'),
        content: Text('确定要把《$name》从书架中删除吗？\n\n该操作会同时删除章节缓存和阅读进度。'),
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
      final bookId = book['id'] as String?;
      if (bookId == null || bookId.isEmpty) return;
      await rust_api.deleteBook(dbPath: dbPath, id: bookId);
      ref.invalidate(allBooksProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('已删除《$name》')),
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
