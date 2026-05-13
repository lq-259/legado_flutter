//! # UMD 格式解析模块
//!
//! 负责解析 UMD (Universal Mobile Document) 格式的电子书。
//! 对应原 Legado 的 UMD 解析功能。

use std::fs;
use std::path::Path;
use tracing::{debug, info};
use std::io::{Read, Seek, SeekFrom};
use flate2::read::ZlibDecoder;

const MAX_CHAPTER_SIZE: usize = 50 * 1024 * 1024; // 50MB 章节大小上限
const MAX_CHAPTER_COUNT: u32 = 100_000; // 章节数量上限

/// UMD 解析器
pub struct UmdParser;

impl UmdParser {
    /// 创建新的 UMD 解析器
    pub fn new() -> Self {
        Self
    }

    /// 解析 UMD 文件
    /// 返回章节列表
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<super::types::Chapter>, String> {
        let path = path.as_ref();
        info!("开始解析 UMD 文件: {:?}", path);
        
        let mut file = fs::File::open(path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        
        // 1. 验证 UMD 文件头
        let header = self.read_header(&mut file)?;
        debug!("UMD 文件头: v{} enc:{} ch:{} clen:{}", header.version, header.encrypt_info, header.chapter_count, header.content_length);
        
        if header.magic != *b"UMD\x1A" {
            return Err("不是有效的 UMD 文件".to_string());
        }

        if header.chapter_count > MAX_CHAPTER_COUNT {
            return Err(format!("章节数量 {} 超过上限 {}", header.chapter_count, MAX_CHAPTER_COUNT));
        }
        
        // 2. 读取章节索引
        let chapter_offsets = self.read_chapter_index(&mut file, header.chapter_count)?;
        debug!("章节数量: {}, 偏移量列表: {:?}", chapter_offsets.len(), chapter_offsets);

        // 获取文件长度用于边界校验
        let file_len = file.metadata()
            .map_err(|e| format!("读取文件元数据失败: {}", e))?
            .len();

        // 校验章节偏移量
        for &offset in &chapter_offsets {
            if offset > file_len {
                return Err(format!("章节偏移 {} 超出文件长度 {}", offset, file_len));
            }
        }
        for w in chapter_offsets.windows(2) {
            if w[1] <= w[0] {
                return Err(format!("章节偏移非单调递增: {} <= {}", w[1], w[0]));
            }
        }

        // 3. 读取章节内容
        let mut chapters = Vec::new();
        for (index, &offset) in chapter_offsets.iter().enumerate() {
            let end_offset = chapter_offsets.get(index + 1).copied();
            let (umd_title, content) = self.read_chapter_content(&mut file, offset, end_offset, file_len)?;

            chapters.push(super::types::Chapter {
                title: umd_title.unwrap_or_else(|| format!("第 {} 章", index + 1)),
                content,
                index,
                href: None,
            });
        }
        
        info!("UMD 解析完成，共 {} 章", chapters.len());
        Ok(chapters)
    }

    /// 读取 UMD 文件头
    fn read_header(&self, file: &mut fs::File) -> Result<UmdHeader, String> {
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(|e| format!("读取魔术数字失败: {}", e))?;
        
        // 验证魔术数字 (UMD\x1A)
        if &magic[0..4] != b"UMD\x1A" {
            return Err("无效的 UMD 文件".to_string());
        }
        
        // 读取版本号 (2字节)
        let mut version_buf = [0u8; 2];
        file.read_exact(&mut version_buf)
            .map_err(|e| format!("读取版本号失败: {}", e))?;
        let version = u16::from_le_bytes(version_buf);
        
        // 读取加密信息 (1字节)
        let mut encrypt_buf = [0u8; 1];
        file.read_exact(&mut encrypt_buf)
            .map_err(|e| format!("读取加密信息失败: {}", e))?;
        
        // 读取章节数量 (4字节)
        let mut chapter_count_buf = [0u8; 4];
        file.read_exact(&mut chapter_count_buf)
            .map_err(|e| format!("读取章节数量失败: {}", e))?;
        let chapter_count = u32::from_le_bytes(chapter_count_buf);
        
        // 读取内容长度 (4字节)
        let mut content_length_buf = [0u8; 4];
        file.read_exact(&mut content_length_buf)
            .map_err(|e| format!("读取内容长度失败: {}", e))?;
        let content_length = u32::from_le_bytes(content_length_buf);
        
        Ok(UmdHeader {
            magic,
            version,
            encrypt_info: encrypt_buf[0],
            chapter_count,
            content_length,
        })
    }

    /// 读取章节索引
    fn read_chapter_index(&self, file: &mut fs::File, chapter_count: u32) -> Result<Vec<u64>, String> {
        let mut offsets = Vec::new();
        
        for _ in 0..chapter_count {
            let mut offset_buf = [0u8; 8];  // UMD 使用 8 字节偏移
            file.read_exact(&mut offset_buf)
                .map_err(|e| format!("读取章节索引失败: 预期 {} 个偏移量，实际读取失败: {}", chapter_count, e))?;
            let offset = u64::from_le_bytes(offset_buf);
            offsets.push(offset);
        }
        
        Ok(offsets)
    }

    /// 读取章节内容（返回标题和内容）
    fn read_chapter_content(&self, file: &mut fs::File, offset: u64, end_offset: Option<u64>, file_len: u64) -> Result<(Option<String>, String), String> {
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("定位章节失败: {}", e))?;

        let content_buf = if let Some(end) = end_offset {
            if end <= offset {
                return Err(format!("章节偏移无效: end={} <= offset={}", end, offset));
            }
            let size = (end - offset) as usize;
            if size > MAX_CHAPTER_SIZE {
                return Err(format!("章节大小 {} 字节超过上限 {}", size, MAX_CHAPTER_SIZE));
            }
            let mut buf = vec![0u8; size];
            file.read_exact(&mut buf)
                .map_err(|e| format!("读取章节内容失败: {}", e))?;
            buf
        } else {
            let remaining = (file_len - offset) as usize;
            if remaining > MAX_CHAPTER_SIZE {
                return Err(format!("最后一章大小 {} 字节超过上限 {}", remaining, MAX_CHAPTER_SIZE));
            }
            let mut buf = vec![0u8; remaining];
            file.read_exact(&mut buf)
                .map_err(|e| format!("读取章节内容失败: {}", e))?;
            buf
        };

        let (title, raw_content) = Self::parse_umd_tags(&content_buf);
        let cleaned = self.clean_content(&raw_content);
        Ok((title, cleaned))
    }

