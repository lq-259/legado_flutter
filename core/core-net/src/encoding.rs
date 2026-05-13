//! # 编码处理模块
//!
//! 提供自动检测和转换文本编码的功能。
//! 对应原 Legado 的中文编码处理。

use encoding_rs::{Encoding, UTF_8, GB18030};
use std::fs;
use std::path::Path;

/// 自动检测并解码字节序列
pub fn detect_and_decode(bytes: &[u8]) -> (String, &'static Encoding) {
    // 1. 检查 BOM
    if bytes.len() >= 3 && bytes[0..3] == [0xEF, 0xBB, 0xBF] {
        let (text, _, _) = UTF_8.decode(&bytes[3..]);
        return (text.into_owned(), UTF_8);
    }
    
    // 2. 统计检测常见中文编码
    let mut gb_count = 0;
    let mut utf8_valid = true;
    
    for window in bytes.windows(3) {
        // GBK/GB18030 双字节检测
        if window[0] >= 0x81 && window[0] <= 0xFE
            && window.len() > 1 && window[1] >= 0x40 && window[1] <= 0xFE {
                gb_count += 1;
            }
        
        // UTF-8 有效性检查
        if window[0] & 0x80 != 0
            && window[0] & 0xE0 == 0xC0
                && window.len() > 1 && window[1] & 0xC0 != 0x80 {
                    utf8_valid = false;
                }
    }
    
    // 3. 根据统计结果选择编码
    let encoding = if gb_count > 10 {
        GB18030
    } else if utf8_valid {
        UTF_8
    } else {
        GB18030 // 默认假设中文
    };
    
    let (text, _, _) = encoding.decode(bytes);
    (text.into_owned(), encoding)
}

/// 从文件读取并自动检测编码
pub fn read_file_with_encoding<P: AsRef<Path>>(path: P) -> Result<(String, String), String> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    
    let (text, encoding) = detect_and_decode(&bytes);
    Ok((text, encoding.name().to_string()))
}

/// 从响应头解析编码
pub fn parse_charset_from_content_type(content_type: &str) -> Option<String> {
    content_type
        .split(';')
        .find(|s| s.trim().starts_with("charset="))
        .map(|s| {
            s.trim()
                .replace("charset=", "")
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_utf8() {
        let bytes = "Hello 世界".as_bytes();
        let (text, encoding) = detect_and_decode(bytes);
        assert_eq!(encoding, UTF_8);
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_detect_gbk() {
        // 模拟 GBK 编码数据
        let gbk_bytes = b"\xc4\xe3\xba\xc3"; // "你好" 的 GBK 编码
        let (_text, encoding) = detect_and_decode(gbk_bytes);
        // 应该检测到是中文编码
        assert!(encoding == GB18030);
    }

    #[test]
    fn test_parse_charset() {
        assert_eq!(
            parse_charset_from_content_type("text/html; charset=utf-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            parse_charset_from_content_type("text/html"),
            None
        );
    }
}
