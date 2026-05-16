import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import '../../../core/providers.dart';
import 'text_page.dart';

class ContentPage extends StatelessWidget {
  final TextPage? page;
  final ReaderSettings settings;
  final Size pageSize;

  const ContentPage({
    super.key,
    required this.page,
    required this.settings,
    required this.pageSize,
  });

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: pageSize,
      painter: ContentPagePainter(
        page: page,
        settings: settings,
      ),
    );
  }
}

class ContentPagePainter extends CustomPainter {
  final TextPage? page;
  final ReaderSettings settings;
  final int totalPages;

  ContentPagePainter({
    required this.page,
    required this.settings,
    this.totalPages = 0,
  });

  @override
  void paint(Canvas canvas, Size size) {
    if (page == null || page!.paragraphTexts.isEmpty) {
      _paintEmptyPage(canvas, size);
      return;
    }

    final bgColor = Color(settings.effectiveBackgroundColor);
    canvas.drawRect(
        Rect.fromLTWH(0, 0, size.width, size.height), Paint()..color = bgColor);

    final availableWidth = size.width - settings.horizontalPadding * 2;
    double currentY = settings.verticalPadding;

    final paragraphStyle = ui.ParagraphStyle(textDirection: TextDirection.ltr);
    final textStyle = ui.TextStyle(
      color: Color(settings.effectiveTextColor),
      fontSize: settings.fontSize,
      fontWeight: FontWeight.values[
          settings.fontWeightIndex.clamp(0, FontWeight.values.length - 1)],
      letterSpacing: settings.letterSpacing,
      height: settings.lineHeight,
    );

    for (var i = 0; i < page!.paragraphTexts.length; i++) {
      final paraText = page!.paragraphTexts[i];
      final builder = ui.ParagraphBuilder(paragraphStyle)
        ..pushStyle(textStyle)
        ..addText(paraText);
      final paragraph = builder.build()
        ..layout(ui.ParagraphConstraints(width: availableWidth));
      canvas.drawParagraph(
          paragraph, Offset(settings.horizontalPadding, currentY));
      currentY += paragraph.height;
      if (i < page!.paragraphTexts.length - 1) {
        currentY += settings.paragraphSpacing;
      }
    }

  }

  void _paintEmptyPage(Canvas canvas, Size size) {
    final bgColor = Color(settings.effectiveBackgroundColor);
    canvas.drawRect(
        Rect.fromLTWH(0, 0, size.width, size.height), Paint()..color = bgColor);
    final paragraphStyle = ui.ParagraphStyle(textDirection: TextDirection.ltr);
    final textStyle = ui.TextStyle(
      color: Color(settings.effectiveTextColor).withAlpha(120),
      fontSize: settings.fontSize,
      fontWeight: FontWeight.values[
          settings.fontWeightIndex.clamp(0, FontWeight.values.length - 1)],
    );
    final builder = ui.ParagraphBuilder(paragraphStyle)
      ..pushStyle(textStyle)
      ..addText('（本章无内容）');
    final paragraph = builder.build()
      ..layout(ui.ParagraphConstraints(
          width: size.width - settings.horizontalPadding * 2));
    canvas.drawParagraph(
        paragraph,
        Offset(settings.horizontalPadding,
            size.height / 2 - paragraph.height / 2));
  }

  @override
  bool shouldRepaint(covariant ContentPagePainter oldDelegate) {
    return page != oldDelegate.page || settings != oldDelegate.settings;
  }
}