    /// 解析 UMD 章节内部的 tag 结构
    /// tag 类型: 0x01=标题, 0x02=内容, 0x83=zlib 压缩内容
    fn parse_umd_tags(data: &[u8]) -> (Option<String>, String) {
        let mut title: Option<String> = None;
        let mut content = String::new();
        let mut pos = 0;

        while pos + 1 < data.len() {
            let tag_type = data[pos];
            pos += 1;

            match tag_type {
                0x01 => {
                    if pos + 2 > data.len() { break; }
                    let len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += 2;
                    if pos + len > data.len() { break; }
                    title = Some(String::from_utf8_lossy(&data[pos..pos + len]).into_owned());
                    pos += len;
                }
                0x02 => {
                    if pos + 4 > data.len() { break; }
                    let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                    pos += 4;
                    if pos + len > data.len() { break; }
                    content = String::from_utf8_lossy(&data[pos..pos + len]).into_owned();
                    pos += len;
                }
                0x83 => {
                    if pos + 4 > data.len() { break; }
                    let uncomp_len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                    pos += 4;
                    if uncomp_len > MAX_CHAPTER_SIZE { break; }
                    let mut decoder = ZlibDecoder::new(&data[pos..]);
                    let mut decompressed = Vec::with_capacity(uncomp_len);
                    if decoder.read_to_end(&mut decompressed).is_ok() {
                        content = String::from_utf8_lossy(&decompressed).into_owned();
                    }
                    break;
                }
                _ => {
                    let remaining = String::from_utf8_lossy(&data[pos - 1..]).into_owned();
                    content = if content.is_empty() { remaining } else { format!("{}{}", content, remaining) };
                    break;
                }
            }
        }

        if content.is_empty() {
            let fallback = String::from_utf8_lossy(data).into_owned();
            let (fb, _, _) = encoding_rs::GBK.decode(data);
            content = if !fallback.contains('\u{FFFD}') { fallback } else { fb.into_owned() };
        }

        (title, content)
    }

