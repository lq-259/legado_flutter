import 'package:flutter/material.dart';
import '../text_page.dart';
import '../content_page.dart';
import '../page_view_controller.dart';
import 'page_delegate.dart';

class SlidePageDelegate extends PageDelegate {
  SlidePageDelegate({
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
    if (direction == PageDirection.next && nextPage != null) {
      _drawSlide(canvas, size, currentPage, nextPage, animProgress, true, totalPages);
    } else if (direction == PageDirection.prev && prevPage != null) {
      _drawSlide(canvas, size, currentPage, prevPage, animProgress, false, totalPages);
    } else {
      final painter = ContentPagePainter(page: currentPage, settings: settings, totalPages: totalPages);
      painter.paint(canvas, size);
    }
  }

  void _drawSlide(
    Canvas canvas,
    Size size,
    TextPage? fromPage,
    TextPage toPage,
    double progress,
    bool forward,
    int totalPages,
  ) {
    final offsetX = forward ? -size.width * progress : size.width * progress;

    canvas.save();
    canvas.translate(offsetX, 0);
    if (fromPage != null) {
      final fromPainter = ContentPagePainter(page: fromPage, settings: settings, totalPages: totalPages);
      fromPainter.paint(canvas, size);
    }
    canvas.restore();

    final nextOffsetX = forward ? size.width * (1 - progress) : -size.width * (1 - progress);
    canvas.save();
    canvas.translate(nextOffsetX, 0);
    final toPainter = ContentPagePainter(page: toPage, settings: settings, totalPages: totalPages);
    toPainter.paint(canvas, size);
    canvas.restore();
  }

  @override
  void dispose() {}
}
