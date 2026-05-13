# Book Source, Search, Bookshelf, And Reader Improvement Plan

## Goal

Improve the main reading flow:

```text
Import book sources -> Search -> View book details -> Add to bookshelf -> Manage bookshelf -> Read -> Cache chapters -> Restore reading progress
```

Current focus is Android only. Multi-device sync is out of scope for now.

## Confirmed Decisions

- Keep the existing paste-JSON book source import.
- Add Android local `.json` file import.
- Search result layout uses cover on the left, book info in the middle, and add button on the right.
- Search result details use a bottom sheet.
- Reading progress is local single-device only for now.
- Deleting a book also deletes its chapters, cached chapter content, and reading progress.

## Batch 0: Current State Audit

Goal: confirm existing APIs, DAO methods, fields, and UI state before changing code.

Check these files:

- `flutter_app/lib/features/source/source_page.dart`
- `flutter_app/lib/features/search/search_page.dart`
- `flutter_app/lib/features/bookshelf/bookshelf_page.dart`
- `flutter_app/lib/features/reader/reader_page.dart`
- `flutter_app/lib/core/api/source_api.dart`
- `flutter_app/lib/core/api/search_api.dart`
- `flutter_app/lib/core/api/bookshelf_api.dart`
- `flutter_app/lib/core/api/reader_api.dart`
- `flutter_app/lib/core/api/dto.dart`
- `core/bridge/src/api.rs`
- `core/core-storage/src/source_dao.rs`
- `core/core-storage/src/book_dao.rs`
- `core/core-storage/src/chapter_dao.rs`
- `core/core-storage/src/progress_dao.rs`
- `core/core-storage/src/models.rs`
- `core/api-server/src/routes/bookshelf.rs`
- `core/api-server/src/routes/reader.rs`
- `core/api-server/src/routes/search.rs`
- `core/api-server/src/routes/sources.rs`

Audit points:

- Whether paste JSON import already supports both single object and array.
- Whether Android file picking dependencies already exist.
- Whether search result DTO contains `cover_url`, `intro`, `kind`, `last_chapter`, `source_id`, and `source_name`.
- Whether adding to bookshelf persists `cover_url`.
- Whether bookshelf delete API already exists.
- Whether chapter table already stores chapter body content.
- Whether reading progress table already stores `chapter_index` and `scroll_offset`.
- Whether reader page already has a `ScrollController`.

Deliverables:

- Minimal file change list.
- Missing fields/interfaces list.
- DB migration requirement, if any.

## Batch 1: Android File Import For Book Sources

Goal: keep paste JSON import and add Android local file import.

Features:

- Add a "Import from file" action on the source page.
- Pick Android local `.json` files.
- Read file content and reuse the existing JSON import path.
- Support a single book source object.
- Support a book source array/collection.
- Show imported source count after success.
- Treat validation warning/info as hints, not import blockers.
- Treat JSON parse errors, file read failures, and DB errors as failures.

Files:

- `source_page.dart`
- `source_api.dart`
- `source_dao.rs` only if compatibility gaps are found.

Possible dependency:

- `file_picker`, unless the project already has a file picker dependency.

Acceptance criteria:

- Paste JSON import still works.
- Android `.json` file import works.
- Single source JSON imports successfully.
- Source collection JSON imports successfully.
- Working sources are not blocked by validation warnings.

## Batch 2: Search Result List Redesign

Goal: make search results easier to scan and separate viewing details from adding to bookshelf.

Layout:

```text
+--------+--------------------------+--------+
| Cover  | Title                    | Add    |
| Left   | Author / Kind / Latest   | Right  |
|        | Intro summary            |        |
+--------+--------------------------+--------+
```

Interactions:

- Tap left cover to open detail bottom sheet.
- Tap middle info area to open detail bottom sheet.
- Tap right add button to add directly to bookshelf.
- Tapping the result body must not add the book accidentally.

Fields:

- `cover_url`
- `name`
- `author`
- `kind`
- `last_chapter`
- `intro`
- `source_name`

Files:

- `search_page.dart`
- `dto.dart`
- `search_api.dart`
- Rust search result field mapping if required.

Acceptance criteria:

- Search results show cover on the left.
- Add button is independent on the right.
- Tapping the result body opens details.
- Tapping add adds the book to bookshelf.

