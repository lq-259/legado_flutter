import 'dto.dart';
import 'api_client.dart';

class SearchApi {
  final ApiClient _client;

  SearchApi(this._client);

  Future<SearchResponse> search(String keyword,
      {List<String>? sourceIds}) async {
    final r = await _client.post('/api/search', data: {
      'keyword': keyword,
      'source_ids': sourceIds,
    });
    return SearchResponse.fromJson(r.data as Map<String, dynamic>);
  }
}
