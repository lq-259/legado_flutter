# Legado Source Compatibility Plan

> Scope: implement source-rule compatibility against the Legado source tutorial at <https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html>.
>
> Last updated: 2026-05-06.

## Goal

Support Legado source features in two layers:

- Rust core supports all non-browser source behavior: import, URL options, HTTP, charset, cookies, selectors, regex, JSONPath, XPath, JavaScript, `java.*` bridges, and parser flows.
- Browser-only behavior is explicit: `webView:true`, `webJs`, and `sourceRegex` require a Flutter/Android WebView bridge and must not silently fail.

This is not the same as claiming full Android Legado App parity inside Rust alone. WebView-dependent behavior needs platform integration.

## Current Baseline

Already available:

- Legado import supports single-source arrays, collection arrays, camelCase-to-snake_case mapping, flexible field types, and real `sy/` samples.
- URL support includes `{{key}}`, `{{page}}`, JS expressions, `<,{{page}}>`, GET/POST options, headers, charset, cookies, and URL option.js.
- Rule support includes Default selector subset, `@css:`, XPath, JSONPath, Regex, partial AllInOne, `&&` / `||` / `%%`, `@js:`, `<js></js>`, `@put/@get`, and `##regex##replacement`.
- JavaScript uses QuickJS by default, Boa as optional `js-boa`, and supports `--no-default-features` builds.
- JS runtime supports multi-statement scripts and many `java.*` bridges: `ajax`, `get`, `post`, `getCookie`, `put`, `get`, `getString`, `getStringList`, `getElements`, Base64, MD5, URI, AES, time, HTML, zip, and restricted file read helpers.
- `java.getElements` returns lightweight Element-like objects with methods such as `text`, `html`, `outerHtml`, `ownText`, `attr`, `hasAttr`, `id`, `className`, `classNames`, `hasClass`, `tagName`, `nodeName`, `absUrl`, `children`, `child`, `select`, `selectFirst`, and `toString`.
- Parser search/book-info/toc/content flows use `LegadoHttpClient` and `execute_legado_rule`.
- Parser JS execution shares cookies, source headers, and charset behavior with normal parser requests.

## Phase 1: Field Preservation And Internal Types

### Purpose

Ensure fields documented by Legado are preserved after import and can be represented by internal types, even before all behavior is implemented.

### Fields To Add

`SearchRule`:

- `intro`
- `word_count`

`BookInfoRule`:

- `book_info_init`
- `toc_url`
- `can_rename`

`TocRule`:

- `next_toc_url`
- `is_vip`
- `update_time`

`ContentRule`:

- `web_js`
- `source_regex`

### Import Mapping To Verify

The import layer should preserve:

- `bookInfoInit -> book_info_init`
- `tocUrl -> toc_url`
- `nextTocUrl -> next_toc_url`
- `isVip -> is_vip`
- `updateTime -> update_time`
- `webJs -> web_js`
- `sourceRegex -> source_regex`
- `canReName -> can_rename`
- `wordCount -> word_count`

### Tests

- Import preserves `ruleBookInfo.bookInfoInit`.
- Import preserves `ruleBookInfo.tocUrl`.
- Import preserves `ruleToc.nextTocUrl`, `isVip`, and `updateTime`.
- Import preserves `ruleContent.webJs` and `sourceRegex`.
- Import preserves `ruleSearch.intro` and `wordCount`.

### Risk

Low. This is mostly type and deserialization preservation. Existing struct literals may need defaults or extra fields.

## Phase 2: Book Info Init And Toc URL

### Purpose

Support the tutorial's book-info preprocessing and directory URL extraction behavior.

### `bookInfoInit`

Legado supports:

- AllInOne regex.
- JavaScript.
- JS returning an object, where detail rules then reference object fields.

Implementation approach:

- In `get_book_info`, execute `book_info_init` after fetching the detail page.
- If the rule is `@js:` or `js:`, run it with shared HTTP state.
- If the rule starts with `:`, use AllInOne regex support.
- If the result is an object/map, evaluate subsequent detail fields against that JSON object.
- Field rules like `a` should work as object field lookup, equivalent to `$.a` for init objects.
- If init is empty or fails, fall back to normal HTML parsing.

