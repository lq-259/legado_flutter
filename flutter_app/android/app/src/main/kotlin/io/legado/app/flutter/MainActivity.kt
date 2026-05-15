package io.legado.app.flutter

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.util.Base64
import android.net.Uri
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.ValueCallback
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.nio.charset.Charset
import javax.crypto.Cipher
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec
import java.io.File
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.CopyOnWriteArrayList
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream

class MainActivity : FlutterActivity() {
    companion object {
        const val DOWNLOAD_CHANNEL_ID = "legado_download"
        const val DOWNLOAD_CHANNEL_NAME = "下载通知"
        const val CHANNEL_NAME = "legado/notifications"
        const val WEBVIEW_CHANNEL_NAME = "legado/webview_executor"
        const val NOTIFICATION_PERMISSION_REQUEST_CODE = 1001
        const val MAX_BRIDGE_BODY_BYTES = 10 * 1024 * 1024L
        const val MAX_ZIP_DOWNLOAD_BYTES = 50 * 1024 * 1024L
        const val MAX_ZIP_ENTRY_BYTES = 10 * 1024 * 1024L
        const val MAX_ZIP_TOTAL_BYTES = 50 * 1024 * 1024L
        const val MAX_ZIP_ENTRIES = 1024

        private fun isAllowedWebViewUrl(url: String): Boolean {
            return try {
                val uri = Uri.parse(url)
                uri.scheme == "https" || uri.scheme == "http"
            } catch (_: Exception) {
                false
            }
        }
    }

