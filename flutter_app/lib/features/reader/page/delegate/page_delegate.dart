import 'package:flutter/material.dart';
import '../../../../core/providers.dart';
import '../text_page.dart';
import '../page_view_controller.dart';

typedef ChapterBoundaryCallback = void Function(PageDirection dir);

abstract class PageDelegate {
  final PageViewController controller;
  final ReaderSettings settings;
  final AnimationController animController;
  ChapterBoundaryCallback? onChapterBoundary;

  bool isRunning = false;
  PageDirection _direction = PageDirection.none;
  double _dragOffset = 0;

  PageDelegate({
    required this.controller,
    required this.settings,
    required this.animController,
    this.onChapterBoundary,
  });

  PageDirection get direction => _direction;
  double get dragOffset => _dragOffset;

  void onDragUpdate(double delta) {
    if (isRunning) return;
    _dragOffset += delta;

    final totalWidth = animController.upperBound > 0
        ? (animController.upperBound - animController.lowerBound)
        : 300.0;
    if (_dragOffset > 5 && _direction == PageDirection.none) {
      _direction = PageDirection.prev;
    } else if (_dragOffset < -5 && _direction == PageDirection.none) {
      _direction = PageDirection.next;
    }

    final progress = (_dragOffset.abs() / totalWidth).clamp(0.0, 1.0);
    if (_direction == PageDirection.prev && !controller.hasPrev) {
      animController.value = 0;
      return;
    }
    if (_direction == PageDirection.next && !controller.hasNext) {
      animController.value = 0;
      return;
    }
    animController.value = progress;
  }

  void onDragEnd(PageDirection detectedDir) {
    if (isRunning) return;

    if (_direction == PageDirection.none) {
      _direction = detectedDir;
    }

    if (_direction == PageDirection.next) {
      goToNext();
    } else if (_direction == PageDirection.prev) {
      goToPrev();
    } else {
      _resetState();
    }
  }

  void goToNext() {
    if (!controller.hasNext) {
      _resetState();
      onChapterBoundary?.call(PageDirection.next);
      return;
    }
    _direction = PageDirection.next;
    _runAnimation(() => controller.goToNextPage());
  }

  void goToPrev() {
    if (!controller.hasPrev) {
      _resetState();
      onChapterBoundary?.call(PageDirection.prev);
      return;
    }
    _direction = PageDirection.prev;
    _runAnimation(() => controller.goToPrevPage());
  }

  void _runAnimation(VoidCallback onComplete) {
    if (isRunning) return;
    isRunning = true;
    animController.forward(from: animController.value).then((_) {
      onComplete();
      _resetState();
    });
  }

  void _resetState() {
    _direction = PageDirection.none;
    _dragOffset = 0;
    isRunning = false;
    animController.value = 0;
  }

  void draw(
    Canvas canvas,
    Size size, {
    required TextPage? currentPage,
    required TextPage? nextPage,
    required TextPage? prevPage,
    required double animProgress,
    required int totalPages,
  });

  void dispose();
}
