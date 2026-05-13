import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:legado_flutter/features/search/search_page.dart';
import 'package:legado_flutter/core/providers.dart';

void main() {
  Widget buildSearchPage() {
    return ProviderScope(
      overrides: [
        dbDirProvider.overrideWith((ref) => Future.value('.')),
        dbPathProvider.overrideWith((ref) => Future.value('test_legado.db')),
        dbInitializedProvider.overrideWith((ref) => Future.value(true)),
      ],
      child: const MaterialApp(home: SearchPage()),
    );
  }

  testWidgets('SearchPage shows app bar title', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();
    expect(find.text('搜索'), findsOneWidget);
  });

  testWidgets('SearchPage shows hint text in input field', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();
    expect(find.text('输入书名或作者'), findsOneWidget);
  });

  testWidgets('SearchPage shows empty state message', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();
    expect(find.text('输入关键词搜索书籍'), findsOneWidget);
  });

  testWidgets('SearchPage shows search icon prefix', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.search), findsOneWidget);
  });

  testWidgets('SearchPage shows send button', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.send), findsOneWidget);
  });

  testWidgets('SearchPage does not crash when disposed during async offline search', (WidgetTester tester) async {
    final completer = Completer<bool>();

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          dbDirProvider.overrideWith((ref) => Future.value('.')),
          dbPathProvider.overrideWith((ref) => Future.value('test_legado.db')),
          dbInitializedProvider.overrideWith((ref) => completer.future),
        ],
        child: const MaterialApp(home: SearchPage()),
      ),
    );
    await tester.pumpAndSettle();

    // Enter text to enable search
    await tester.enterText(find.byType(TextField), 'test');
    await tester.pumpAndSettle();

    // Trigger search (default offline mode) — enters await ref.read(dbInitializedProvider.future)
    await tester.tap(find.byIcon(Icons.send));
    await tester.pump();
    // Search is now suspended on the hanging dbInitializedProvider future

    // Navigate away, disposing SearchPage
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: Text('SAFE'))),
    );
    await tester.pump();

    // Complete the future that SearchPage was awaiting — code resumes at mounted guard
    completer.complete(true);
    await tester.pump();

    // If we reach here without exception, the mounted guard worked
    expect(find.text('SAFE'), findsOneWidget);
  });

  testWidgets('SearchPage can enter text in search field', (WidgetTester tester) async {
    await tester.pumpWidget(buildSearchPage());
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'test keyword');
    await tester.pumpAndSettle();

    expect(find.text('test keyword'), findsOneWidget);
  });
}
