use crate::error::ApiError;

pub fn open_db(db_path: &str) -> Result<rusqlite::Connection, ApiError> {
    core_storage::database::get_connection(db_path).map_err(|e| ApiError::Database(e.to_string()))
}

pub fn storage_to_core_source(
    s: &core_storage::models::BookSource,
) -> Result<core_source::types::BookSource, ApiError> {
    let rule_search = s
        .rule_search
        .as_deref()
        .map(|r| serde_json::from_str(r))
        .transpose()
        .map_err(|e| ApiError::Parse(format!("解析 rule_search 失败: {}", e)))?;
    let rule_book_info = s
        .rule_book_info
        .as_deref()
        .map(|r| serde_json::from_str(r))
        .transpose()
        .map_err(|e| ApiError::Parse(format!("解析 rule_book_info 失败: {}", e)))?;
    let rule_toc = s
        .rule_toc
        .as_deref()
        .map(|r| serde_json::from_str(r))
        .transpose()
        .map_err(|e| ApiError::Parse(format!("解析 rule_toc 失败: {}", e)))?;
    let rule_content = s
        .rule_content
        .as_deref()
        .map(|r| serde_json::from_str(r))
        .transpose()
        .map_err(|e| ApiError::Parse(format!("解析 rule_content 失败: {}", e)))?;

    Ok(core_source::types::BookSource {
        id: s.id.clone(),
        name: s.name.clone(),
        url: s.url.clone(),
        source_type: s.source_type,
        enabled: s.enabled,
        group_name: s.group_name.clone(),
        custom_order: s.custom_order,
        weight: s.weight,
        rule_search,
        rule_book_info,
        rule_toc,
        rule_content,
        login_url: s.login_url.clone(),
        header: s.header.clone(),
        js_lib: s.js_lib.clone(),
        rule_explore: s
            .rule_explore
            .as_deref()
            .map(|r| serde_json::from_str(r))
            .transpose()
            .map_err(|e| ApiError::Parse(format!("解析 rule_explore 失败: {}", e)))?,
        explore_url: s.explore_url.clone(),
        book_url_pattern: s.book_url_pattern.clone(),
        enabled_explore: s.enabled_explore,
        last_update_time: s.last_update_time,
        book_source_comment: s.book_source_comment.clone(),
        created_at: s.created_at,
        updated_at: s.updated_at,
    })
}
