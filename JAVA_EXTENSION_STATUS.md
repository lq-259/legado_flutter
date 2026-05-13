# Java Extension Support Matrix

| Function | Rust Binding | Rust Test | Rust Full | Android Bridge | Android JS | Notes |
|----------|-------------|-----------|-----------|----------------|------------|-------|
| ajax | ✅ | ✅ | ✅ | ✅ (http) | ✅ | HTTP GET, auto charset detection |
| connect | ✅ | ✅ | ✅ | ✅ (http) | ✅ | Wraps get(); returns {body, toString} |
| ajaxAll | ✅ | ❌ | ✅ | ❌ | ❌ | Parallel GET via __legado_http_request |
| post | ✅ | ✅ | ✅ | ✅ (http) | ✅ | HTTP POST with body & headers |
| get | ✅ | ✅ | ✅ | ✅ (http) | ✅ | Var lookup (1 arg) or HTTP GET (2 args) |
| getCookie | ✅ | ✅ | ✅ | ✅ | ✅ | Cookie jar + tag/key extraction |
| put | ✅ | ✅ | ✅ | ❌ | ❌ | Writes to _vars map; no bridge needed |
| getFromMemory | ✅ | ✅ (via cache) | ✅ | ✅ (cacheGet) | ✅ | L1: JS vars, L2: SQLite cache |
| putMemory | ✅ | ✅ (via cache) | ✅ | ✅ (cachePut) | ✅ | L1: JS vars, L2: SQLite cache |
| setContent | ✅ | ✅ | ✅ | ✅ | ❌ | Sets storedContent + storedBaseUrl |
| getString | ✅ | ✅ | ✅ | ✅ | ❌ | Rule-based HTML extraction |
| getStringList | ✅ | ✅ | ✅ | ✅ | ❌ | Rule-based HTML list extraction |
| getElements | ✅ | ✅ | ✅ | ✅ | ❌ | Full element tree with children, select |
| log | ✅ | ✅ | ⚠️ | ✅ | ✅ | Rust: no-op stub; Android: logcat |
| encodeURI | ✅ | ⚠️ | ✅ | ✅ (URI compat) | ✅ | Same impl as encodeURIComponent |
| encodeURIComponent | ✅ | ✅ | ✅ | ✅ (URI compat) | ✅ | URL encoding (UTF-8) |
| decodeURI | ✅ | ✅ | ✅ | ✅ (URI compat) | ✅ | URL decoding (UTF-8) |
| base64Encode | ✅ | ✅ | ✅ | ✅ | ✅ | Standard Base64 |
| base64Decode | ✅ | ✅ | ✅ | ✅ | ✅ | Standard Base64 decode |
| base64DecodeToByteArray | ✅ | ✅ | ✅ | ✅ | ✅ | Base64 → JSON byte array |
| md5Encode | ✅ | ✅ | ✅ | ✅ | ✅ | MD5 hex (lowercase) |
| md5Encode16 | ✅ | ✅ | ✅ | ✅ | ✅ | MD5 16-char (substring 8..24) |
| aesDecodeToString | ✅ | ✅ (ecb hex) | ✅ | ✅ | ❌ | AES decrypt → hex/string |
| aesBase64DecodeToString | ✅ | ✅ (cbc) | ✅ | ✅ | ❌ | AES decrypt base64 → string |
| aesEncodeToString | ✅ | ✅ (ecb hex) | ✅ | ✅ | ❌ | AES encrypt → hex string |
| aesEncodeToBase64String | ✅ | ✅ (cbc) | ✅ | ✅ | ❌ | AES encrypt → base64 |
| aesDecodeToByteArray | ✅ | ⚠️ | ✅ | ✅ | ❌ | Same flow as base64 variant, untested standalone |
| aesBase64DecodeToByteArray | ✅ | ✅ | ✅ | ✅ | ❌ | AES base64 decrypt → byte array |
| aesEncodeToByteArray | ✅ | ✅ | ✅ | ✅ | ❌ | AES encrypt → byte array |
| aesEncodeToBase64ByteArray | ✅ | ❌ | ✅ | ✅ | ❌ | AES encrypt → base64 byte array, untested |
| timeFormat | ✅ | ✅ | ✅ | ✅ | ✅ | Unix ms → yyyy/MM/dd HH:mm |
| htmlFormat | ✅ | ✅ | ✅ | ✅ | ✅ | HTML entity decode, br/p → newline, strip tags |
| getZipStringContent | ✅ | ✅ | ✅ | ✅ | ❌ | Stream ZIP over HTTP → string |
| getZipByteArrayContent | ✅ | ✅ | ✅ | ✅ | ❌ | Stream ZIP over HTTP → byte array |
| readFile | ✅ | ✅ | ✅ | ✅ | ❌ | Read file → JSON byte array |
| readTxtFile | ✅ | ✅ | ✅ | ✅ | ❌ | Read file with charset auto-detect |
| downloadFile | ✅ | ✅ | ✅ | ✅ | ❌ | HTTP GET → save to disk |
| getFile | ✅ | ✅ | ⚠️ | ✅ | ❌ | Rust returns content; Android returns path |
| deleteFile | ✅ | ✅ | ✅ | ✅ | ❌ | Delete file, returns "true"/"false" |
| unzipFile | ✅ | ✅ | ✅ | ✅ | ❌ | Unzip to dest dir |
| getTxtInFolder | ✅ | ✅ | ✅ | ✅ | ❌ | Concatenate .txt files in dir |
| utf8ToGbk | ✅ | ❌ | ✅ | ✅ | ❌ | Re-encode UTF-8 bytes as GBK |
| queryBase64Ttf | ✅ | ❌ | ✅ | ✅ | ✅ | TTF cmap extraction (base64 input) |
| queryTtf | ✅ | ❌ | ✅ | ✅ | ✅ | TTF cmap extraction (URL/file/base64 auto-detect) |
| replaceFont | ✅ | ❌ | ✅ | ✅ | ✅ | Glyph substitution via font maps |

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Full implementation / test present |
| ⚠️ | Partial, stub, or untested standalone |
| ❌ | Missing entirely |
| 🔷 | Android only |

