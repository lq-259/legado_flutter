import 'package:flutter/material.dart';
import '../../../core/providers.dart';
import 'text_page.dart';
import 'page_view_controller.dart';
import 'delegate/page_delegate.dart';
import 'delegate/no_anim_page_delegate.dart';
import 'delegate/cover_page_delegate.dart';
import 'delegate/slide_page_delegate.dart';

class PageViewWidget extends StatefulWidget {
  final PageViewController controller;
  final ReaderSettings settings;
  final int pageAnim;
  final ChapterBoundaryCallback? onChapterBoundary;

  const PageViewWidget({
    super.key,
    required this.controller,
    required this.settings,
    this.pageAnim = 0,
    this.onChapterBoundary,
  });

  @override
  State<PageViewWidget> createState() => _PageViewWidgetState();
}

class _PageViewWidgetState extends State<PageViewWidget>
    with TickerProviderStateMixin {
  late PageDelegate _delegate;
  AnimationController? _animController;

  @override
  void initState() {
    super.initState();
    _createDelegate();
    widget.controller.addListener(_onControllerChanged);
  }

  @override
  void didUpdateWidget(PageViewWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.pageAnim != widget.pageAnim ||
        oldWidget.settings.effectiveTextColor != widget.settings.effectiveTextColor ||
        oldWidget.settings.effectiveBackgroundColor != widget.settings.effectiveBackgroundColor) {
      _createDelegate();
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onControllerChanged);
    _animController?.dispose();
    _delegate.dispose();
    super.dispose();
  }

  void _onControllerChanged() {
    if (mounted) setState(() {});
  }

  void _createDelegate() {
    _animController?.dispose();
    _animController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 300),
    );

    final ctrl = widget.controller;
    switch (widget.pageAnim) {
      case 0:
        _delegate = NoAnimPageDelegate(
          controller: ctrl,
          settings: widget.settings,
          animController: _animController!,
          onChapterBoundary: widget.onChapterBoundary,
        );
        break;
      case 2:
        _delegate = CoverPageDelegate(
          controller: ctrl,
          settings: widget.settings,
          animController: _animController!,
          onChapterBoundary: widget.onChapterBoundary,
        );
        break;
      case 3:
        _delegate = SlidePageDelegate(
          controller: ctrl,
          settings: widget.settings,
          animController: _animController!,
          onChapterBoundary: widget.onChapterBoundary,
        );
        break;
      default:
        _delegate = NoAnimPageDelegate(
          controller: ctrl,
          settings: widget.settings,
          animController: _animController!,
          onChapterBoundary: widget.onChapterBoundary,
        );
    }
  }

  PageDirection _detectDirection(double velocityX) {
    if (velocityX < -50) return PageDirection.next;
    if (velocityX > 50) return PageDirection.prev;
    return PageDirection.none;
  }

  void _onHorizontalDragUpdate(DragUpdateDetails details) {
    _delegate.onDragUpdate(details.primaryDelta ?? 0);
  }

  void _onHorizontalDragEnd(DragEndDetails details) {
    final dir = _detectDirection(details.primaryVelocity ?? 0);
    _delegate.onDragEnd(dir);
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = Size(constraints.maxWidth, constraints.maxHeight);
        widget.controller.updatePageSize(size);

        return GestureDetector(
          onHorizontalDragUpdate: _onHorizontalDragUpdate,
          onHorizontalDragEnd: _onHorizontalDragEnd,
          child: AnimatedBuilder(
            animation: Listenable.merge([widget.controller, _animController!]),
            builder: (context, child) {
              return CustomPaint(
                size: size,
                painter: _PageViewPainter(
                  delegate: _delegate,
                  currentPage: widget.controller.currentPage,
                  nextPage: widget.controller.nextPage,
                  prevPage: widget.controller.prevPage,
                  settings: widget.settings,
                  animProgress: _animController!.value,
                  direction: _delegate.direction,
                  isRunning: _delegate.isRunning,
                  totalPages: widget.controller.totalPagesInChapter,
                ),
              );
            },
          ),
        );
      },
    );
  }
}

class _PageViewPainter extends CustomPainter {
  final PageDelegate delegate;
  final TextPage? currentPage;
  final TextPage? nextPage;
  final TextPage? prevPage;
  final ReaderSettings settings;
  final double animProgress;
  final PageDirection direction;
  final bool isRunning;
  final int totalPages;

  _PageViewPainter({
    required this.delegate,
    required this.currentPage,
    required this.nextPage,
    required this.prevPage,
    required this.settings,
    required this.animProgress,
    required this.direction,
    required this.isRunning,
    required this.totalPages,
  });

  @override
  void paint(Canvas canvas, Size size) {
    delegate.draw(
      canvas,
      size,
      currentPage: currentPage,
      nextPage: nextPage,
      prevPage: prevPage,
      animProgress: animProgress,
      totalPages: totalPages,
    );
  }

  @override
  bool shouldRepaint(covariant _PageViewPainter oldDelegate) {
    return true;
  }
}
