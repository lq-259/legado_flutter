use serde_json::{json, Value};
use tempfile::TempDir;

fn setup_db() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db").to_string_lossy().to_string();
    core_storage::init_database(&db_path).unwrap();
    (dir, db_path)
}

fn ensure_book(db_path: &str, book_id: &str, book_name: &str) {
    let conn = core_storage::database::get_connection(db_path).unwrap();
    let now = 1700000000_i64;
    let source_id = format!("src_{}", book_id);
    let source_name = format!("Source {}", book_name);
    let source_url = format!("https://{}.example.com", book_id);
    conn.execute(
        "INSERT INTO book_sources (id, name, url, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        (source_id.as_str(), source_name.as_str(), source_url.as_str(), now, now),
    ).unwrap();
    conn.execute(
        "INSERT INTO books (id, source_id, source_name, name, order_time, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (book_id, source_id.as_str(), source_name.as_str(), book_name, now, now, now),
    ).unwrap();
}

#[test]
fn test_create_task_with_chapters() {
    let (_dir, db_path) = setup_db();
    ensure_book(&db_path, "book1", "Test Book");
    let now = 1700000000_i64;

    let task = json!({
        "id": "task1",
        "book_id": "book1",
        "book_name": "Test Book",
        "cover_url": null,
        "total_chapters": 3,
        "downloaded_chapters": 0,
        "status": 1,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    });

    let chapters = json!([
        {
            "id": "ch1",
            "task_id": "task1",
            "chapter_id": "bk1_ch1",
            "chapter_index": 0,
            "chapter_title": "Chapter 1",
            "status": 0,
            "file_path": null,
            "file_size": 0,
            "error_message": null,
            "created_at": now,
            "updated_at": now,
        },
        {
            "id": "ch2",
            "task_id": "task1",
            "chapter_id": "bk1_ch2",
            "chapter_index": 1,
            "chapter_title": "Chapter 2",
            "status": 0,
            "file_path": null,
            "file_size": 0,
            "error_message": null,
            "created_at": now,
            "updated_at": now,
        },
    ]);

    let result = bridge::create_download_task_with_chapters(
        db_path.clone(),
        task.to_string(),
        chapters.to_string(),
    )
    .unwrap();

    let returned_task: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(returned_task["id"], "task1");

    let chapters_json =
        bridge::get_download_chapters(db_path.clone(), "task1".to_string()).unwrap();
    let chapters_result: Vec<Value> = serde_json::from_str(&chapters_json).unwrap();
    assert_eq!(chapters_result.len(), 2);
    assert_eq!(chapters_result[0]["id"], "ch1");
    assert_eq!(chapters_result[1]["id"], "ch2");
}

#[test]
fn test_get_download_task_by_book() {
    let (_dir, db_path) = setup_db();

    let result =
        bridge::get_download_task_by_book(db_path.clone(), "nonexistent".to_string()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert!(tasks.is_empty());

    ensure_book(&db_path, "book2", "Book Two");
    let now = 1700000001_i64;
    let task = json!({
        "id": "task2",
        "book_id": "book2",
        "book_name": "Book Two",
        "cover_url": null,
        "total_chapters": 5,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    });

    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task.to_string(),
        json!([]).to_string(),
    )
    .unwrap();

    let result =
        bridge::get_download_task_by_book(db_path.clone(), "book2".to_string()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], "task2");
    assert_eq!(tasks[0]["book_name"], "Book Two");
}