### `tocUrl`

Implementation approach:

- If `rule_book_info.toc_url` exists, parse it from the detail page or init result.
- Normalize relative URLs against the detail page URL.
- Set `BookDetail.chapters_url` to the parsed `toc_url`.
- If missing, keep current behavior: use the detail page URL.

### Tests

- JS `bookInfoInit` returns `{a, b, h}` and `name = "a"`, `author = "b"`, `toc_url = "h"` resolve correctly.
- AllInOne `bookInfoInit` can feed fields.
- `tocUrl` selector resolves relative links to full URLs.
- Existing HTML detail parsing still works without init.

### Risk

Medium. Must avoid breaking normal book-info parsing.

## Phase 3: Toc Pagination With `nextTocUrl`

### Purpose

Support multi-page catalogs.

### Behavior

Legado allows `nextTocUrl` to return:

- A single URL.
- A URL array.
- `[]`, `null`, or `""` to stop.

### Implementation Approach

- Change `get_chapters` to loop through catalog pages.
- Parse chapters on each page using existing `chapter_list`, `chapter_name`, and `chapter_url` rules.
- Resolve `next_toc_url` after each page.
- Normalize next URLs against the current catalog page URL.
- Keep a `seen_urls` set.
- Add a `max_toc_pages` limit, e.g. 50.

### Tests

- Two-page catalog merges chapters.
- Empty `nextTocUrl` stops.
- JS `nextTocUrl` returns an array and all URLs are loaded.
- Repeated URL does not infinite loop.
- Catalog reverse marker `-` is respected where applicable.

### Risk

Medium. Needs strict loop limits.

## Phase 4: Content Pagination With `nextContentUrl`

### Purpose

Support multi-page chapter content.

### Important Semantics

`nextContentUrl` is a current-chapter page URL, not the next chapter URL.

### Implementation Approach

- In `get_chapter_content`, parse first page content.
- Resolve `next_content_url`.
- Fetch and append subsequent pages until empty/null/repeated/max limit.
- Normalize next content URL against the current content page URL.
- Use `max_content_pages`, e.g. 20.
- Merge text with `\n` initially.

### Tests

- Two-page chapter content merges text.
- Empty next URL stops.
- Repeated URL stops.
- JS `nextContentUrl` returns URL.
- Existing single-page content remains unchanged.

### Risk

Medium. Need to avoid misusing `ChapterContent.next_chapter_url` for content pagination.

## Phase 5: Non-Search `{{rule}}` Templates

### Purpose

Support tutorial behavior where non-search contexts can use rule expressions inside `{{}}`.

### Tutorial Semantics

In search/explore URLs, `{{}}` is JavaScript only.

Outside search/explore URLs, `{{}}` may contain:

- `@@` Default rules.
- `@xpath:` or `//` XPath.
- `@json:` or `$.` JSONPath.
- `@css:` CSS.
- Default JavaScript.

### Implementation Approach

Add a resolver:

```rust
resolve_rule_template(input, html_or_json, context)
```

Rules:

- `{{@@tag.a@href}}` executes Default rule `tag.a@href`.
- `{{@css:a@href}}` executes CSS rule.
- `{{//a/@href}}` executes XPath.
- `{{$.id}}` executes JSONPath.
- Otherwise execute JavaScript.

Apply initially to URL/content fields only:

- `book_url`
- `cover_url`
- `toc_url`
- `chapter_url`
- `next_toc_url`
- `content`
- `next_content_url`

### Tests

- `{{@css:a@href}}` resolves.
- `{{//a/@href}}` resolves.
- `{{$.id}}` resolves.
- `{{@@tag.a@href}}` resolves.
- JS fallback resolves.

### Risk

Medium-high. Keep search/explore URL semantics JS-only to avoid regressions.

## Phase 6: Explore Support

### Purpose

Implement the tutorial's discovery/explore feature.

### API

Add parser method:

```rust
pub async fn explore(&self, source: &BookSource, explore_url: &str, page: i32) -> Vec<SearchResult>
```

### URL Formats

Support:

