import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'dart:io';
import 'package:legado_flutter/core/providers.dart';
import 'package:legado_flutter/features/settings/settings_page.dart';
import 'package:legado_flutter/main.dart';

void main() {
  testWidgets('App builds without crashing', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          dbDirProvider.overrideWith((ref) => Future.value('.')),
          dbPathProvider.overrideWith((ref) => Future.value('legado_test.db')),
          dbInitializedProvider.overrideWith((ref) => Future.value(true)),
          allBooksProvider.overrideWith((ref) => Future.value([])),
          allSourcesProvider.overrideWith((ref) => Future.value([])),
        ],
        child: const LegadoApp(),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byType(MaterialApp), findsOneWidget);
  });

  testWidgets('Settings page shows notification not granted', (WidgetTester tester) async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
      const MethodChannel('legado/notifications'),
      (MethodCall call) async {
        if (call.method == 'hasPermission') return false;
        return null;
      },
    );

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(
          home: SettingsPage(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('设置'), findsOneWidget);
    expect(find.text('未授权（点击开启）'), findsOneWidget);
  });

  testWidgets('Settings page shows notification authorized', (WidgetTester tester) async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
      const MethodChannel('legado/notifications'),
      (MethodCall call) async {
        if (call.method == 'hasPermission') return true;
        return null;
      },
    );

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(
          home: SettingsPage(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('已授权'), findsOneWidget);
  });

  testWidgets('Settings page renders theme options', (WidgetTester tester) async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
      const MethodChannel('legado/notifications'),
      (MethodCall call) async {
        if (call.method == 'hasPermission') return false;
        return null;
      },
    );

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(
          home: SettingsPage(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('浅色模式'), findsOneWidget);
    expect(find.text('深色模式'), findsOneWidget);
    expect(find.text('跟随系统'), findsOneWidget);
  });

  testWidgets('Settings page requests permission on switch tap', (WidgetTester tester) async {
    var requestCalled = false;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
      const MethodChannel('legado/notifications'),
      (MethodCall call) async {
        if (call.method == 'hasPermission') return false;
        if (call.method == 'requestPermission') {
          requestCalled = true;
          return true;
        }
        return null;
      },
    );

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(
          home: SettingsPage(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();

    expect(requestCalled, isTrue);
  });

  testWidgets('Settings page shows dialog on notification off', (WidgetTester tester) async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
      const MethodChannel('legado/notifications'),
      (MethodCall call) async {
        if (call.method == 'hasPermission') return true;
        return null;
      },
    );

    await tester.pumpWidget(
      const ProviderScope(
        child: MaterialApp(
          home: SettingsPage(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();

    expect(find.text('请在系统设置中关闭通知权限'), findsOneWidget);
    expect(find.text('去设置'), findsOneWidget);
  });

  test('Theme persistence round-trips through disk', () async {
    final tempDir = Directory.systemTemp.createTempSync('theme_test_').path;
    try {
      await saveThemeModeToDisk(ThemeMode.dark, directory: tempDir);
      final loaded = await loadThemeModeFromDisk(directory: tempDir);
      expect(loaded, ThemeMode.dark);

      await saveThemeModeToDisk(ThemeMode.light, directory: tempDir);
      final loaded2 = await loadThemeModeFromDisk(directory: tempDir);
      expect(loaded2, ThemeMode.light);
    } finally {
      Directory(tempDir).deleteSync(recursive: true);
    }
  });

  test('Theme persistence returns system when no file', () async {
    final tempDir = Directory.systemTemp.createTempSync('theme_test_empty_').path;
    try {
      final loaded = await loadThemeModeFromDisk(directory: tempDir);
      expect(loaded, ThemeMode.system);
    } finally {
      Directory(tempDir).deleteSync(recursive: true);
    }
  });
}
