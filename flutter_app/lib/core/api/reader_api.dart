import 'dto.dart';
import 'api_client.dart';

class ReaderApi {
  final ApiClient _client;

  ReaderApi(this._client);

  Future<List<Map<String, dynamic>>> getChapters(String bookId) async {
    final r = await _client.get('/api/books/$bookId/chapters');
    return (r.data as List).cast<Map<String, dynamic>>();
  }

  Future<ChapterContentResponse> getChapterContent(
      String bookId, int chapterIndex) async {
    final r = await _client.get(
      '/api/books/$bookId/chapters/content?chapter_index=$chapterIndex',
    );
    return ChapterContentResponse.fromJson(r.data as Map<String, dynamic>);
  }

  Future<void> saveChapterContent(
      String bookId, int chapterIndex, String content) async {
    await _client.post('/api/books/$bookId/chapters/content/save', data: {
      'chapter_index': chapterIndex,
      'content': content,
    });
  }

  Future<Map<String, dynamic>?> getProgress(String bookId) async {
    final r = await _client.get('/api/books/$bookId/progress');
    return r.data as Map<String, dynamic>?;
  }

  Future<void> saveProgress(
      String bookId, int chapterIndex, int paragraphIndex, int offset) async {
    await _client.put('/api/books/$bookId/progress', data: {
      'chapter_index': chapterIndex,
      'paragraph_index': paragraphIndex,
      'offset': offset,
    });
  }
}