## Summary

- **45** functions defined in the `java` object (PREAMBLE)
- **45** have associated `__legado_*` native bindings registered in Rust (`register_quickjs_bridge`)
- **45** have Rust implementations (`#[cfg(feature = "js-quickjs")]` functions)
- **37** have dedicated Rust `#[test]` functions
- **39** have `@JavascriptInterface` methods in Android `LegadoJsBridge`
- **22** are exposed in the Android `wrapWebJs` JS template

## Key Gaps

### Rust Tests Missing (8 functions)
- **ajaxAll** — uses same `__legado_http_request` as `ajax`, but has no dedicated test
- **encodeURI** — same impl as `encodeURIComponent`, tested indirectly but no standalone test
- **aesDecodeToByteArray** — same flow as base64 variant, no standalone coverage
- **aesEncodeToBase64ByteArray** — completely untested
- **utf8ToGbk** — no test at all
- **queryBase64Ttf** — no test (complex TTF cmap parsing)
- **queryTtf** — no test (complex TTF parsing + network/file/base64 dispatch)
- **replaceFont** — no test (glyph map substitution)

### Rust Stubs
- **log** — returns empty string (intentional: server-side has no log sink)
- **getFile** — Rust returns file *content*, Android returns file *path* (behavior mismatch)

### Android JS Surface Gaps (23 functions not in wrapWebJs)
These have `@JavascriptInterface` bridge methods but are NOT wired into the `wrapWebJs` template:
`ajaxAll`, `put`, `setContent`, `getString`, `getStringList`, `getElements`, all 8 `aes*` functions, `getZipStringContent`, `getZipByteArrayContent`, `readFile`, `readTxtFile`, `downloadFile`, `getFile`, `deleteFile`, `unzipFile`, `getTxtInFolder`, `utf8ToGbk`

### No Android Bridge At All (4 functions)
- **ajaxAll** — no `@JavascriptInterface`, no JS surface
- **put** — no bridge needed (operates on JS-side `_vars` map)
- **decodeURIComponent** — exposed in JS but uses same `encodeURIComponentCompat`/`decodeURIComponentCompat` bridge

### Duplicated/Redundant
- `encodeURIComponentCompat` is used for both `encodeURIComponent` and `decodeURIComponent` in the bridge (Android uses a single `encodeURIComponentCompat` method for both encode/decode which is a documentation bug — it actually calls two separate `@JavascriptInterface` methods, `encodeURIComponentCompat` and `decodeURIComponentCompat`)

## Test File Reference
All tests are in `core/core-source/src/legado/js_runtime.rs:1558-2250` (49 `#[test]` functions, 37 specifically for `java.*`).
