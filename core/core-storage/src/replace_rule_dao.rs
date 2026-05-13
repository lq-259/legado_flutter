//! # 替换规则 DAO (Data Access Object)
//!
//! 提供替换规则相关的数据库操作。
//! 对应原 Legado 的 ReplaceRule 实体操作 (data/entities/ReplaceRule.kt)

use rusqlite::{Connection, Result as SqlResult, params};
use tracing::{debug, info};
use uuid::Uuid;
use chrono::Utc;
use super::models::ReplaceRule;

/// 替换规则 DAO
pub struct ReplaceRuleDao<'a> {
    conn: &'a Connection,
}

impl<'a> ReplaceRuleDao<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入或更新替换规则
    pub fn upsert(&self, rule: &ReplaceRule) -> SqlResult<()> {
        debug!("插入/更新替换规则: {}", rule.name);

        self.conn.execute(
            "INSERT INTO replace_rules (
                id, name, pattern, replacement, enabled, scope, sort_number,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                pattern = excluded.pattern,
                replacement = excluded.replacement,
                enabled = excluded.enabled,
                scope = excluded.scope,
                sort_number = excluded.sort_number,
                updated_at = excluded.updated_at",
            params![
                rule.id,
                rule.name,
                rule.pattern,
                rule.replacement,
                rule.enabled as i32,
                rule.scope,
                rule.sort_number,
                rule.created_at,
                rule.updated_at,
            ],
        )?;

        Ok(())
    }

    /// 根据 ID 获取替换规则
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<ReplaceRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, pattern, replacement, enabled, scope, sort_number,
                    created_at, updated_at
             FROM replace_rules WHERE id = ?"
        )?;

        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(replace_rule_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 获取所有替换规则（按排序号）
    pub fn get_all(&self) -> SqlResult<Vec<ReplaceRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, pattern, replacement, enabled, scope, sort_number,
                    created_at, updated_at
             FROM replace_rules ORDER BY sort_number ASC"
        )?;

        let rows = stmt.query_map([], replace_rule_from_row)?;
        rows.collect()
    }

    /// 获取所有启用的替换规则
    pub fn get_enabled(&self) -> SqlResult<Vec<ReplaceRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, pattern, replacement, enabled, scope, sort_number,
                    created_at, updated_at
             FROM replace_rules WHERE enabled = 1 ORDER BY sort_number ASC"
        )?;

        let rows = stmt.query_map([], replace_rule_from_row)?;
        rows.collect()
    }

    /// 根据作用域获取替换规则
    pub fn get_by_scope(&self, scope: i32) -> SqlResult<Vec<ReplaceRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, pattern, replacement, enabled, scope, sort_number,
                    created_at, updated_at
             FROM replace_rules WHERE scope = ? ORDER BY sort_number ASC"
        )?;

        let rows = stmt.query_map(params![scope], replace_rule_from_row)?;
        rows.collect()
    }

    /// 删除替换规则
    pub fn delete(&self, id: &str) -> SqlResult<()> {
        info!("删除替换规则: {}", id);
        self.conn.execute("DELETE FROM replace_rules WHERE id = ?", params![id])?;
        Ok(())
    }

    /// 启用/禁用替换规则
    pub fn set_enabled(&self, id: &str, enabled: bool) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE replace_rules SET enabled = ?, updated_at = ? WHERE id = ?",
            params![enabled as i32, Utc::now().timestamp(), id],
        )?;
        Ok(())
    }

    /// 更新排序号
    pub fn update_order(&self, id: &str, sort_number: i32) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE replace_rules SET sort_number = ?, updated_at = ? WHERE id = ?",
            params![sort_number, Utc::now().timestamp(), id],
        )?;
        Ok(())
    }

    /// 创建新替换规则（便捷函数）
    pub fn create(
        &self,
        name: &str,
        pattern: &str,
        replacement: &str,
        scope: i32,
    ) -> SqlResult<ReplaceRule> {
        let now = Utc::now().timestamp();
        let rule = ReplaceRule {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            enabled: true,
            scope,
            sort_number: 0,
            created_at: now,
            updated_at: now,
        };

        self.upsert(&rule)?;
        Ok(rule)
    }
}

/// 从数据库行转换到 ReplaceRule 结构体
fn replace_rule_from_row(row: &rusqlite::Row) -> SqlResult<ReplaceRule> {
    Ok(ReplaceRule {
        id: row.get(0)?,
        name: row.get(1)?,
        pattern: row.get(2)?,
        replacement: row.get(3)?,
        enabled: row.get::<_, i32>(4)? != 0,
        scope: row.get(5)?,
        sort_number: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
