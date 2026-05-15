import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'theme.dart';
import '../src/rust/api.dart' as rust_api;

import 'api/api_client.dart';
import 'api/reader_api.dart';
import 'api/bookshelf_api.dart';
import 'api/search_api.dart';
import 'api/source_api.dart';

// HTTP mode is retained only for future/debug clients. Android app data
// providers below intentionally use FRB so reads and writes share one store.
enum BackendMode { frb, http }

final backendModeProvider = StateProvider<BackendMode>((ref) => BackendMode.frb);

final apiBaseUrlProvider = StateProvider<String>((ref) => 'http://localhost:3000');

final apiTokenProvider = StateProvider<String?>((ref) => null);

final apiClientProvider = Provider<ApiClient>((ref) {
  final baseUrl = ref.watch(apiBaseUrlProvider);
  final token = ref.watch(apiTokenProvider);
  return ApiClient(baseUrl: baseUrl, token: token);
});

final readerApiProvider = Provider<ReaderApi>((ref) {
  final client = ref.watch(apiClientProvider);
  return ReaderApi(client);
});

final bookshelfApiProvider = Provider<BookshelfApi>((ref) {
  final client = ref.watch(apiClientProvider);
  return BookshelfApi(client);
});

final sourceApiProvider = Provider<SourceApi>((ref) {
  final client = ref.watch(apiClientProvider);
  return SourceApi(client);
});

final searchApiProvider = Provider<SearchApi>((ref) {
  final client = ref.watch(apiClientProvider);
  return SearchApi(client);
});

final themeModeProvider = StateProvider<ThemeMode>((ref) => ThemeMode.system);

final lightThemeProvider = Provider<ThemeData>((ref) => AppTheme.light);

final darkThemeProvider = Provider<ThemeData>((ref) => AppTheme.dark);

final fontSizeProvider = StateProvider<double>((ref) => 18.0);

final dbDirProvider = FutureProvider<String>((ref) async {
  if (kIsWeb) return '.';
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    return dir;
  } catch (e) {
    return '.';
  }
});

final dbPathProvider = FutureProvider<String>((ref) async {
  final dbDir = await ref.watch(dbDirProvider.future);
  return '$dbDir/legado.db';
});

final dbInitializedProvider = FutureProvider<bool>((ref) async {
  final dbPath = await ref.watch(dbPathProvider.future);
  try {
    final result = await rust_api.initLegado(dbPath: dbPath);
    debugPrint('[FRB] initLegado: $result');
    final version = await rust_api.getDbVersion(dbPath: dbPath);
    debugPrint('[FRB] DB version: $version');
    return true;
  } catch (e) {
    debugPrint('[FRB] initLegado failed: $e');
    rethrow;
  }
});

