import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:legado_flutter/features/reader/reader_page.dart';
import 'package:legado_flutter/core/providers.dart';

void main() {
  testWidgets('shows error when bookId is empty', (WidgetTester tester) async {
    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(home: ReaderPage(bookId: '')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('未指定书籍'), findsOneWidget);
  });

  testWidgets('shows loading indicator when chapters loading', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_1').overrideWith(
            (ref) => Future.delayed(const Duration(seconds: 1), () => <Map<String, dynamic>>[]),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_1')),
      ),
    );
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('阅读器'), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('shows error when chapters fail to load', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_2').overrideWith(
            (ref) => Future.error('Network error'),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_2')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('加载章节失败: Network error'), findsOneWidget);
    expect(find.text('阅读器'), findsOneWidget);
  });

  testWidgets('shows empty message when no chapters', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_3').overrideWith(
            (ref) => Future.value(<Map<String, dynamic>>[]),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_3')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('暂无章节'), findsOneWidget);
    expect(find.text('阅读器'), findsOneWidget);
  });

  testWidgets('shows chapter list with titles', (WidgetTester tester) async {
    final chapters = [
      <String, dynamic>{'title': 'Chapter 1', 'url': '/ch1', 'id': 'ch1'},
      <String, dynamic>{'title': 'Chapter 2', 'url': '/ch2', 'id': 'ch2'},
      <String, dynamic>{'title': 'Chapter 3', 'url': '/ch3', 'id': 'ch3'},
    ];
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_4').overrideWith(
            (ref) => Future.value(chapters),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_4')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('目录'), findsOneWidget);
    expect(find.text('Chapter 1'), findsOneWidget);
    expect(find.text('Chapter 2'), findsOneWidget);
    expect(find.text('Chapter 3'), findsOneWidget);
    expect(find.byIcon(Icons.arrow_back), findsOneWidget);
    expect(find.byType(ListTile), findsNWidgets(3));
  });

  testWidgets('shows fallback title for null chapter names', (WidgetTester tester) async {
    final chapters = [
      <String, dynamic>{'title': null, 'url': '/ch1', 'id': 'ch1'},
      <String, dynamic>{'title': null, 'url': '/ch2', 'id': 'ch2'},
    ];
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_5').overrideWith(
            (ref) => Future.value(chapters),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_5')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('章节 1'), findsOneWidget);
    expect(find.text('章节 2'), findsOneWidget);
  });

  testWidgets('shows app bar title in loading state', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_6').overrideWith(
            (ref) => Future.delayed(const Duration(seconds: 1), () => <Map<String, dynamic>>[]),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_6')),
      ),
    );
    await tester.pump();
    expect(find.text('阅读器'), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('shows app bar title in error state', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          bookChaptersProvider('book_7').overrideWith(
            (ref) => Future.error('Error'),
          ),
        ],
        child: const MaterialApp(home: ReaderPage(bookId: 'book_7')),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('阅读器'), findsOneWidget);
  });
}