- `title::url`
- Multiple entries separated by newlines or `&&`.
- JSON arrays with `title`, `url`, and optional `style`.

### Rule Fields

Reuse search-like fields from `ruleExplore`:

- `book_list`
- `name`
- `author`
- `kind`
- `word_count`
- `last_chapter`
- `intro`
- `cover_url`
- `book_url`

### Tests

- `title::/path?page={{page}}` works.
- JSON-array explore URL works.
- `ruleExplore` parses results like search.
- Relative URLs are normalized.

### Risk

Medium. Flutter/API can be wired later; start with core parser.

## Phase 7: Additional `java.*` P1 Bridges

### Purpose

Cover high-frequency functions from the tutorial that remain missing or incomplete.

### P1 Functions

- `java.connect(url)` as `java.get(url, {})` alias returning response object.
- `java.base64DecodeToByteArray(str[, flags])` returning number array.
- `java.base64Encode(str, flags)` and `java.base64Decode(str, flags)` accepting ignored-compatible flags.
- `java.setContent(content, baseUrl)` for subsequent `getString`, `getStringList`, and `getElements`.
- `java.log(msg)` forwarding to tracing or no-op with stable behavior.

### P2 File/Download Functions

- `java.downloadFile`
- `java.getFile`
- `deleteFile`
- `java.unzipFile`
- `java.getTxtInFolder`

These must remain restricted by `LEGADO_FILE_ROOT` or equivalent runtime config.

### P3 Font Functions

- `java.queryBase64TTF`
- `java.queryTTF`
- `java.replaceFont`

These are complex and should be postponed.

### Tests

- One JS runtime test per function.
- Confirm `--no-default-features` still compiles.
- Confirm `js-boa` still compiles.

### Risk

Low to high depending on function. Start with aliases and pure utilities.

## Phase 8: WebView Bridge Boundary

### Purpose

Handle tutorial features that require a browser.

### WebView-Only Features

- `webView:true`
- `webJs`
- `sourceRegex`
- resource sniffing
- DOM after browser JavaScript execution

### Rust Core Minimum

- Detect `webView:true` in URL options.
- Preserve `web_js` and `source_regex` fields.
- Return or log explicit `NeedWebView` state instead of silently pretending support.

### Platform Bridge Design

Suggested shape:

```rust
trait WebViewExecutor {
    async fn load(&self, request: WebViewRequest) -> Result<WebViewResult>;
}
```

`WebViewRequest` should include:

- URL
- headers
- cookies
- method/body where applicable
- `web_js`
- `source_regex`

`WebViewResult` should include:

- final URL
- HTML
- JS return value
- sniffed resource URLs
- cookies

### Flutter/Android Work

- Load URL in WebView.
- Apply headers/cookies.
- Execute `webJs` after page load.
- Return HTML/JS value/resource sniffing result to Rust/parser.

### Risk

High. Keep as a separate phase.

## Fixed Verification Suite

Run after each phase:

```bash
cargo test -p core-source parser::tests
cargo test -p core-source js_runtime
cargo test -p core-source legado::rule::tests
cargo test -p core-source legado::url::tests
cargo test -p core-source legado::import::tests
cargo test -p core-source rule_engine::tests
cargo check -p core-source --no-default-features
cargo check -p core-source --no-default-features --features js-boa
cargo check -p api-server
```

## Completion Criteria

Non-WebView source support can be considered broadly aligned with the tutorial when:

- Tutorial examples for basic/search/book-info/toc/content import successfully.
- GET/POST/headers/charset/cookies/URL option.js behave consistently.
- Default/CSS/XPath/JSONPath/Regex/AllInOne/OnlyOne/purification/combinators/JS have tests.
- Search/explore/book-info/toc/content parser flows have mock-server tests.
- High-frequency `java.*` bridges have tests.
- Real `sy/` samples keep passing.
- `webView:true`, `webJs`, and `sourceRegex` have explicit bridge handling or explicit unsupported status, not silent failure.

## Recommended Next Work

Start with:

1. Phase 1: Field preservation and internal types.
2. Phase 2: `bookInfoInit` and `tocUrl`.
3. Full verification suite.

These are high-value and lower-risk than pagination or WebView support.
