import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:webview_flutter/webview_flutter.dart';

import 'api/dto.dart';

class WebViewExecutionResult {
  final String content;
  final String? resourceUrl;
  final bool sourceRegexRequired;
  final bool sourceRegexMatched;

  const WebViewExecutionResult({
    required this.content,
    this.resourceUrl,
    this.sourceRegexRequired = false,
    this.sourceRegexMatched = false,
  });
}

class PlatformWebViewExecutor {
  static const MethodChannel _channel = MethodChannel('legado/webview_executor');

  Future<WebViewExecutionResult> execute(
    BuildContext context,
    PlatformRequest request,
  ) async {
    if (request.type != 'web_view_content') {
      throw UnsupportedError('Unsupported platform request: ${request.type}');
    }
    final url = request.url;
    if (url == null || url.isEmpty) {
      throw ArgumentError('WebView request missing url');
    }
    final nativeResult = await _executeNative(request);
    if (nativeResult != null) {
      return nativeResult;
    }
    final result = await Navigator.of(context).push<WebViewExecutionResult>(
      MaterialPageRoute(
        fullscreenDialog: true,
        builder: (_) => _WebViewExecutionPage(request: request),
      ),
    );
    if (result == null) {
      throw StateError('WebView execution was cancelled');
    }
    return result;
  }

  Future<WebViewExecutionResult?> _executeNative(PlatformRequest request) async {
    try {
      final result = await _channel.invokeMapMethod<String, dynamic>('execute', {
        'url': request.url,
        'webJs': request.webJs,
        'sourceRegex': request.sourceRegex,
        'headers': request.headers,
        'userAgent': request.userAgent,
        'timeoutMs': 30000,
      });
      if (result == null) return null;
      return WebViewExecutionResult(
        content: result['content'] as String? ?? '',
        resourceUrl: result['resourceUrl'] as String?,
        sourceRegexRequired: request.sourceRegex?.trim().isNotEmpty == true,
        sourceRegexMatched: result['sourceRegexMatched'] as bool? ?? false,
      );
    } on MissingPluginException {
      return null;
    } on PlatformException {
      return null;
    }
  }
}

class _WebViewExecutionPage extends StatefulWidget {
  final PlatformRequest request;

  const _WebViewExecutionPage({required this.request});

  @override
  State<_WebViewExecutionPage> createState() => _WebViewExecutionPageState();
}

class _WebViewExecutionPageState extends State<_WebViewExecutionPage> {
  late final WebViewController _controller;
  var _isExecuting = false;
  var _progress = 0;
  String? _error;

  @override
  void initState() {
    super.initState();
    _controller = WebViewController()
      ..setJavaScriptMode(JavaScriptMode.unrestricted)
      ..setNavigationDelegate(
        NavigationDelegate(
          onProgress: (progress) {
            if (mounted) setState(() => _progress = progress);
          },
          onPageFinished: (_) => _executeRules(),
          onWebResourceError: (error) {
            if (mounted) {
              setState(() => _error = error.description);
            }
          },
        ),
      )
      ..loadRequest(Uri.parse(widget.request.url!));
  }

  Future<void> _executeRules() async {
    if (_isExecuting) return;
    _isExecuting = true;
    try {
      final webJs = widget.request.webJs?.trim();
      final Object? rawResult;
      if (webJs != null && webJs.isNotEmpty) {
        rawResult = await _controller.runJavaScriptReturningResult(
          _wrapJavaScript(webJs),
        );
      } else {
        rawResult = await _controller.runJavaScriptReturningResult(
          'document.documentElement.outerHTML',
        );
      }
      final content = _normalizeJsResult(rawResult);
      if (!mounted) return;
      Navigator.of(context).pop(
        WebViewExecutionResult(
          content: content,
          sourceRegexRequired: widget.request.sourceRegex?.trim().isNotEmpty == true,
        ),
      );
    } catch (e) {
      if (mounted) {
        setState(() => _error = e.toString());
      }
    }
  }

  String _wrapJavaScript(String script) {
    return '''
(function() {
  var result = '';
  var src = document.documentElement.outerHTML;
  var baseUrl = location.href;
  var out = (function(){
$script
  })();
  if (out === undefined || out === null) return '';
  if (typeof out === 'string') return out;
  return String(out);
})()
''';
  }

  String _normalizeJsResult(Object? value) {
    if (value == null) return '';
    final text = value.toString();
    if (text.length >= 2 &&
        ((text.startsWith('"') && text.endsWith('"')) ||
            (text.startsWith("'") && text.endsWith("'")))) {
      return text.substring(1, text.length - 1);
    }
    return text;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('WebView 解析'),
        actions: [
          TextButton(
            onPressed: _executeRules,
            child: const Text('执行'),
          ),
        ],
      ),
      body: Column(
        children: [
          if (_progress < 100) LinearProgressIndicator(value: _progress / 100),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.all(12),
              child: Text(
                'WebView 执行失败: $_error',
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          Expanded(child: WebViewWidget(controller: _controller)),
        ],
      ),
    );
  }
}