## Batch 3: Search Detail Bottom Sheet

Goal: show complete book information before adding to bookshelf.

Bottom sheet content:

- Cover
- Title
- Author
- Kind/category
- Latest chapter
- Source name
- Intro
- Add to bookshelf button

Interactions:

- Search result body opens the bottom sheet.
- Bottom sheet add button adds to bookshelf.
- If the book already exists in bookshelf, show an existing-state hint or disabled button.

Files:

- `search_page.dart`
- Optional `_SearchResultTile`
- Optional `_BookDetailSheet`

Acceptance criteria:

- Full book details are visible from search results.
- Details bottom sheet can add the book to bookshelf.
- Added book appears in bookshelf.

## Batch 4: Bookshelf Cover And Delete

Goal: add basic bookshelf management.

Features:

- Show book cover in bookshelf.
- Show title, author, and latest chapter or reading progress.
- Delete a book by long press or menu action.
- Confirm before deletion.
- Refresh bookshelf after deletion.

Deletion behavior:

```text
Delete book
Delete its chapters
Delete its cached chapter content
Delete its reading progress
Do not delete the book source
Do not delete unrelated global cache
```

Files:

- `bookshelf_page.dart`
- `bookshelf_api.dart`
- `book_dao.rs`
- `chapter_dao.rs`
- `progress_dao.rs`
- `bridge/src/api.rs`
- `api-server/src/routes/bookshelf.rs`

Acceptance criteria:

- Bookshelf shows covers.
- User can delete books.
- Deletion asks for confirmation.
- Deleted book disappears from bookshelf.
- Related chapter cache and reading progress are removed.

## Batch 5: Reader Table Of Contents Cache

Goal: avoid reloading TOC every time the reader opens.

Strategy:

- When opening reader from bookshelf, read local chapters first.
- If no local chapters exist, fetch TOC from network.
- Save fetched chapters locally.
- If TOC is empty, show refresh TOC action.
- Manual refresh refetches network TOC and updates local chapters.

Files:

- `reader_page.dart`
- `reader_api.dart`
- `chapter_dao.rs`
- `api-server/src/routes/reader.rs`
- `bridge/src/api.rs`
- `parser.rs`

Acceptance criteria:

- First reader open loads TOC.
- Later opens use local TOC first.
- User can refresh TOC manually.
- Empty TOC does not produce a blank dead-end UI.

## Batch 6: Reader Chapter Content Cache

Goal: already-read chapters should open quickly without network.

Strategy:

- When opening a chapter, read local `chapter.content` first.
- If content is non-empty, display it immediately.
- If content is empty, fetch from network.
- Save fetched content to cache.
- On network failure, keep old cached content if available.
- After current chapter loads, pre-cache one next chapter in the background.
- Pre-cache only one chapter to avoid excessive requests.

Refresh:

- Add refresh current chapter action.
- Refresh ignores cache.
- Successful refresh overwrites cached content.
- Failed refresh does not clear old content.

Files:

- `reader_page.dart`
- `reader_api.dart`
- `chapter_dao.rs`
- `api-server/src/routes/reader.rs`
- `bridge/src/api.rs`

Acceptance criteria:

- Already-read chapter opens from cache.
- Current chapter can be manually refreshed.
- Next chapter can be pre-cached in background.
- Failed refresh does not erase existing content.

## Batch 7: Reading Progress Save And Restore

Goal: reopening a book returns to the previous reading location.

Scope:

```text
Single-device local progress only.
No sync for now.
```

Persisted fields:

```text
book_id
chapter_id
chapter_index
scroll_offset
updated_at
```

Save timing:

- Before switching chapters.
- When leaving reader page.
- After scrolling stops with debounce.
- Attempt one save when app enters background.

Recommended debounce:

```text
Save 500ms after scrolling stops.
```

Restore flow:

- User opens a book from bookshelf.
- Query local progress by `book_id`.
- If progress exists, open saved chapter.
- After content renders, scroll to `scroll_offset`.
- If no progress exists, open first chapter.
- If saved chapter no longer exists, fall back to nearest valid chapter or first chapter.

Files:

- `reader_page.dart`
- `progress_dao.rs`
- `models.rs`
- `database.rs`
- `reader_api.dart`
- `bridge/src/api.rs`