#[test]
fn test_delete_task_cleans_files() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db").to_string_lossy().to_string();
    core_storage::init_database(&db_path).unwrap();
    ensure_book(&db_path, "book3", "Book Three");

    let file_path = dir.path().join("test_file.txt");
    std::fs::write(&file_path, "test content").unwrap();
    let file_path_str = file_path.to_string_lossy().to_string();
    assert!(file_path.exists());

    let now = 1700000002_i64;
    let task = json!({
        "id": "task3",
        "book_id": "book3",
        "book_name": "Book Three",
        "cover_url": null,
        "total_chapters": 1,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    });
    let chapters = json!([{
        "id": "ch3",
        "task_id": "task3",
        "chapter_id": "bk3_ch1",
        "chapter_index": 0,
        "chapter_title": "Chapter 1",
        "status": 2,
        "file_path": file_path_str,
        "file_size": 12,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    }]);

    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task.to_string(),
        chapters.to_string(),
    )
    .unwrap();

    bridge::delete_download_task(db_path.clone(), "task3".to_string()).unwrap();
    assert!(!file_path.exists(), "File should be deleted along with the task");

    let result = bridge::get_download_tasks(db_path.clone()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_update_task_status_and_progress() {
    let (_dir, db_path) = setup_db();
    ensure_book(&db_path, "book4", "Book Four");
    let now = 1700000003_i64;

    let task = json!({
        "id": "task4",
        "book_id": "book4",
        "book_name": "Book Four",
        "cover_url": null,
        "total_chapters": 10,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    });

    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task.to_string(),
        json!([]).to_string(),
    )
    .unwrap();

    bridge::update_download_task_status(db_path.clone(), "task4".to_string(), 1, None).unwrap();
    bridge::update_download_progress(db_path.clone(), "task4".to_string(), 5, 1024).unwrap();

    let result = bridge::get_download_tasks(db_path.clone()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["status"], 1);
    assert_eq!(tasks[0]["downloaded_chapters"], 5);
    assert_eq!(tasks[0]["downloaded_size"], 1024);

    bridge::update_download_task_status(
        db_path.clone(),
        "task4".to_string(),
        4,
        Some("Network error".to_string()),
    )
    .unwrap();

    let result = bridge::get_download_tasks(db_path.clone()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(tasks[0]["status"], 4);
    assert_eq!(tasks[0]["error_message"], "Network error");
}

#[test]
fn test_get_all_download_tasks_order() {
    let (_dir, db_path) = setup_db();
    ensure_book(&db_path, "book_a", "Book A");
    ensure_book(&db_path, "book_b", "Book B");

    let result = bridge::get_download_tasks(db_path.clone()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert!(tasks.is_empty());

    let task1 = json!({
        "id": "task_a",
        "book_id": "book_a",
        "book_name": "Book A",
        "cover_url": null,
        "total_chapters": 3,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": 1700000004_i64,
        "updated_at": 1700000004_i64,
    });
    let task2 = json!({
        "id": "task_b",
        "book_id": "book_b",
        "book_name": "Book B",
        "cover_url": null,
        "total_chapters": 5,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": 1700000005_i64,
        "updated_at": 1700000005_i64,
    });

    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task1.to_string(),
        json!([]).to_string(),
    )
    .unwrap();
    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task2.to_string(),
        json!([]).to_string(),
    )
    .unwrap();

    let result = bridge::get_download_tasks(db_path.clone()).unwrap();
    let tasks: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(tasks.len(), 2);
    // Ordered by created_at DESC
    assert_eq!(tasks[0]["id"], "task_b");
    assert_eq!(tasks[1]["id"], "task_a");
}

#[test]
fn test_update_chapter_status() {
    let (_dir, db_path) = setup_db();
    ensure_book(&db_path, "book5", "Book Five");
    let now = 1700000006_i64;

    let task = json!({
        "id": "task5",
        "book_id": "book5",
        "book_name": "Book Five",
        "cover_url": null,
        "total_chapters": 1,
        "downloaded_chapters": 0,
        "status": 0,
        "total_size": 0,
        "downloaded_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    });
    let chapters = json!([{
        "id": "ch5",
        "task_id": "task5",
        "chapter_id": "bk5_ch1",
        "chapter_index": 0,
        "chapter_title": "Chapter 1",
        "status": 0,
        "file_path": null,
        "file_size": 0,
        "error_message": null,
        "created_at": now,
        "updated_at": now,
    }]);

    bridge::create_download_task_with_chapters(
        db_path.clone(),
        task.to_string(),
        chapters.to_string(),
    )
    .unwrap();

    bridge::update_download_chapter_status(
        db_path.clone(),
        "ch5".to_string(),
        2,
        Some("/path/to/file.txt".to_string()),
        1024,
        None,
    )
    .unwrap();

    let result =
        bridge::get_download_chapters(db_path.clone(), "task5".to_string()).unwrap();
    let chapters: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0]["status"], 2);
    assert_eq!(chapters[0]["file_path"], "/path/to/file.txt");
    assert_eq!(chapters[0]["file_size"], 1024);

    bridge::update_download_chapter_status(
        db_path.clone(),
        "ch5".to_string(),
        3,
        None::<String>,
        0,
        Some("Download failed".to_string()),
    )
    .unwrap();

    let result =
        bridge::get_download_chapters(db_path.clone(), "task5".to_string()).unwrap();
    let chapters: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(chapters[0]["status"], 3);
    assert_eq!(chapters[0]["error_message"], "Download failed");
}
