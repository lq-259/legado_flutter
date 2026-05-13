import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:legado_flutter/features/download/download_page.dart';
import 'package:legado_flutter/core/providers.dart';

void main() {
  testWidgets('DownloadPage shows loading indicator', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith(
            (ref) => Future.delayed(const Duration(seconds: 1), () => <Map<String, dynamic>>[]),
          ),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('DownloadPage shows error message', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.error('Connection failed')),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('加载失败: Connection failed'), findsOneWidget);
  });

  testWidgets('DownloadPage shows empty message when no tasks', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(<Map<String, dynamic>>[])),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('暂无下载任务，在阅读器中点击下载按钮开始'), findsOneWidget);
  });

  testWidgets('DownloadPage shows task with downloading status', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task1',
        'book_id': 'book1',
        'book_name': 'Test Book',
        'status': 1,
        'total_chapters': 10,
        'downloaded_chapters': 5,
        'error_message': null,
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Test Book'), findsOneWidget);
    expect(find.text('下载中'), findsOneWidget);
    expect(find.byIcon(Icons.downloading), findsOneWidget);
    expect(find.text('5 / 10 章'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
  });

  testWidgets('DownloadPage shows task with completed status', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task2',
        'book_id': 'book2',
        'book_name': 'Completed Book',
        'status': 3,
        'total_chapters': 20,
        'downloaded_chapters': 20,
        'error_message': null,
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Completed Book'), findsOneWidget);
    expect(find.text('已完成'), findsOneWidget);
    expect(find.byIcon(Icons.check_circle), findsOneWidget);
  });

  testWidgets('DownloadPage shows task with failed status and error', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task3',
        'book_id': 'book3',
        'book_name': 'Failed Book',
        'status': 4,
        'total_chapters': 5,
        'downloaded_chapters': 2,
        'error_message': 'Network timeout',
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Failed Book'), findsOneWidget);
    expect(find.text('失败'), findsOneWidget);
    expect(find.byIcon(Icons.error), findsOneWidget);
    expect(find.text('Network timeout'), findsOneWidget);
  });

  testWidgets('DownloadPage shows task with paused status', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task4',
        'book_id': 'book4',
        'book_name': 'Paused Book',
        'status': 2,
        'total_chapters': 8,
        'downloaded_chapters': 3,
        'error_message': null,
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('已暂停'), findsOneWidget);
    expect(find.byIcon(Icons.pause_circle), findsOneWidget);
  });

  testWidgets('DownloadPage shows task with waiting status', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task5',
        'book_id': 'book5',
        'book_name': 'Waiting Book',
        'status': 0,
        'total_chapters': 3,
        'downloaded_chapters': 0,
        'error_message': null,
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('等待中'), findsOneWidget);
    expect(find.byIcon(Icons.hourglass_empty), findsOneWidget);
  });

  testWidgets('DownloadPage has refresh button', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(<Map<String, dynamic>>[])),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.refresh), findsOneWidget);
  });

  testWidgets('DownloadPage shows multiple tasks', (WidgetTester tester) async {
    final tasks = [
      <String, dynamic>{
        'id': 'task_a',
        'book_id': 'book_a',
        'book_name': 'Book A',
        'status': 3,
        'total_chapters': 10,
        'downloaded_chapters': 10,
        'error_message': null,
      },
      <String, dynamic>{
        'id': 'task_b',
        'book_id': 'book_b',
        'book_name': 'Book B',
        'status': 1,
        'total_chapters': 5,
        'downloaded_chapters': 2,
        'error_message': null,
      },
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          downloadTasksProvider.overrideWith((ref) => Future.value(tasks)),
        ],
        child: const MaterialApp(home: DownloadPage()),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Book A'), findsOneWidget);
    expect(find.text('Book B'), findsOneWidget);
    expect(find.text('已完成'), findsOneWidget);
    expect(find.text('下载中'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsNWidgets(2));
  });
}