Acceptance criteria:

- Exit mid-chapter and reopen from bookshelf restores same chapter and position.
- Switching chapters updates progress.
- Refreshing current chapter does not lose progress.
- Deleting a book deletes its progress.

## Batch 8: Reader Refresh And Error Fallbacks

Goal: users can recover from cache, parse, network, or WebView failures.

Features:

- Refresh TOC.
- Refresh current chapter.
- Show reload action when content is empty.
- Show readable network error.
- Show retry action on WebView failure.
- Failed refresh must not overwrite old cached content.

Files:

- `reader_page.dart`
- `reader_api.dart`
- `platform_webview_executor.dart`
- `api-server/src/routes/reader.rs`

Acceptance criteria:

- Empty chapter can be retried.
- TOC error can be refreshed.
- WebView failure is not a permanent blank screen.
- Old cache survives failed refresh.

## Batch 9: Cover Cache Enhancement

Goal: stable cover display with fewer repeated requests.

Lightweight first step:

- Use `cached_network_image` in search results.
- Use `cached_network_image` in bookshelf.
- Show placeholder while loading.
- Show default cover on failure.
- Persist `cover_url` when adding to bookshelf.

Later enhancement:

- Add local cover directory.
- Hash `cover_url` as filename.
- Download cover when adding to bookshelf.
- Save local path to `custom_cover_path`.
- Display priority:
  - `custom_cover_path`
  - `cover_url`
  - default cover

Acceptance criteria:

- Search page shows covers.
- Bookshelf shows covers.
- Network failures do not block the main flow.
- Later local-cover support can show cached cover offline.

## Batch 10: Book Source Validation Improvements

Goal: validation should not mislead users for valid Legado rules.

Rules:

- `@js:` should be info.
- Default JSOUP chain rules should be info.
- `@css:` rules with JSOUP pseudo-classes should be info.
- Standard CSS rules should be validated by CSS parser.
- Real errors should stay error-level.

JSOUP pseudo-classes:

```text
:contains()
:matches()
:matchText()
:eq()
:lt()
:gt()
```

Real errors:

- Invalid JSON.
- Missing required URL.
- Missing required fields.
- Invalid regex syntax.
- XPath/JSONPath compile failure.

Acceptance criteria:

- 爱下电子书 does not show CSS warnings for valid JSOUP rules.
- 速读谷 `:contains()` does not show CSS warnings.
- JS/JSOUP dynamic rules show info.
- Working sources are not blocked by static validation.

## Batch 11: Android Build Script

Goal: avoid rebuilding Flutter APK without rebuilding updated Rust `.so`.

Problem:

- `flutter build apk` does not rebuild Rust `libbridge.so`.
- After changing Rust code, the bridge library must be rebuilt and copied into `jniLibs`.

Suggested script:

```text
build_android_debug.sh
```

Flow:

```text
cargo build --manifest-path core/bridge/Cargo.toml --release --target aarch64-linux-android
cp core/target/aarch64-linux-android/release/libbridge.so flutter_app/android/app/src/main/jniLibs/arm64-v8a/libbridge.so
cd flutter_app
flutter build apk --debug
adb install -r build/app/outputs/flutter-apk/app-debug.apk
```

Acceptance criteria:

- One command rebuilds Rust `.so`, APK, and installs to Android.
- Avoids stale native library issues.

## Recommended Execution Order

1. Batch 0: Current state audit
2. Batch 1: Android file import
3. Batch 2: Search result list redesign
4. Batch 3: Search detail bottom sheet
5. Batch 4: Bookshelf cover and delete
6. Batch 5: Reader TOC cache
7. Batch 6: Reader chapter content cache
8. Batch 7: Reading progress save and restore
9. Batch 8: Reader refresh and error fallbacks
10. Batch 9: Cover cache enhancement
11. Batch 10: Book source validation improvements
12. Batch 11: Android build script

## First Usable Milestone

The first milestone should deliver:

```text
Android file import
-> Search list with left cover
-> Detail bottom sheet
-> Right-side add button
-> Bookshelf cover display
-> Delete book with cache/progress cleanup
-> Cached chapter reading
-> Restore reading progress
-> Refresh current chapter
```

This milestone gives the main user flow a clear upgrade without waiting for every enhancement to be complete.