final allBooksProvider = FutureProvider<List<Map<String, dynamic>>>((ref) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getAllBooks(dbPath: dbPath);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final allSourcesProvider = FutureProvider<List<Map<String, dynamic>>>((ref) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getAllSources(dbPath: dbPath);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final allReplaceRulesProvider = FutureProvider<List<Map<String, dynamic>>>((ref) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getReplaceRules(dbPath: dbPath);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final downloadDirProvider = FutureProvider<String>((ref) async {
  final dbDir = await ref.watch(dbDirProvider.future);
  return '$dbDir/downloads';
});

final downloadTasksProvider = FutureProvider<List<Map<String, dynamic>>>((ref) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getDownloadTasks(dbPath: dbPath);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final downloadChaptersProvider =
    FutureProvider.family<List<Map<String, dynamic>>, String>((ref, taskId) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getDownloadChapters(dbPath: dbPath, taskId: taskId);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final searchResultsProvider = FutureProvider.family<List<Map<String, dynamic>>, String>((ref, keyword) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.searchBooksOffline(dbPath: dbPath, keyword: keyword);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

final bookByIdProvider =
    FutureProvider.family<Map<String, dynamic>?, String>((ref, bookId) async {
  final books = await ref.watch(allBooksProvider.future);
  for (final book in books) {
    if (book['id'] == bookId) return book;
  }
  return null;
});

final bookChaptersProvider =
    FutureProvider.family<List<Map<String, dynamic>>, String>(
        (ref, bookId) async {
  await ref.watch(dbInitializedProvider.future);
  final dbPath = await ref.watch(dbPathProvider.future);
  final json = await rust_api.getBookChapters(dbPath: dbPath, bookId: bookId);
  final List<dynamic> list = jsonDecode(json);
  return list.cast<Map<String, dynamic>>();
});

Future<ThemeMode> loadThemeModeFromDisk({String? directory}) async {
  try {
    final dir = directory ?? (Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path);
    final file = File('$dir/settings.json');
    if (!await file.exists()) return ThemeMode.system;
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    return ThemeMode.values[json['themeMode'] as int? ?? 0];
  } catch (e) {
    return ThemeMode.system;
  }
}

Future<void> saveThemeModeToDisk(ThemeMode mode, {String? directory}) async {
  try {
    final dir = directory ?? (Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path);
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['themeMode'] = mode.index;
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
    debugPrint('Failed to save theme mode: $e');
  }
}

Future<void> savePendingRoute(String route, {String? directory}) async {
  try {
    final dir = directory ?? (Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path);
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['pendingRoute'] = route;
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
  }
}

Future<String?> loadPendingRoute({String? directory}) async {
  try {
    final dir = directory ?? (Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path);
    final file = File('$dir/settings.json');
    if (!await file.exists()) {
      return null;
    }
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final route = json['pendingRoute'] as String?;
    return route;
  } catch (e) {
    return null;
  }
}

Future<void> clearPendingRoute({String? directory}) async {
  try {
    final dir = directory ?? (Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path);
    final file = File('$dir/settings.json');
    if (await file.exists()) {
      final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
      json.remove('pendingRoute');
      await file.writeAsString(jsonEncode(json));
    } else {
    }
  } catch (e) {
  }
}

Future<double> loadFontSizeFromDisk() async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    if (!await file.exists()) return 18.0;
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final v = json['fontSize'];
    if (v is num) return v.toDouble().clamp(14.0, 28.0);
    return 18.0;
  } catch (e) {
    return 18.0;
  }
}

Future<void> saveFontSizeToDisk(double fontSize) async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['fontSize'] = fontSize;
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
    debugPrint('Failed to save font size: $e');
  }
}

Future<List<String>> loadSearchHistoryFromDisk() async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    if (!await file.exists()) return [];
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final list = json['searchHistory'];
    if (list is List) return list.map((e) => e.toString()).toList();
    return [];
  } catch (e) {
    return [];
  }
}

Future<void> saveSearchHistoryToDisk(List<String> history) async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['searchHistory'] = history;
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
    debugPrint('Failed to save search history: $e');
  }
}
enum ReaderPageMode {
  continuousScroll,
  tapChapter,
  page,
}

class ReaderSettings {
  final double fontSize;
  final int fontWeightIndex;
  final String? fontFamily;
  final int textColor;
  final int backgroundColor;
  final String? backgroundImagePath;
  final double letterSpacing;
  final double lineHeight;
  final double paragraphSpacing;
  final double horizontalPadding;
  final double verticalPadding;
  final String paragraphIndent;
  final ReaderPageMode pageMode;
  final int pageAnim;
  final bool nightMode;
  final int nightBackgroundColor;
  final int nightTextColor;
  final bool showReadingInfo;
  final bool showChapterTitle;
  final bool showClock;
  final bool showProgress;
  final double ttsSpeed;