    /// 清洗内容
    fn clean_content(&self, content: &str) -> String {
        // 移除多余空白和特殊字符
        let re = regex::Regex::new(r"\s+").unwrap();
        let cleaned = re.replace_all(content, " ");
        cleaned.trim().to_string()
    }
}

impl Default for UmdParser {
    fn default() -> Self {
        Self::new()
    }
}

/// UMD 文件头信息
#[derive(Debug, Clone)]
struct UmdHeader {
    magic: [u8; 4],
    version: u16,
    encrypt_info: u8,
    chapter_count: u32,
    content_length: u32,
}

/// 便捷函数：快速解析 UMD 文件
pub fn parse_umd_file<P: AsRef<Path>>(path: P) -> Result<Vec<super::types::Chapter>, String> {
    let parser = UmdParser::new();
    parser.parse_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_temp_file(name: &str, data: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(data).unwrap();
        path
    }

    fn umd_header(chapter_count: u32) -> Vec<u8> {
        let mut h = vec![b'U', b'M', b'D', 0x1A]; // magic
        h.extend_from_slice(&[0x00, 0x00]); // version
        h.push(0); // encrypt
        h.extend_from_slice(&chapter_count.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes()); // content_length
        h
    }

    #[test]
    fn test_invalid_magic() {
        let path = make_temp_file("umd_bad_magic.umd", b"BAD\x1A");
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_chapter_count_exceeded() {
        let mut data = umd_header(MAX_CHAPTER_COUNT + 1);
        data.extend_from_slice(&[0u8; 8]); // one offset
        let path = make_temp_file("umd_too_many.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("超过上限"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_index_truncated() {
        let mut data = umd_header(3); // declares 3 chapters
        data.extend_from_slice(&1u64.to_le_bytes()); // only 1 offset
        let path = make_temp_file("umd_trunc.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_offset_out_of_bounds() {
        let mut data = umd_header(1);
        data.extend_from_slice(&999999u64.to_le_bytes()); // offset beyond file
        let path = make_temp_file("umd_oob.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_non_monotonic_offsets() {
        let mut data = umd_header(2);
        data.extend_from_slice(&200u64.to_le_bytes());
        data.extend_from_slice(&100u64.to_le_bytes()); // non-monotonic
        data.extend_from_slice(&[0u8; 300]); // content
        let path = make_temp_file("umd_nonmon.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("非单调"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_last_chapter_too_large() {
        let mut data = umd_header(1);
        // 偏移量指向文件开头位置
        data.extend_from_slice(&(umd_header(1).len() as u64 + 8).to_le_bytes());
        // 超出 MAX_CHAPTER_SIZE 的内容
        data.extend_from_slice(&vec![0u8; MAX_CHAPTER_SIZE + 1]);
        let path = make_temp_file("umd_last_oversize.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("超过上限"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_mid_chapter_too_large() {
        let header_len = umd_header(2).len() as u64;
        let mut data = umd_header(2);
        // 第一个 offset 指向内容开始
        data.extend_from_slice(&(header_len + 16).to_le_bytes());
        // 第二个 offset 很远，使第一章大小 > MAX_CHAPTER_SIZE
        data.extend_from_slice(&(header_len + 16 + MAX_CHAPTER_SIZE as u64 + 1).to_le_bytes());
        data.extend_from_slice(&vec![0u8; MAX_CHAPTER_SIZE + 2]);
        let path = make_temp_file("umd_mid_oversize.umd", &data);
        let result = UmdParser::new().parse_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("超过上限"));
        let _ = std::fs::remove_file(&path);
    }
}
