import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import '../text_page.dart';
import '../content_page.dart';
import '../page_view_controller.dart';
import 'page_delegate.dart';

class CoverPageDelegate extends PageDelegate {
  CoverPageDelegate({
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
      _drawCover(canvas, size, currentPage, nextPage, animProgress, true, totalPages);
    } else if (direction == PageDirection.prev && prevPage != null) {
      _drawCover(canvas, size, currentPage, prevPage, animProgress, false, totalPages);
    } else {
      final painter = ContentPagePainter(page: currentPage, settings: settings, totalPages: totalPages);
      painter.paint(canvas, size);
    }
  }

  void _drawCover(
    Canvas canvas,
    Size size,
    TextPage? fromPage,
    TextPage toPage,
    double progress,
    bool forward,
    int totalPages,
  ) {
    final painter = ContentPagePainter(page: toPage, settings: settings, totalPages: totalPages);
    painter.paint(canvas, size);

    final shadowPaint = Paint()
      ..shader = ui.Gradient.linear(
        forward ? Offset(size.width * (1 - progress), 0) : Offset(size.width * progress, 0),
        forward
            ? Offset(size.width * (1 - progress) + 20, 0)
            : Offset(size.width * progress - 20, 0),
        [Colors.black.withAlpha(80), Colors.transparent],
      );

    final shadowRect = forward
        ? Rect.fromLTWH(size.width * (1 - progress) - 20, 0, 20, size.height)
        : Rect.fromLTWH(size.width * progress, 0, 20, size.height);
    canvas.drawRect(shadowRect, shadowPaint);

    if (fromPage != null) {
      canvas.save();
      final clipRect = forward
          ? Rect.fromLTWH(0, 0, size.width * (1 - progress), size.height)
          : Rect.fromLTWH(size.width * progress, 0, size.width * (1 - progress), size.height);
      canvas.clipRect(clipRect);
      final fromPainter = ContentPagePainter(page: fromPage, settings: settings, totalPages: totalPages);
      fromPainter.paint(canvas, size);
      canvas.restore();
    }
  }

  @override
  void dispose() {}
}
