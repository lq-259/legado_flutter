import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/download_runner.dart';
import 'core/notification_service.dart';
import 'core/providers.dart';
import 'core/router.dart';
import 'core/theme.dart';
import 'src/rust/api.dart' as rust_api;
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  debugRepaintTextRainbowEnabled = false;
  debugRepaintRainbowEnabled = false;
  debugPaintSizeEnabled = false;
  debugPaintBaselinesEnabled = false;
  debugPaintTextLayoutBoxes = false;
  debugPaintLayerBordersEnabled = false;

  try {
    await RustLib.init();
    final pong = await rust_api.ping();
    debugPrint('[FRB smoke] ping() returned: $pong');
    if (pong != 'pong') {
      debugPrint('[FRB smoke] WARNING: unexpected ping response: $pong');
    }
  } catch (e, st) {
    debugPrint('[FRB smoke] init/ping FAILED: $e');
    debugPrint('[FRB smoke] stack: $st');
    // Re-throw so the app doesn't silently continue with a broken bridge.
    // In debug mode this will show the red error screen, in release it will crash.
    rethrow;
  }

  await NotificationService.init();

  final themeMode = await loadThemeModeFromDisk();
  final fontSize = await loadFontSizeFromDisk();
  runApp(ProviderScope(
    overrides: [
      themeModeProvider.overrideWith((ref) => themeMode),
      fontSizeProvider.overrideWith((ref) => fontSize),
    ],
    child: const LegadoApp(),
  ));
  WidgetsBinding.instance.addPostFrameCallback((_) async {
    final route = await loadPendingRoute();
    if (route != null) {
      router.go(route);
      await clearPendingRoute();
    }
  });
}

class LegadoApp extends ConsumerWidget {
  const LegadoApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeModeProvider);
    // Trigger DB init eagerly
    ref.listen(dbInitializedProvider, (_, state) {
      state.whenOrNull(
        data: (ok) {
          debugPrint('[FRB] DB init: $ok');
          if (ok) {
            final dbPath = ref.read(dbPathProvider).valueOrNull;
            if (dbPath != null) {
              DownloadRunner.resetInterruptedTasks(dbPath);
            }
          }
        },
        error: (e, _) => debugPrint('[FRB] DB init error: $e'),
      );
    });

    return MaterialApp.router(
      title: 'Legado Reader',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light,
      darkTheme: AppTheme.dark,
      themeMode: themeMode,
      routerConfig: router,
    );
  }
}