    private var pendingNotificationResult: MethodChannel.Result? = null
    private var waitingForSettingsReturn = false

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL_NAME).setMethodCallHandler { call, result ->
            when (call.method) {
                "hasPermission" -> result.success(hasNotificationPermission())
                "requestPermission" -> requestNotificationPermission(result)
                "openNotificationSettings" -> {
                    openAppNotificationSettings()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, WEBVIEW_CHANNEL_NAME).setMethodCallHandler { call, result ->
            when (call.method) {
                "execute" -> executeWebViewRequest(call.arguments as? Map<*, *>, result)
                else -> result.notImplemented()
            }
        }
    }

    private fun executeWebViewRequest(args: Map<*, *>?, result: MethodChannel.Result) {
        val url = args?.get("url") as? String
        if (url.isNullOrBlank()) {
            result.error("INVALID_ARGUMENT", "url is required", null)
            return
        }
        if (!isAllowedWebViewUrl(url)) {
            result.error("INVALID_URL", "Only http(s) WebView URLs are allowed", null)
            return
        }
        val webJs = args["webJs"] as? String
        val sourceRegex = args["sourceRegex"] as? String
        val headers = (args["headers"] as? Map<*, *>)
            ?.mapNotNull { (key, value) ->
                val k = key?.toString() ?: return@mapNotNull null
                val v = value?.toString() ?: return@mapNotNull null
                k to v
            }
            ?.toMap()
            ?: emptyMap()
        val userAgent = args["userAgent"] as? String
        val timeoutMs = (args["timeoutMs"] as? Number)?.toLong() ?: 30000L

        runOnUiThread {
            val webView = WebView(this)
            var completed = false
            val matchedResources = CopyOnWriteArrayList<String>()
            val bridgeRoot = File(cacheDir, "legado_webview").apply { mkdirs() }
            val compiledRegex = try {
                sourceRegex?.takeIf { it.isNotBlank() && it.length <= 1024 }?.let { Regex(it) }
            } catch (_: Exception) {
                null
            }
            val finish = { payload: Map<String, Any?> ->
                if (!completed) {
                    completed = true
                    try {
                        webView.stopLoading()
                        webView.destroy()
                    } catch (_: Exception) {
                    }
                    result.success(payload)
                }
            }
            fun evaluateAndFinish(payloadExtra: Map<String, Any?> = emptyMap()) {
                if (completed) return
                val script = wrapWebJs(webJs)
                webView.evaluateJavascript(script, ValueCallback { value ->
                    val payload = mutableMapOf<String, Any?>(
                        "content" to decodeJsString(value),
                        "resourceUrl" to matchedResources.firstOrNull(),
                        "sourceRegexMatched" to matchedResources.isNotEmpty(),
                    )
                    payload.putAll(payloadExtra)
                    finish(payload)
                })
            }

            webView.settings.javaScriptEnabled = true
            webView.settings.domStorageEnabled = true
            webView.settings.allowFileAccess = false
            webView.settings.allowContentAccess = false
            if (!userAgent.isNullOrBlank()) {
                webView.settings.userAgentString = userAgent
            }
            webView.addJavascriptInterface(LegadoJsBridge(headers, bridgeRoot), "legadoNative")
            webView.webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                    val next = request?.url?.toString() ?: return true
                    return !isAllowedWebViewUrl(next)
                }

                override fun shouldInterceptRequest(
                    view: WebView?,
                    request: WebResourceRequest?
                ): WebResourceResponse? {
                    val requestUrl = request?.url?.toString()
                    if (compiledRegex != null && requestUrl != null && matchedResources.isEmpty()) {
                        if (compiledRegex.containsMatchIn(requestUrl)) {
                            matchedResources.add(requestUrl)
                            webView.post { evaluateAndFinish() }
                        }
                    }
                    return super.shouldInterceptRequest(view, request)
                }

                override fun onPageFinished(view: WebView?, finishedUrl: String?) {
                    if (completed) return
                    if (compiledRegex != null && matchedResources.isEmpty()) {
                        webView.postDelayed({ evaluateAndFinish(mapOf("idle" to true)) }, 1500L)
                    } else {
                        evaluateAndFinish()
                    }
                }
            }

            webView.postDelayed({
                if (matchedResources.isNotEmpty()) {
                    evaluateAndFinish(mapOf("timeout" to true))
                } else {
                    finish(
                        mapOf(
                            "content" to "",
                            "resourceUrl" to null,
                            "sourceRegexMatched" to false,
                            "timeout" to true,
                        )
                    )
                }
            }, timeoutMs)
            webView.loadUrl(url, headers)
        }
    }

    private fun wrapWebJs(webJs: String?): String {
        val script = webJs?.takeIf { it.isNotBlank() } ?: "return document.documentElement.outerHTML;"
        return """
            (function() {
              try {
                var result = '';
                var src = document.documentElement.outerHTML;
                var baseUrl = location.href;
                var cache = {
                  getFromMemory: function(key) { return legadoNative.cacheGet(String(key || '')); },
                  putMemory: function(key, value) { return legadoNative.cachePut(String(key || ''), value == null ? '' : String(value)); },
                  get: function(key) { return legadoNative.cacheGet(String(key || '')); },
                  put: function(key, value) { return legadoNative.cachePut(String(key || ''), value == null ? '' : String(value)); }
                };
                var java = {
                  ajax: function(url) { return legadoNative.http('GET', String(url || ''), '', '{}'); },
                  connect: function(url) { var body = legadoNative.http('GET', String(url || ''), '', '{}'); return { body: function(){ return body; }, toString: function(){ return body; } }; },
                  get: function(url, headers) { var body = legadoNative.http('GET', String(url || ''), '', JSON.stringify(headers || {})); return { body: function(){ return body; }, toString: function(){ return body; } }; },
                  post: function(url, body, headers) { var response = legadoNative.http('POST', String(url || ''), String(body || ''), JSON.stringify(headers || {})); return { body: function(){ return response; }, toString: function(){ return response; } }; },
                  getCookie: function(tag, key) { return legadoNative.getCookie(String(tag || ''), key == null ? '' : String(key)); },
                  log: function(msg) { legadoNative.log(String(msg || '')); return ''; },
                  base64Encode: function(str) { return legadoNative.base64Encode(String(str || '')); },
                  base64Decode: function(str) { return legadoNative.base64Decode(String(str || '')); },
                  base64DecodeToByteArray: function(str) { return JSON.parse(legadoNative.base64DecodeToByteArray(String(str || ''))); },
                  md5Encode: function(str) { return legadoNative.md5Encode(String(str || '')); },
                  md5Encode16: function(str) { return legadoNative.md5Encode16(String(str || '')); },
                  encodeURI: function(str) { return legadoNative.encodeURIComponentCompat(String(str || '')); },
                  encodeURIComponent: function(str) { return legadoNative.encodeURIComponentCompat(String(str || '')); },
                  decodeURI: function(str) { return legadoNative.decodeURIComponentCompat(String(str || '')); },
                  decodeURIComponent: function(str) { return legadoNative.decodeURIComponentCompat(String(str || '')); },
                  timeFormat: function(value) { return legadoNative.timeFormat(value == null ? '' : String(value)); },
                   htmlFormat: function(value) { return legadoNative.htmlFormat(value == null ? '' : String(value)); },
                   queryBase64Ttf: function(base64) { return legadoNative.queryBase64Ttf(String(base64 || '')); },
                   queryTtf: function(input) { return legadoNative.queryTtf(String(input || '')); },
                   replaceFont: function(text, font1Json, font2Json) { return legadoNative.replaceFont(String(text || ''), String(font1Json || ''), String(font2Json || '')); },
                   setContent: function(content, baseUrl) { return legadoNative.setContent(String(content || ''), String(baseUrl || '')); },
                   getString: function(rule, isUrl) { return legadoNative.getString(String(rule || ''), isUrl || false); },
                   getStringList: function(rule, isUrl) { return legadoNative.getStringList(String(rule || ''), isUrl || false); },
                   getElements: function(rule) { return JSON.parse(legadoNative.getElements(String(rule || ''))); },
                   utf8ToGbk: function(str) { return legadoNative.utf8ToGbk(String(str || '')); },
                   getFile: function(path) { return legadoNative.getFile(String(path || '')); },
                   readFile: function(path) { return JSON.parse(legadoNative.readFile(String(path || ''))); },
                   readTxtFile: function(path, charset) { return legadoNative.readTxtFile(String(path || ''), charset == null ? '' : String(charset)); },
                   downloadFile: function(url, path) { return legadoNative.downloadFile(String(url || ''), String(path || '')); },
                   deleteFile: function(path) { return legadoNative.deleteFile(String(path || '')); },
                   unzipFile: function(zipPath, destDir) { return legadoNative.unzipFile(String(zipPath || ''), String(destDir || '')); },
                   getTxtInFolder: function(dirPath) { return legadoNative.getTxtInFolder(String(dirPath || '')); },
                   getZipStringContent: function(url, path) { return legadoNative.getZipStringContent(String(url || ''), String(path || '')); },
                   getZipByteArrayContent: function(url, path) { return JSON.parse(legadoNative.getZipByteArrayContent(String(url || ''), String(path || ''))); },
                   getFromMemory: function(key) { return legadoNative.cacheGet(String(key || '')); },
                  putMemory: function(key, value) { return legadoNative.cachePut(String(key || ''), value == null ? '' : String(value)); }
                };
                var out = (function(){
                  $script
                })();
                if (out === undefined || out === null) return '';
                if (typeof out === 'string') return out;
                return String(out);
              } catch (e) {
                return 'WEBVIEW_JS_ERROR: ' + e.message;
              }
            })();
        """.trimIndent()
    }

    class LegadoJsBridge(private val defaultHeaders: Map<String, String>, private val fileRoot: File) {
        private val memory = mutableMapOf<String, String>()
        private var storedContent = ""
        private var storedBaseUrl = ""

        @JavascriptInterface
        fun http(method: String, url: String, body: String, headersJson: String): String {
            return try {
                if (!isAllowedWebViewUrl(url)) return ""
                val conn = URL(url).openConnection() as HttpURLConnection
                conn.requestMethod = method.uppercase()
                conn.connectTimeout = 15000
                conn.readTimeout = 30000
                if (conn.contentLengthLong > MAX_BRIDGE_BODY_BYTES) return ""
                for ((key, value) in defaultHeaders) {
                    conn.setRequestProperty(key, value)
                }
                parseHeaders(headersJson).forEach { (key, value) ->
                    conn.setRequestProperty(key, value)
                }
                if (conn.requestMethod == "POST") {
                    conn.doOutput = true
                    val bytes = body.toByteArray(Charsets.UTF_8)
                    conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
                    conn.outputStream.use { it.write(bytes) }
                }
                val stream = if (conn.responseCode >= 400) conn.errorStream else conn.inputStream
                stream?.use { String(readLimitedBytes(it, MAX_BRIDGE_BODY_BYTES), Charsets.UTF_8) } ?: ""
            } catch (e: Exception) {
                ""
            }
        }

        @JavascriptInterface
        fun getCookie(tag: String, key: String): String {
            val cookie = CookieManager.getInstance().getCookie(tag) ?: return ""
            if (key.isBlank()) return cookie
            return cookie.split(';')
                .map { it.trim() }
                .firstOrNull { it.substringBefore('=') == key }
                ?.substringAfter('=', "")
                ?: ""
        }

        @JavascriptInterface
        fun log(message: String) {
            android.util.Log.d("LegadoWebView", message)
        }

        @JavascriptInterface
        fun cacheGet(key: String): String = memory[key] ?: ""

        @JavascriptInterface
        fun cachePut(key: String, value: String): String {
            memory[key] = value
            return value
        }

        @JavascriptInterface
        fun base64Encode(value: String): String = Base64.encodeToString(value.toByteArray(Charsets.UTF_8), Base64.NO_WRAP)

        @JavascriptInterface
        fun base64Decode(value: String): String = try {
            String(Base64.decode(value, Base64.DEFAULT), Charsets.UTF_8)
        } catch (_: Exception) {
            ""
        }

        @JavascriptInterface
        fun base64DecodeToByteArray(value: String): String = try {
            Base64.decode(value, Base64.DEFAULT).joinToString(prefix = "[", postfix = "]") { byte ->
                (byte.toInt() and 0xff).toString()
            }
        } catch (_: Exception) {
            "[]"
        }

        @JavascriptInterface
        fun md5Encode(value: String): String = digestHex("MD5", value).lowercase(Locale.ROOT)

        @JavascriptInterface
        fun md5Encode16(value: String): String = md5Encode(value).substring(8, 24)

        @JavascriptInterface
        fun encodeURIComponentCompat(value: String): String = try {
            URLEncoder.encode(value, "UTF-8").replace("+", "%20")
        } catch (_: Exception) {
            value
        }

        @JavascriptInterface
        fun decodeURIComponentCompat(value: String): String = try {
            URLDecoder.decode(value, "UTF-8")
        } catch (_: Exception) {
            value
        }

        @JavascriptInterface
        fun timeFormat(value: String): String {
            val timestamp = value.trim().toLongOrNull() ?: return value.trim()
            val millis = if (kotlin.math.abs(timestamp) >= 1_000_000_000_000L || kotlin.math.abs(timestamp) < 1_000_000_000L) {
                timestamp
            } else {
                timestamp * 1000
            }
            return try {
                SimpleDateFormat("yyyy/MM/dd HH:mm", Locale.ROOT).format(Date(millis))
            } catch (_: Exception) {
                ""
            }
        }

        @JavascriptInterface
        fun htmlFormat(value: String): String {
            var out = value
                .replace("&nbsp;", " ")
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&apos;", "'")
            out = Regex("(?i)<br\\s*/?>").replace(out, "\n")
            out = Regex("(?i)</p\\s*>").replace(out, "\n")
            out = Regex("(?is)<script.*?</script>").replace(out, "")
            out = Regex("(?is)<style.*?</style>").replace(out, "")
            out = Regex("(?is)<[^>]+>").replace(out, "")
            return out.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.joinToString("\n")
        }

        @JavascriptInterface
        fun aesDecodeToString(data: String, key: String, transformation: String, iv: String): String = try {
            String(aesCrypt(Cipher.DECRYPT_MODE, hexToBytes(data), key, transformation, iv), Charsets.UTF_8)
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun aesBase64DecodeToString(data: String, key: String, transformation: String, iv: String): String = try {
            String(aesCrypt(Cipher.DECRYPT_MODE, Base64.decode(data, Base64.DEFAULT), key, transformation, iv), Charsets.UTF_8)
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun aesEncodeToString(data: String, key: String, transformation: String, iv: String): String = try {
            bytesToHex(aesCrypt(Cipher.ENCRYPT_MODE, data.toByteArray(Charsets.UTF_8), key, transformation, iv))
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun aesEncodeToBase64String(data: String, key: String, transformation: String, iv: String): String = try {
            Base64.encodeToString(aesCrypt(Cipher.ENCRYPT_MODE, data.toByteArray(Charsets.UTF_8), key, transformation, iv), Base64.NO_WRAP)
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun aesDecodeToByteArray(data: String, key: String, transformation: String, iv: String): String = try {
            val result = aesCrypt(Cipher.DECRYPT_MODE, hexToBytes(data), key, transformation, iv)
            result.joinToString(prefix = "[", postfix = "]") { (it.toInt() and 0xff).toString() }
        } catch (_: Exception) { "[]" }

        @JavascriptInterface
        fun aesBase64DecodeToByteArray(data: String, key: String, transformation: String, iv: String): String = try {
            val result = aesCrypt(Cipher.DECRYPT_MODE, Base64.decode(data, Base64.DEFAULT), key, transformation, iv)
            result.joinToString(prefix = "[", postfix = "]") { (it.toInt() and 0xff).toString() }
        } catch (_: Exception) { "[]" }

        @JavascriptInterface
        fun aesEncodeToByteArray(data: String, key: String, transformation: String, iv: String): String = try {
            val result = aesCrypt(Cipher.ENCRYPT_MODE, data.toByteArray(Charsets.UTF_8), key, transformation, iv)
            result.joinToString(prefix = "[", postfix = "]") { (it.toInt() and 0xff).toString() }
        } catch (_: Exception) { "[]" }

        @JavascriptInterface
        fun aesEncodeToBase64ByteArray(data: String, key: String, transformation: String, iv: String): String = try {
            val result = aesCrypt(Cipher.ENCRYPT_MODE, data.toByteArray(Charsets.UTF_8), key, transformation, iv)
            Base64.encodeToString(result, Base64.NO_WRAP)
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun setContent(content: String, baseUrl: String): String {
            storedContent = content
            storedBaseUrl = baseUrl
            return ""
        }

        @JavascriptInterface
        fun getString(rule: String, isUrl: Boolean): String {
            if (rule.isBlank() || storedContent.isBlank()) return ""
            return try {
                val results = evaluateRule(storedContent, rule)
                if (results.isEmpty()) return ""
                if (isUrl) extractAttr(results[0], "href") ?: extractAttr(results[0], "src") ?: results[0]
                else extractText(results[0])
            } catch (_: Exception) { "" }
        }

        @JavascriptInterface
        fun getStringList(rule: String, isUrl: Boolean): String {
            if (rule.isBlank() || storedContent.isBlank()) return "[]"
            return try {
                val results = evaluateRule(storedContent, rule)
                val list = results.map { if (isUrl) extractAttr(it, "href") ?: extractAttr(it, "src") ?: it else extractText(it) }
                org.json.JSONArray(list).toString()
            } catch (_: Exception) { "[]" }
        }

        @JavascriptInterface
        fun getElements(rule: String): String {
            if (rule.isBlank() || storedContent.isBlank()) return "[]"
            return try {
                val results = evaluateRule(storedContent, rule)
                val arr = org.json.JSONArray()
                for (html in results) {
                    val obj = org.json.JSONObject()
                    obj.put("tagName", extractTagName(html) ?: "")
                    obj.put("text", extractText(html))
                    obj.put("ownText", extractOwnText(html))
                    obj.put("html", extractInnerHtml(html))
                    obj.put("outerHtml", html)
                    val attrs = org.json.JSONObject()
                    val attrPattern = Regex("""(\w[\w-]*)\s*=\s*["']([^"']*)["']""")
                    for (match in attrPattern.findAll(html)) {
                        attrs.put(match.groupValues[1], match.groupValues[2])
                    }
                    obj.put("attrs", attrs)
                    obj.put("children", org.json.JSONArray())
                    arr.put(obj)
                }
                arr.toString()
            } catch (_: Exception) { "[]" }
        }

        @JavascriptInterface
        fun utf8ToGbk(str: String): String = try {
            String(str.toByteArray(Charsets.UTF_8), Charset.forName("GBK"))
        } catch (_: Exception) { str }

        @JavascriptInterface
        fun getFile(path: String): String = try {
            resolvePath(path).absolutePath
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun readFile(path: String): String {
            return try {
                val file = resolvePath(path)
                if (!file.exists()) "[]"
                else {
                    if (file.length() > MAX_BRIDGE_BODY_BYTES) return "[]"
                    val bytes = file.readBytes()
                    bytes.joinToString(prefix = "[", postfix = "]") { (it.toInt() and 0xff).toString() }
                }
            } catch (_: Exception) { "[]" }
        }

        @JavascriptInterface
        fun readTxtFile(path: String, charsetName: String): String {
            return try {
                val file = resolvePath(path)
                if (!file.exists()) ""
                else {
                    if (file.length() > MAX_BRIDGE_BODY_BYTES) return ""
                    val bytes = file.readBytes()
                    if (charsetName.isBlank()) String(bytes, Charsets.UTF_8)
                    else String(bytes, Charset.forName(charsetName))
                }
            } catch (_: Exception) { "" }
        }

        @JavascriptInterface
        fun deleteFile(path: String): String = try {
            val file = resolvePath(path)
            if (file.exists()) file.delete()
            "true"
        } catch (_: Exception) { "false" }

        @JavascriptInterface
        fun downloadFile(url: String, path: String): String = try {
            if (!isAllowedWebViewUrl(url)) throw IllegalArgumentException("invalid url")
            val file = resolvePath(path)
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.connectTimeout = 15000
            conn.readTimeout = 30000
            if (conn.contentLengthLong > MAX_BRIDGE_BODY_BYTES) throw IllegalStateException("response too large")
            conn.inputStream.use { input ->
                FileOutputStream(file).use { output ->
                    copyLimited(input, output, MAX_BRIDGE_BODY_BYTES)
                }
            }
            file.absolutePath
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun unzipFile(zipPath: String, destDir: String): String {
            return try {
                val zipFile = resolvePath(zipPath)
                if (!zipFile.exists()) ""
                else if (zipFile.length() > MAX_ZIP_DOWNLOAD_BYTES) ""
                else {
                    val dest = resolvePath(destDir)
                    dest.mkdirs()
                    val destCanonical = dest.canonicalFile
                    var total = 0L
                    var count = 0
                    ZipInputStream(FileInputStream(zipFile)).use { zis ->
                        var entry = zis.nextEntry
                        while (entry != null) {
                            count++
                            if (count > MAX_ZIP_ENTRIES) break
                            val entryFile = safeZipEntryFile(destCanonical, entry)
                            if (entryFile != null) {
                                if (entry.isDirectory) {
                                    entryFile.mkdirs()
                                } else {
                                    entryFile.parentFile?.mkdirs()
                                    FileOutputStream(entryFile).use { fos ->
                                        val written = copyLimited(zis, fos, MAX_ZIP_ENTRY_BYTES)
                                        total += written
                                        if (total > MAX_ZIP_TOTAL_BYTES) throw IllegalStateException("zip too large")
                                    }
                                }
                            }
                            zis.closeEntry()
                            entry = zis.nextEntry
                        }
                    }
                    dest.absolutePath
                }
            } catch (_: Exception) { "" }
        }

        @JavascriptInterface
        fun getTxtInFolder(dirPath: String): String {
            return try {
                val dir = resolvePath(dirPath)
                if (!dir.isDirectory) ""
                else {
                    val sb = StringBuilder()
                    dir.listFiles()?.filter { it.extension.lowercase(Locale.ROOT) == "txt" }?.sortedBy { it.name }?.forEach { file ->
                        val root = fileRoot.canonicalFile
                        val canonical = file.canonicalFile
                        if (canonical.path.startsWith(root.path + File.separator) && file.length() <= MAX_BRIDGE_BODY_BYTES) {
                            sb.append(file.readText())
                        }
                    }
                    sb.toString()
                }
            } catch (_: Exception) { "" }
        }

        @JavascriptInterface
        fun getZipStringContent(url: String, path: String): String = try {
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.connectTimeout = 15000
            conn.readTimeout = 30000
            if (!isAllowedWebViewUrl(url) || conn.contentLengthLong > MAX_ZIP_DOWNLOAD_BYTES) throw IllegalArgumentException("invalid zip url")
            var found: String? = null
            conn.inputStream.use { input ->
                val zipBytes = readLimitedBytes(input, MAX_ZIP_DOWNLOAD_BYTES)
                ZipInputStream(ByteArrayInputStream(zipBytes)).use { zis ->
                    var entry = zis.nextEntry
                    while (entry != null) {
                        if (entry.name == path) {
                            found = String(readLimitedBytes(zis, MAX_ZIP_ENTRY_BYTES), Charsets.UTF_8)
                            break
                        }
                        zis.closeEntry()
                        entry = zis.nextEntry
                    }
                }
            }
            found ?: ""
        } catch (_: Exception) { "" }

        @JavascriptInterface
        fun getZipByteArrayContent(url: String, path: String): String = try {
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.connectTimeout = 15000
            conn.readTimeout = 30000
            if (!isAllowedWebViewUrl(url) || conn.contentLengthLong > MAX_ZIP_DOWNLOAD_BYTES) throw IllegalArgumentException("invalid zip url")
            var found: String? = null
            conn.inputStream.use { input ->
                val zipBytes = readLimitedBytes(input, MAX_ZIP_DOWNLOAD_BYTES)
                ZipInputStream(ByteArrayInputStream(zipBytes)).use { zis ->
                    var entry = zis.nextEntry
                    while (entry != null) {
                        if (entry.name == path) {
                            val bytes = readLimitedBytes(zis, MAX_ZIP_ENTRY_BYTES)
                            found = bytes.joinToString(prefix = "[", postfix = "]") { (it.toInt() and 0xff).toString() }
                            break
                        }
                        zis.closeEntry()
                        entry = zis.nextEntry
                    }
                }
            }
            found ?: "[]"
        } catch (_: Exception) { "[]" }

        @JavascriptInterface
        fun queryBase64Ttf(base64: String): String {
            return try {
                val bytes = Base64.decode(base64, Base64.DEFAULT)
                queryTtfBytes(bytes)
            } catch (_: Exception) {
                "null"
            }
        }

        @JavascriptInterface
        fun queryTtf(input: String): String {
            return try {
                val bytes = when {
                    input.startsWith("http://") || input.startsWith("https://") -> {
                        if (!isAllowedWebViewUrl(input)) return "null"
                        val conn = URL(input).openConnection() as HttpURLConnection
                        conn.connectTimeout = 15000
                        conn.readTimeout = 30000
                        if (conn.contentLengthLong > MAX_BRIDGE_BODY_BYTES) return "null"
                        conn.inputStream.use { readLimitedBytes(it, MAX_BRIDGE_BODY_BYTES) }
                    }
                    input.length > 100 && !input.contains('/') && !input.contains('\\') -> {
                        Base64.decode(input, Base64.DEFAULT)
                    }
                    else -> {
                        val file = resolvePath(input)
                        if (file.exists()) file.readBytes() else return "null"
                    }
                }
                queryTtfBytes(bytes)
            } catch (_: Exception) {
                "null"
            }
        }

        @JavascriptInterface
        fun replaceFont(text: String, font1Json: String, font2Json: String): String {
            return try {
                val map1 = parseGlyphMap(font1Json)
                val map2 = parseGlyphMap(font2Json)
                val glyphToCodepoint = mutableMapOf<Int, Int>()
                for ((codepoint, glyph) in map2) {
                    glyphToCodepoint[glyph] = codepoint
                }
                val sb = StringBuilder()
                for (ch in text) {
                    val codepoint = ch.code
                    val glyph = map1[codepoint] ?: -1
                    val replacement = glyphToCodepoint[glyph]
                    if (replacement != null) {
                        sb.append(replacement.toChar())
                    } else {
                        sb.append(ch)
                    }
                }
                sb.toString()
            } catch (_: Exception) {
                text
            }
        }

        private fun queryTtfBytes(bytes: ByteArray): String {
            return try {
                val bi = java.nio.ByteBuffer.wrap(bytes)
                bi.order(java.nio.ByteOrder.BIG_ENDIAN)
                val sfVersion = bi.getInt()
                val numTables = bi.getShort().toInt() and 0xFFFF
                bi.position(bi.position() + 6)
                var cmapOffset = -1L
                for (i in 0 until numTables) {
                    val tag = String(byteArrayOf(bi.get(), bi.get(), bi.get(), bi.get()), Charsets.US_ASCII)
                    bi.getInt()
                    val offset = bi.getInt().toLong() and 0xFFFFFFFFL
                    val length = bi.getInt().toLong() and 0xFFFFFFFFL
                    if (tag == "cmap") {
                        cmapOffset = offset
                        break
                    }
                }
                if (cmapOffset < 0) return "null"
                bi.position(cmapOffset.toInt())
                bi.getShort()
                val numSubtables = bi.getShort().toInt() and 0xFFFF
                var subtableOffset = -1L
                for (i in 0 until numSubtables) {
                    val platformId = bi.getShort().toInt() and 0xFFFF
                    val encodingId = bi.getShort().toInt() and 0xFFFF
                    val offset = bi.getInt().toLong() and 0xFFFFFFFFL
                    if ((platformId == 0 || platformId == 3) && (encodingId == 1 || encodingId == 10)) {
                        subtableOffset = cmapOffset + offset
                        break
                    }
                }
                if (subtableOffset < 0) {
                    subtableOffset = cmapOffset + (bi.getInt(0).toLong() and 0xFFFFFFFFL)
                }
                bi.position(subtableOffset.toInt())
                val format = bi.getShort().toInt() and 0xFFFF
                val map = mutableMapOf<Int, Int>()
                when (format) {
                    4 -> {
                        bi.position(bi.position() + 2)
                        val segCountX2 = bi.getShort().toInt() and 0xFFFF
                        val segCount = segCountX2 / 2
                        bi.position(bi.position() + 6)
                        val endCodes = ShortArray(segCount)
                        for (i in 0 until segCount) endCodes[i] = bi.getShort()
                        bi.getShort()
                        val startCodes = ShortArray(segCount)
                        for (i in 0 until segCount) startCodes[i] = bi.getShort()
                        val idDeltas = ShortArray(segCount)
                        for (i in 0 until segCount) idDeltas[i] = bi.getShort()
                        val idRangeOffsetsPos = bi.position()
                        val idRangeOffsets = ShortArray(segCount)
                        for (i in 0 until segCount) idRangeOffsets[i] = bi.getShort()
                        for (i in 0 until segCount) {
                            val start = startCodes[i].toInt() and 0xFFFF
                            val end = endCodes[i].toInt() and 0xFFFF
                            val delta = idDeltas[i].toInt() and 0xFFFF
                            val rangeOffset = idRangeOffsets[i].toInt() and 0xFFFF
                            for (c in start..end) {
                                val glyph = if (rangeOffset != 0) {
                                    val savedPos = bi.position()
                                    bi.position(idRangeOffsetsPos + i * 2 + rangeOffset + (c - start) * 2)
                                    val g = bi.getShort().toInt() and 0xFFFF
                                    bi.position(savedPos)
                                    if (g != 0) (g + delta) and 0xFFFF else -1
                                } else {
                                    (c + delta) and 0xFFFF
                                }
                                if (glyph >= 0) map[c] = glyph
                            }
                        }
                    }
                    12 -> {
                        bi.position(bi.position() + 2)
                        val numGroups = bi.getInt()
                        for (i in 0 until numGroups) {
                            val startCharCode = bi.getInt().toLong() and 0xFFFFFFFFL
                            val endCharCode = bi.getInt().toLong() and 0xFFFFFFFFL
                            val startGlyphId = bi.getInt().toLong() and 0xFFFFFFFFL
                            for (c in startCharCode..endCharCode) {
                                map[c.toInt()] = (startGlyphId + (c - startCharCode)).toInt()
                            }
                        }
                    }
                    else -> return "null"
                }
                val json = org.json.JSONObject()
                for ((k, v) in map) {
                    json.put(k.toString(), v)
                }
                json.toString()
            } catch (_: Exception) {
                "null"
            }
        }

        private fun parseGlyphMap(json: String): Map<Int, Int> {
            if (json.isBlank() || json == "null") return emptyMap()
            return try {
                val obj = org.json.JSONObject(json)
                val map = mutableMapOf<Int, Int>()
                for (key in obj.keys()) {
                    map[key.toInt()] = obj.getInt(key)
                }
                map
            } catch (_: Exception) {
                emptyMap()
            }
        }

        private fun evaluateRule(content: String, rule: String): List<String> {
            val trimmed = rule.trim()
            if (trimmed.isBlank()) return emptyList()

            val tagOnly = Regex("""^@css:(.*)""").find(trimmed)?.groupValues?.get(1)?.trim() ?: trimmed
            val tagPattern = Regex("""^(\w+)(?:\.([\w-]+))?(?:#([\w-]+))?""")
            val match = tagPattern.find(tagOnly)
            if (match == null) return Regex(Regex.escape(tagOnly)).findAll(content).map { it.value }.toList()

            val tag = match.groupValues[1]
            val cls = match.groupValues.getOrNull(2)?.takeIf { it.isNotEmpty() }
            val id = match.groupValues.getOrNull(3)?.takeIf { it.isNotEmpty() }

            val attrPattern = StringBuilder()
            if (cls != null) attrPattern.append("""class\s*=\s*["'][^"']*\b${Regex.escape(cls)}\b[^"']*["']""")
            if (id != null) {
                if (attrPattern.isNotEmpty()) attrPattern.append("""\s+""")
                attrPattern.append("""id\s*=\s*["']${Regex.escape(id)}["']""")
            }

            val tagRegex = if (attrPattern.isNotEmpty()) {
                Regex("""<$tag\b[^>]*${attrPattern}[^>]*>(.*?)</$tag>""", RegexOption.DOT_MATCHES_ALL)
            } else {
                Regex("""<$tag\b[^>]*>(.*?)</$tag>""", RegexOption.DOT_MATCHES_ALL)
            }

            return tagRegex.findAll(content).map { it.value }.toList()
        }

        private fun extractTagName(html: String): String? {
            return Regex("""<(\w+)""").find(html)?.groupValues?.get(1)
        }

        private fun extractText(html: String): String {
            return html.replace(Regex("<[^>]+>"), "").replace(Regex("\\s+"), " ").trim()
        }

        private fun extractOwnText(html: String): String {
            val inner = Regex("""^<[^>]+>(.*?)<""", RegexOption.DOT_MATCHES_ALL).find(html)?.groupValues?.get(1) ?: ""
            return inner.replace(Regex("\\s+"), " ").trim()
        }

        private fun extractInnerHtml(html: String): String {
            return Regex("""^<[^>]+>(.*)</\w+>$""", RegexOption.DOT_MATCHES_ALL).find(html)?.groupValues?.get(1) ?: ""
        }

        private fun extractAttr(html: String, attr: String): String? {
            return Regex("""$attr\s*=\s*["']([^"']*)["']""").find(html)?.groupValues?.get(1)
        }

        private fun resolvePath(path: String): File {
            if (path.isBlank()) throw IllegalArgumentException("empty path")
            val root = fileRoot.canonicalFile
            val candidate = File(root, path).canonicalFile
            if (!candidate.path.startsWith(root.path + File.separator) && candidate != root) {
                throw SecurityException("path escapes sandbox")
            }
            return candidate
        }

        private fun safeZipEntryFile(destCanonical: File, entry: ZipEntry): File? {
            val name = entry.name
            if (name.isBlank() || name.startsWith("/") || name.startsWith("\\")) return null
            val segments = name.replace('\\', '/').split('/')
            if (segments.any { it.isBlank() || it == "." || it == ".." }) return null
            val candidate = File(destCanonical, name).canonicalFile
            val prefix = destCanonical.path + File.separator
            return if (candidate.path.startsWith(prefix) || candidate == destCanonical) candidate else null
        }

        private fun readLimitedBytes(input: InputStream, maxBytes: Long): ByteArray {
            val out = ByteArrayOutputStream()
            copyLimited(input, out, maxBytes)
            return out.toByteArray()
        }

        private fun copyLimited(input: InputStream, output: OutputStream, maxBytes: Long): Long {
            val buffer = ByteArray(8 * 1024)
            var total = 0L
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                total += read
                if (total > maxBytes) throw IllegalStateException("stream too large")
                output.write(buffer, 0, read)
            }
            return total
        }

        private fun hexToBytes(hex: String): ByteArray {
            val len = hex.length
            val data = ByteArray(len / 2)
            for (i in 0 until len step 2) {
                data[i / 2] = ((Character.digit(hex[i], 16) shl 4) + Character.digit(hex[i + 1], 16)).toByte()
            }
            return data
        }

        private fun bytesToHex(bytes: ByteArray): String = bytes.joinToString("") { byte -> "%02x".format(byte) }

        private fun aesCrypt(mode: Int, data: ByteArray, key: String, transformation: String, iv: String): ByteArray {
            val cipher = Cipher.getInstance(transformation)
            val keyBytes = key.toByteArray(Charsets.UTF_8)
            val keySize = if (transformation.contains("192")) 24 else if (transformation.contains("256")) 32 else 16
            val keySpec = SecretKeySpec(keyBytes.copyOf(keySize), "AES")
            if (iv.isNotEmpty() && !transformation.uppercase(Locale.ROOT).contains("ECB")) {
                val ivBytes = iv.toByteArray(Charsets.UTF_8).copyOf(16)
                val ivSpec = IvParameterSpec(ivBytes)
                cipher.init(mode, keySpec, ivSpec)
            } else {
                cipher.init(mode, keySpec)
            }
            return cipher.doFinal(data)
        }

        private fun digestHex(algorithm: String, value: String): String {
            val digest = MessageDigest.getInstance(algorithm).digest(value.toByteArray(Charsets.UTF_8))
            return digest.joinToString("") { byte -> "%02x".format(byte) }
        }

        private fun parseHeaders(headersJson: String): Map<String, String> {
            if (headersJson.isBlank()) return emptyMap()
            return try {
                val obj = org.json.JSONObject(headersJson)
                obj.keys().asSequence().associateWith { key -> obj.optString(key) }
            } catch (_: Exception) {
                emptyMap()
            }
        }
    }

    private fun decodeJsString(value: String?): String {
        if (value == null || value == "null") return ""
        return try {
            org.json.JSONTokener(value).nextValue().toString()
        } catch (_: Exception) {
            value.trim('"')
        }
    }

    override fun onResume() {
        super.onResume()
        createNotificationChannels()
        if (waitingForSettingsReturn && pendingNotificationResult != null) {
            pendingNotificationResult?.success(hasNotificationPermission())
            pendingNotificationResult = null
            waitingForSettingsReturn = false
        }
    }

    private fun hasNotificationPermission(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(
                    this, Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED) {
                return false
            }
        }
        return NotificationManagerCompat.from(this).areNotificationsEnabled()
    }

    private fun requestNotificationPermission(result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (hasNotificationPermission()) {
                result.success(true)
            } else if (pendingNotificationResult != null) {
                result.error("PERMISSION_REQUEST_PENDING", "A permission request is already in progress", null)
            } else if (ContextCompat.checkSelfPermission(
                    this, Manifest.permission.POST_NOTIFICATIONS
                ) == PackageManager.PERMISSION_GRANTED) {
                pendingNotificationResult = result
                waitingForSettingsReturn = true
                openAppNotificationSettings()
            } else {
                pendingNotificationResult = result
                ActivityCompat.requestPermissions(
                    this,
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    NOTIFICATION_PERMISSION_REQUEST_CODE
                )
            }
        } else {
            if (pendingNotificationResult != null) {
                result.error("PERMISSION_REQUEST_PENDING", "A permission request is already in progress", null)
            } else {
                pendingNotificationResult = result
                waitingForSettingsReturn = true
                openAppNotificationSettings()
            }
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == NOTIFICATION_PERMISSION_REQUEST_CODE) {
            val granted = grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED
            val fullStatus = if (granted) hasNotificationPermission() else false
            pendingNotificationResult?.success(fullStatus)
            pendingNotificationResult = null
            waitingForSettingsReturn = false
            if (fullStatus) {
                android.util.Log.i("MainActivity", "Notification permission granted")
            } else {
                android.util.Log.w("MainActivity", "Notification permission denied")
            }
        }
    }

    private fun openAppNotificationSettings() {
        try {
            startActivity(
                Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).apply {
                    putExtra(Settings.EXTRA_APP_PACKAGE, packageName)
                }
            )
        } catch (e: Exception) {
            startActivity(
                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                    data = android.net.Uri.parse("package:$packageName")
                }
            )
        }
    }

    private fun createNotificationChannels() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                DOWNLOAD_CHANNEL_ID,
                DOWNLOAD_CHANNEL_NAME,
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "书籍下载进度通知"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }
}
