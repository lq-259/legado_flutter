class FailedSource {
  final String sourceId;
  final String sourceName;
  final String error;

  FailedSource({
    required this.sourceId,
    required this.sourceName,
    required this.error,
  });

  factory FailedSource.fromJson(Map<String, dynamic> json) => FailedSource(
        sourceId: json['source_id'] as String? ?? '',
        sourceName: json['source_name'] as String? ?? '',
        error: json['error'] as String? ?? '',
      );
}

class SearchResponse {
  final List<Map<String, dynamic>> items;
  final List<FailedSource> failedSources;

  SearchResponse({required this.items, required this.failedSources});

  factory SearchResponse.fromJson(Map<String, dynamic> json) => SearchResponse(
        items: (json['items'] as List)
            .map((e) => Map<String, dynamic>.from(e as Map))
            .toList(),
        failedSources: (json['failed_sources'] as List)
            .map((e) => FailedSource.fromJson(e as Map<String, dynamic>))
            .toList(),
      );
}

class AddBookRequest {
  final String sourceId;
  final String? sourceName;
  final String name;
  final String? author;
  final String? coverUrl;
  final String bookUrl;

  AddBookRequest({
    required this.sourceId,
    this.sourceName,
    required this.name,
    this.author,
    this.coverUrl,
    required this.bookUrl,
  });

  Map<String, dynamic> toJson() => {
        'source_id': sourceId,
        'source_name': sourceName,
        'name': name,
        'author': author,
        'cover_url': coverUrl,
        'book_url': bookUrl,
      };
}

class AddBookResponse {
  final String bookId;
  final int chapterCount;

  AddBookResponse({required this.bookId, required this.chapterCount});

  factory AddBookResponse.fromJson(Map<String, dynamic> json) =>
      AddBookResponse(
        bookId: json['book_id'] as String? ?? '',
        chapterCount: json['chapter_count'] as int? ?? 0,
      );
}

class ChapterContentResponse {
  final String bookId;
  final int chapterIndex;
  final String title;
  final String content;
  final PlatformRequest? platformRequest;

  ChapterContentResponse({
    required this.bookId,
    required this.chapterIndex,
    required this.title,
    required this.content,
    this.platformRequest,
  });

  factory ChapterContentResponse.fromJson(Map<String, dynamic> json) =>
      ChapterContentResponse(
        bookId: json['book_id'] as String? ?? '',
        chapterIndex: json['chapter_index'] as int? ?? 0,
        title: json['title'] as String? ?? '',
        content: json['content'] as String? ?? '',
        platformRequest: PlatformRequest.fromJsonOrNull(
          json['platform_request'],
        ),
      );
}

class PlatformRequest {
  final String type;
  final String? url;
  final String? contentRule;
  final String? webJs;
  final String? sourceRegex;
  final Map<String, String> headers;
  final String? userAgent;

  PlatformRequest({
    required this.type,
    this.url,
    this.contentRule,
    this.webJs,
    this.sourceRegex,
    this.headers = const {},
    this.userAgent,
  });

  factory PlatformRequest.fromJson(Map<String, dynamic> json) => PlatformRequest(
        type: json['type'] as String? ?? '',
        url: json['url'] as String?,
        contentRule: json['content_rule'] as String?,
        webJs: json['web_js'] as String?,
        sourceRegex: json['source_regex'] as String?,
        headers: _stringMap(json['headers']),
        userAgent: json['user_agent'] as String?,
      );

  static PlatformRequest? fromJsonOrNull(Object? value) {
    if (value is Map<String, dynamic>) {
      return PlatformRequest.fromJson(value);
    }
    if (value is Map) {
      return PlatformRequest.fromJson(Map<String, dynamic>.from(value));
    }
    return null;
  }

  static Map<String, String> _stringMap(Object? value) {
    if (value is! Map) return const {};
    return value.map((key, val) => MapEntry(key.toString(), val.toString()));
  }
}