  const ReaderSettings({
    this.fontSize = 18.0,
    this.fontWeightIndex = 1,
    this.fontFamily,
    this.textColor = 0xFF3E3D3B,
    this.backgroundColor = 0xFFEEEEEE,
    this.backgroundImagePath,
    this.letterSpacing = 0.1,
    this.lineHeight = 1.8,
    this.paragraphSpacing = 8.0,
    this.horizontalPadding = 16.0,
    this.verticalPadding = 16.0,
    this.paragraphIndent = '\u3000\u3000',
    this.pageMode = ReaderPageMode.continuousScroll,
    this.pageAnim = 0,
    this.nightMode = false,
    this.nightBackgroundColor = 0xFF1A1A1A,
    this.nightTextColor = 0xFFADADAD,
    this.showReadingInfo = true,
    this.showChapterTitle = true,
    this.showClock = true,
    this.showProgress = true,
    this.ttsSpeed = 0.5,
  });

  static const List<int> fontWeightValues = [400, 700, 900];
  static const List<Color> presetColors = [
    Color(0xFFF5ECD7),
    Color(0xFFFFF8E7),
    Color(0xFFC8E6C9),
    Color(0xFF212121),
    Color(0xFF000000),
  ];

  int get effectiveBackgroundColor => nightMode ? nightBackgroundColor : backgroundColor;
  int get effectiveTextColor => nightMode ? nightTextColor : textColor;

  ReaderSettings copyWith({
    double? fontSize,
    int? fontWeightIndex,
    String? fontFamily,
    int? textColor,
    int? backgroundColor,
    String? backgroundImagePath,
    double? letterSpacing,
    double? lineHeight,
    double? paragraphSpacing,
    double? horizontalPadding,
    double? verticalPadding,
    String? paragraphIndent,
    ReaderPageMode? pageMode,
    int? pageAnim,
    bool? nightMode,
    int? nightBackgroundColor,
    int? nightTextColor,
    bool? showReadingInfo,
    bool? showChapterTitle,
    bool? showClock,
    bool? showProgress,
    double? ttsSpeed,
  }) {
    return ReaderSettings(
      fontSize: fontSize ?? this.fontSize,
      fontWeightIndex: fontWeightIndex ?? this.fontWeightIndex,
      fontFamily: fontFamily ?? this.fontFamily,
      textColor: textColor ?? this.textColor,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      backgroundImagePath: backgroundImagePath ?? this.backgroundImagePath,
      letterSpacing: letterSpacing ?? this.letterSpacing,
      lineHeight: lineHeight ?? this.lineHeight,
      paragraphSpacing: paragraphSpacing ?? this.paragraphSpacing,
      horizontalPadding: horizontalPadding ?? this.horizontalPadding,
      verticalPadding: verticalPadding ?? this.verticalPadding,
      paragraphIndent: paragraphIndent ?? this.paragraphIndent,
      pageMode: pageMode ?? this.pageMode,
      pageAnim: pageAnim ?? this.pageAnim,
      nightMode: nightMode ?? this.nightMode,
      nightBackgroundColor: nightBackgroundColor ?? this.nightBackgroundColor,
      nightTextColor: nightTextColor ?? this.nightTextColor,
      showReadingInfo: showReadingInfo ?? this.showReadingInfo,
      showChapterTitle: showChapterTitle ?? this.showChapterTitle,
      showClock: showClock ?? this.showClock,
      showProgress: showProgress ?? this.showProgress,
      ttsSpeed: ttsSpeed ?? this.ttsSpeed,
    );
  }

  int get fontWeight =>
      fontWeightIndex >= 0 && fontWeightIndex < fontWeightValues.length
          ? fontWeightValues[fontWeightIndex]
          : 400;

  Map<String, dynamic> toJson() => {
        'fontSize': fontSize,
        'fontWeightIndex': fontWeightIndex,
        'fontFamily': fontFamily,
        'textColor': textColor,
        'backgroundColor': backgroundColor,
        'backgroundImagePath': backgroundImagePath,
        'letterSpacing': letterSpacing,
        'lineHeight': lineHeight,
        'paragraphSpacing': paragraphSpacing,
        'horizontalPadding': horizontalPadding,
        'verticalPadding': verticalPadding,
        'paragraphIndent': paragraphIndent,
        'pageMode': pageMode.index,
        'pageAnim': pageAnim,
        'nightMode': nightMode,
        'nightBackgroundColor': nightBackgroundColor,
        'nightTextColor': nightTextColor,
        'showReadingInfo': showReadingInfo,
        'showChapterTitle': showChapterTitle,
        'showClock': showClock,
        'showProgress': showProgress,
        'ttsSpeed': ttsSpeed,
      };

