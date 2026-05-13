import 'dto.dart';
import 'api_client.dart';

class BookshelfApi {
  final ApiClient _client;

  BookshelfApi(this._client);

  Future<List<Map<String, dynamic>>> list() async {
    final r = await _client.get('/api/bookshelf');
    return (r.data as List).cast<Map<String, dynamic>>();
  }

  Future<AddBookResponse> addBook(AddBookRequest req) async {
    final r = await _client.post('/api/bookshelf', data: req.toJson());
    return AddBookResponse.fromJson(r.data as Map<String, dynamic>);
  }

  Future<void> delete(String bookId) async {
    await _client.delete('/api/bookshelf/$bookId');
  }
}
