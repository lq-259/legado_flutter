import 'package:flutter/material.dart';
import '../text_page.dart';
import '../content_page.dart';
import 'page_delegate.dart';

class NoAnimPageDelegate extends PageDelegate {
  NoAnimPageDelegate({
    required super.controller,
    required super.settings,
    required super.animController,
    super.onChapterBoundary,
  });

  @override
  void draw(
    Canvas canvas,
    Size size, {
    required TextPage? currentPage,
    required TextPage? nextPage,
    required TextPage? prevPage,
    required double animProgress,
    required int totalPages,
  }) {
    final painter = ContentPagePainter(
      page: currentPage,
      settings: settings,
      totalPages: totalPages,
    );
    painter.paint(canvas, size);
  }

  @override
  void dispose() {}
}