  factory ReaderSettings.fromJson(Map<String, dynamic> json) {
    return ReaderSettings(
      fontSize: (json['fontSize'] as num?)?.toDouble() ?? 18.0,
      fontWeightIndex: json['fontWeightIndex'] as int? ?? 1,
      fontFamily: json['fontFamily'] as String?,
      textColor: json['textColor'] as int? ?? 0xFF3E3D3B,
      backgroundColor: json['backgroundColor'] as int? ?? 0xFFEEEEEE,
      backgroundImagePath: json['backgroundImagePath'] as String?,
      letterSpacing: (json['letterSpacing'] as num?)?.toDouble() ?? 0.1,
      lineHeight: (json['lineHeight'] as num?)?.toDouble() ?? 1.8,
      paragraphSpacing: (json['paragraphSpacing'] as num?)?.toDouble() ?? 8.0,
      horizontalPadding: (json['horizontalPadding'] as num?)?.toDouble() ?? 16.0,
      verticalPadding: (json['verticalPadding'] as num?)?.toDouble() ?? 16.0,
      paragraphIndent: json['paragraphIndent'] as String? ?? '\u3000\u3000',
      pageMode: ReaderPageMode.values[(json['pageMode'] as int? ?? 0).clamp(0, ReaderPageMode.values.length - 1)],
      pageAnim: json['pageAnim'] as int? ?? 0,
      nightMode: json['nightMode'] as bool? ?? false,
      nightBackgroundColor: json['nightBackgroundColor'] as int? ?? 0xFF1A1A1A,
      nightTextColor: json['nightTextColor'] as int? ?? 0xFFADADAD,
      showReadingInfo: json['showReadingInfo'] as bool? ?? true,
      showChapterTitle: json['showChapterTitle'] as bool? ?? true,
      showClock: json['showClock'] as bool? ?? true,
      showProgress: json['showProgress'] as bool? ?? true,
      ttsSpeed: (json['ttsSpeed'] as num?)?.toDouble() ?? 0.5,
    );
  }
}

final readerSettingsProvider = StateProvider<ReaderSettings>((ref) => const ReaderSettings());

Future<ReaderSettings> loadReaderSettingsFromDisk() async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    if (!await file.exists()) return const ReaderSettings();
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final settingsJson = json['readerSettings'];
    if (settingsJson is Map<String, dynamic>) {
      return ReaderSettings.fromJson(settingsJson);
    }
    return const ReaderSettings();
  } catch (e) {
    return const ReaderSettings();
  }
}

Future<void> saveReaderSettingsToDisk(ReaderSettings settings) async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['readerSettings'] = settings.toJson();
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
    debugPrint('Failed to save reader settings: $e');
  }
}

Future<bool> loadBookshelfGridViewFromDisk() async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    if (!await file.exists()) return false;
    final json = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final v = json['bookshelfGridView'];
    if (v is bool) return v;
    return false;
  } catch (e) {
    return false;
  }
}

Future<void> saveBookshelfGridViewToDisk(bool isGridView) async {
  try {
    final dir = Platform.isAndroid
        ? (await getApplicationDocumentsDirectory()).path
        : (await getApplicationSupportDirectory()).path;
    final file = File('$dir/settings.json');
    final Map<String, dynamic> data = file.existsSync()
        ? jsonDecode(await file.readAsString()) as Map<String, dynamic>
        : {};
    data['bookshelfGridView'] = isGridView;
    await file.writeAsString(jsonEncode(data));
  } catch (e) {
    debugPrint('Failed to save bookshelf grid view: $e');
  }
}
