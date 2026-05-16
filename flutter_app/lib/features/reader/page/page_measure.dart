import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import '../../../core/providers.dart';
import 'text_page.dart';

class PageMeasureResult {
  final List<TextPage> pages;
  final Map<int, List<int>> chapterPageIndices;

  const PageMeasureResult({
    required this.pages,
    required this.chapterPageIndices,
  });

  int get totalPages => pages.length;
}

class PageMeasure {
  final ReaderSettings settings;
  final Size pageSize;
  final String? chapterTitle;

  PageMeasure({
    required this.settings,
    required this.pageSize,
    this.chapterTitle,
  });

  static const double _footerHeight = 36.0;

  double get _contentWidth => pageSize.width - settings.horizontalPadding * 2;

  double get _contentHeight =>
      pageSize.height - settings.verticalPadding * 2 - _footerHeight;

  ui.ParagraphStyle get _paragraphStyle {
    return ui.ParagraphStyle(
      textDirection: TextDirection.ltr,
      maxLines: null,
    );
  }

  ui.TextStyle get _textStyle {
    return ui.TextStyle(
      color: Color(settings.effectiveTextColor),
      fontSize: settings.fontSize,
      fontWeight: FontWeight.values[
          settings.fontWeightIndex.clamp(0, FontWeight.values.length - 1)],
      letterSpacing: settings.letterSpacing,
      height: settings.lineHeight,
    );
  }

  ui.Paragraph _buildParagraph(String text) {
    final builder = ui.ParagraphBuilder(_paragraphStyle)
      ..pushStyle(_textStyle)
      ..addText(text);
    return builder.build()
      ..layout(ui.ParagraphConstraints(width: _contentWidth));
  }

  PageMeasureResult measureChapter(
    int chapterIndex,
    List<String> paragraphs,
  ) {
    final pages = <TextPage>[];
    final pageIndices = <int>[];

    int currentPageIndex = 0;
    int startParagraph = 0;
    int startCharOffset = 0;
    double usedHeight = 0;
    final currentPageParagraphs = <String>[];

    int i = 0;
    String? pendingText;

    while (i < paragraphs.length || pendingText != null) {
      final bool isPending = pendingText != null;
      final String paraText = pendingText ?? paragraphs[i];

      final displayText = settings.paragraphIndent.isNotEmpty && !isPending
          ? '${settings.paragraphIndent}$paraText'
          : paraText;

      final paragraph = _buildParagraph(displayText);
      final paraHeight = paragraph.height;
      final spacing =
          currentPageParagraphs.isEmpty ? 0.0 : settings.paragraphSpacing;

      if (usedHeight + spacing + paraHeight <= _contentHeight) {
        currentPageParagraphs.add(displayText);
        usedHeight += spacing + paraHeight;
        if (isPending) {
          pendingText = null;
          i++;
        } else {
          i++;
        }
      } else {
        final lines = paragraph.computeLineMetrics();
        final remaining = _contentHeight - usedHeight - spacing;

        int fittingLines = 0;
        double fitHeight = 0;
        for (final lm in lines) {
          if (fitHeight + lm.height <= remaining) {
            fitHeight += lm.height;
            fittingLines++;
          } else {
            break;
          }
        }

        if (fittingLines > 0) {
          int endOffset = _charOffsetForLine(
              paragraph, lines, fittingLines, displayText.length);
          currentPageParagraphs.add(displayText.substring(0, endOffset));
        }

        if (currentPageParagraphs.isNotEmpty) {
          _finalizePage(
            pages,
            pageIndices,
            currentPageParagraphs,
            chapterIndex,
            currentPageIndex,
            startParagraph,
            i,
            startCharOffset,
          );
          currentPageIndex++;
        }
        currentPageParagraphs.clear();
        usedHeight = 0;

        if (fittingLines < lines.length) {
          int startOffset = _charOffsetForLine(
              paragraph, lines, fittingLines, displayText.length);
          pendingText = displayText.substring(startOffset);
          startParagraph = i;
          startCharOffset =
              paragraphs.take(i).fold(0, (sum, p) => sum + p.length);
        } else {
          pendingText = null;
          i++;
          startParagraph = i;
          startCharOffset = i <= paragraphs.length
              ? paragraphs.take(i).fold(0, (sum, p) => sum + p.length)
              : 0;
        }
      }
    }

    if (currentPageParagraphs.isNotEmpty) {
      _finalizePage(
        pages,
        pageIndices,
        currentPageParagraphs,
        chapterIndex,
        currentPageIndex,
        startParagraph,
        paragraphs.length - 1,
        startCharOffset,
      );
      currentPageIndex++;
    }

    if (pages.isEmpty) {
      pages.add(TextPage(
        chapterIndex: chapterIndex,
        pageIndex: 0,
        startParagraphIndex: 0,
        endParagraphIndex: 0,
        startCharOffset: 0,
        endCharOffset: 0,
        paragraphTexts: const [],
        headerText: chapterTitle,
        contentHeight: 0,
      ));
      pageIndices.add(0);
    }

    final chapterPageIndices = <int, List<int>>{};
    chapterPageIndices[chapterIndex] = pageIndices;
    return PageMeasureResult(
        pages: pages, chapterPageIndices: chapterPageIndices);
  }

  void _finalizePage(
    List<TextPage> pages,
    List<int> pageIndices,
    List<String> paragraphTexts,
    int chapterIndex,
    int pageIndex,
    int startParagraph,
    int endParagraph,
    int startCharOffset,
  ) {
    final endCharOffset = startCharOffset +
        paragraphTexts.fold<int>(0, (sum, p) => sum + p.length);
    pages.add(TextPage(
      chapterIndex: chapterIndex,
      pageIndex: pageIndex,
      startParagraphIndex: startParagraph,
      endParagraphIndex: endParagraph,
      startCharOffset: startCharOffset,
      endCharOffset: endCharOffset,
      paragraphTexts: List.from(paragraphTexts),
      headerText: chapterTitle,
      contentHeight: _contentHeight,
    ));
    pageIndices.add(pageIndex);
  }

  int _charOffsetForLine(ui.Paragraph paragraph, List<ui.LineMetrics> lines,
      int lineIndex, int textLength) {
    if (lineIndex <= 0) return 0;
    if (lineIndex >= lines.length) return textLength;
    final lm = lines[lineIndex];
    final pos = paragraph.getPositionForOffset(
      ui.Offset(0, lm.baseline - lm.ascent),
    );
    return pos.offset.clamp(0, textLength);
  }
}
