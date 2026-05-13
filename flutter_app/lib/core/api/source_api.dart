import 'api_client.dart';

class SourceApi {
  final ApiClient _client;

  SourceApi(this._client);

  Future<List<Map<String, dynamic>>> list() async {
    final r = await _client.get('/api/sources');
    return (r.data as List).cast<Map<String, dynamic>>();
  }

  Future<List<Map<String, dynamic>>> listEnabled() async {
    final r = await _client.get('/api/sources/enabled');
    return (r.data as List).cast<Map<String, dynamic>>();
  }

  Future<Map<String, dynamic>> create(String name, String url) async {
    final r = await _client.post('/api/sources', data: {
      'name': name,
      'url': url,
    });
    return r.data as Map<String, dynamic>;
  }

  Future<int> import(String json) async {
    final r = await _client.post('/api/sources/import', data: {
      'json': json,
    });
    return (r.data['count'] as int?) ?? 0;
  }

  Future<void> setEnabled(String id, bool enabled) async {
    await _client.put('/api/sources/$id/enabled', data: {
      'enabled': enabled,
    });
  }

  Future<void> delete(String id) async {
    await _client.delete('/api/sources/$id');
  }
}
