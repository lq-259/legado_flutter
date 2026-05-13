import 'package:flutter/material.dart';
import '../../../core/providers.dart';
import 'text_page.dart';
import 'page_measure.dart';

enum PageDirection { none, next, prev }

class PageViewController extends ChangeNotifier {
  final List<_ChapterData> _chapters = [];
  final Map<int, List<TextPage>> _measuredPages = {};

  int _currentChapterIndex = 0;
  int _currentPageIndex = 0;
  ReaderSettings _settings;
  Size _pageSize = Size.zero;

  bool _isMeasuring = false;
  bool _disposed = false;

  PageViewController({
    required ReaderSettings settings,
    int initialChapterIndex = 0,
    int initialPageIndex = 0,
  })  : _settings = settings,
        _currentChapterIndex = initialChapterIndex,
        _currentPageIndex = initialPageIndex;

  int get currentChapterIndex => _currentChapterIndex;
  int get currentPageIndex => _currentPageIndex;
  ReaderSettings get settings => _settings;

  TextPage? get currentPage {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages == null || _currentPageIndex < 0 || _currentPageIndex >= pages.length) {
      return null;
    }
    return pages[_currentPageIndex];
  }

  TextPage? get nextPage {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages == null) return null;
    final nextIdx = _currentPageIndex + 1;
    if (nextIdx < pages.length) return pages[nextIdx];
    if (_currentChapterIndex + 1 < _chapters.length) {
      final nextChapterPages = _measuredPages[_currentChapterIndex + 1];
      if (nextChapterPages != null && nextChapterPages.isNotEmpty) {
        return nextChapterPages.first;
      }
    }
    return null;
  }

  TextPage? get prevPage {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages == null) return null;
    final prevIdx = _currentPageIndex - 1;
    if (prevIdx >= 0) return pages[prevIdx];
    if (_currentChapterIndex > 0) {
      final prevChapterPages = _measuredPages[_currentChapterIndex - 1];
      if (prevChapterPages != null && prevChapterPages.isNotEmpty) {
        return prevChapterPages.last;
      }
    }
    return null;
  }

  bool get hasNext {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages == null) return false;
    if (_currentPageIndex + 1 < pages.length) return true;
    return _currentChapterIndex + 1 < _chapters.length;
  }

  bool get hasPrev {
    if (_currentPageIndex > 0) return true;
    return _currentChapterIndex > 0;
  }

  int get totalPagesInChapter {
    return _measuredPages[_currentChapterIndex]?.length ?? 0;
  }

  void updatePageSize(Size size) {
    _pageSize = size;
    _measureCurrentChapterIfNeeded();
  }

  void updateSettings(ReaderSettings settings) {
    if (_settings.fontSize == settings.fontSize &&
        _settings.fontWeightIndex == settings.fontWeightIndex &&
        _settings.letterSpacing == settings.letterSpacing &&
        _settings.lineHeight == settings.lineHeight &&
        _settings.paragraphSpacing == settings.paragraphSpacing &&
        _settings.horizontalPadding == settings.horizontalPadding &&
        _settings.verticalPadding == settings.verticalPadding &&
        _settings.paragraphIndent == settings.paragraphIndent) {
      _settings = settings;
      return;
    }
    _settings = settings;
    _measuredPages.clear();
    _measureCurrentChapterIfNeeded();
  }

  void loadChapter(int index, String title, String content, {bool jumpToLast = false}) {
    final paragraphs = content
        .split(RegExp(r'\n+'))
        .where((p) => p.trim().isNotEmpty)
        .toList();
    final ch = _ChapterData(index, title, paragraphs);
    _chapters.clear();
    _chapters.add(ch);
    _currentChapterIndex = 0;
    _currentPageIndex = 0;
    _measuredPages.clear();
    _measureCurrentChapterIfNeeded(jumpToLast: jumpToLast);
  }

  void clearChapters() {
    _chapters.clear();
    _measuredPages.clear();
    _currentChapterIndex = 0;
    _currentPageIndex = 0;
  }

  void jumpToPage(int pageIndex) {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages == null || pages.isEmpty) return;
    _currentPageIndex = pageIndex.clamp(0, pages.length - 1);
    notifyListeners();
  }

  bool goToNextPage() {
    final pages = _measuredPages[_currentChapterIndex];
    if (pages != null && _currentPageIndex + 1 < pages.length) {
      _currentPageIndex++;
      notifyListeners();
      return true;
    }
    return goToNextChapter();
  }

  bool goToPrevPage() {
    if (_currentPageIndex > 0) {
      _currentPageIndex--;
      notifyListeners();
      return true;
    }
    return goToPrevChapter();
  }

  bool goToNextChapter() {
    if (_currentChapterIndex + 1 < _chapters.length) {
      _currentChapterIndex++;
      _currentPageIndex = 0;
      _measureCurrentChapterIfNeeded();
      notifyListeners();
      return true;
    }
    return false;
  }

  bool goToPrevChapter() {
    if (_currentChapterIndex > 0) {
      _currentChapterIndex--;
      final pages = _measuredPages[_currentChapterIndex];
      _currentPageIndex = pages != null ? (pages.length - 1).clamp(0, 999999) : 0;
      _measureCurrentChapterIfNeeded();
      notifyListeners();
      return true;
    }
    return false;
  }

  void _measureCurrentChapterIfNeeded({bool jumpToLast = false}) {
    if (_currentChapterIndex >= _chapters.length) return;
    if (_measuredPages.containsKey(_currentChapterIndex)) return;
    if (_pageSize.width <= 0 || _pageSize.height <= 0) return;
    if (_isMeasuring) return;

    final ch = _chapters[_currentChapterIndex];
    _measureChapter(ch, jumpToLast: jumpToLast);
  }

  void _measureChapter(_ChapterData ch, {bool jumpToLast = false}) {
    if (_isMeasuring) return;
    _isMeasuring = true;

    final settings = _settings;
    final pageSize = _pageSize;
    final chapterIndex = _currentChapterIndex;

    final measure = PageMeasure(
      settings: settings,
      pageSize: pageSize,
      chapterTitle: ch.title,
    );
    final result = measure.measureChapter(ch.index, ch.paragraphs);
    _measuredPages[chapterIndex] = result.pages;
    _isMeasuring = false;

    if (jumpToLast && result.pages.isNotEmpty) {
      _currentPageIndex = result.pages.length - 1;
      if (chapterIndex == _currentChapterIndex && !_disposed) {
        notifyListeners();
      }
    }

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_disposed && chapterIndex == _currentChapterIndex) {
        if (!jumpToLast) {
          notifyListeners();
        }
        _measureNeighborIfNeeded(_currentChapterIndex + 1);
        _measureNeighborIfNeeded(_currentChapterIndex - 1);
      }
    });
  }

  void _measureNeighborIfNeeded(int index) {
    if (index < 0 || index >= _chapters.length) return;
    if (_measuredPages.containsKey(index)) return;
    _measureChapter(_chapters[index]);
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class _ChapterData {
  final int index;
  final String title;
  final List<String> paragraphs;

  _ChapterData(this.index, this.title, this.paragraphs);
}
