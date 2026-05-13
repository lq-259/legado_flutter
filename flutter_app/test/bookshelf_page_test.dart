import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:legado_flutter/features/bookshelf/bookshelf_page.dart';
import 'package:legado_flutter/core/providers.dart';

void main() {
  Widget buildBookshelfPage({List<Map<String, dynamic>>? books}) {
    return ProviderScope(
      overrides: [
        allBooksProvider.overrideWith((ref) => Future.value(books ?? <Map<String, dynamic>>[])),
      ],
      child: const MaterialApp(home: BookshelfPage()),
    );
  }

  testWidgets('BookshelfPage shows loading indicator', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          allBooksProvider.overrideWith(
            (ref) => Future.delayed(const Duration(seconds: 1), () => <Map<String, dynamic>>[]),
          ),
        ],
        child: const MaterialApp(home: BookshelfPage()),
      ),
    );
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('BookshelfPage shows error message', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          allBooksProvider.overrideWith((ref) => Future.error('Connection failed')),
        ],
        child: const MaterialApp(home: BookshelfPage()),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('加载失败: Connection failed'), findsOneWidget);
  });

  testWidgets('BookshelfPage shows app bar title', (WidgetTester tester) async {
    await tester.pumpWidget(buildBookshelfPage());
    await tester.pumpAndSettle();
    expect(find.text('书架'), findsOneWidget);
  });

  testWidgets('BookshelfPage shows empty message when no books', (WidgetTester tester) async {
    await tester.pumpWidget(buildBookshelfPage());
    await tester.pumpAndSettle();
    expect(find.text('书架为空，去搜索添加书籍吧'), findsOneWidget);
  });

  testWidgets('BookshelfPage shows book with name and author', (WidgetTester tester) async {
    final books = [
      <String, dynamic>{
        'id': 'book1',
        'name': 'Test Book',
        'author': 'Test Author',
        'chapter_count': 42,
      },
    ];

    await tester.pumpWidget(buildBookshelfPage(books: books));
    await tester.pumpAndSettle();

    expect(find.text('Test Book'), findsOneWidget);
    expect(find.text('Test Author'), findsOneWidget);
    expect(find.text('42章'), findsOneWidget);
  });

  testWidgets('BookshelfPage shows multiple books', (WidgetTester tester) async {
    final books = [
      <String, dynamic>{
        'id': 'book1',
        'name': 'First Book',
        'author': 'Author One',
        'chapter_count': 10,
      },
      <String, dynamic>{
        'id': 'book2',
        'name': 'Second Book',
        'author': 'Author Two',
        'chapter_count': 20,
      },
    ];

    await tester.pumpWidget(buildBookshelfPage(books: books));
    await tester.pumpAndSettle();

    expect(find.text('First Book'), findsOneWidget);
    expect(find.text('Second Book'), findsOneWidget);
    expect(find.text('10章'), findsOneWidget);
    expect(find.text('20章'), findsOneWidget);
  });

  testWidgets('BookshelfPage shows book icon and handles null fields', (WidgetTester tester) async {
    final books = [
      <String, dynamic>{
        'id': 'book1',
        'name': null,
        'author': null,
        'chapter_count': null,
      },
    ];

    await tester.pumpWidget(buildBookshelfPage(books: books));
    await tester.pumpAndSettle();

    expect(find.text('未知书名'), findsOneWidget);
    expect(find.text('未知作者'), findsOneWidget);
    expect(find.text('0章'), findsOneWidget);
    expect(find.byIcon(Icons.book), findsOneWidget);
  });
}
