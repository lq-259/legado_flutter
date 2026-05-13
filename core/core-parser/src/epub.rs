//! # EPUB 格式解析模块
//!
//! 负责解析 EPUB 格式的电子书。
//! 对应原 Legado 的 EPUB 解析功能 (modules/book/EpubBook.kt)

use std::fs;
use std::path::Path;
use std::io::Read;
use zip::ZipArchive;
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;
use tracing::{debug, info, warn};

/// EPUB 解析器
pub struct EpubParser;

impl EpubParser {
    /// 创建新的 EPUB 解析器
    pub fn new() -> Self {
        Self
    }

    /// 解析 EPUB 文件
    /// 返回 (元数据, 章节列表)
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<(super::types::BookMetadata, Vec<super::types::Chapter>), String> {
        let path = path.as_ref();
        info!("开始解析 EPUB 文件: {:?}", path);
        
        let file = fs::File::open(path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| format!("解压 EPUB 失败: {}", e))?;
        
        // 1. 读取 container.xml 找到 content.opf 路径
        let content_opf_path = self.find_content_opf_path(&mut archive)?;
        debug!("找到 content.opf 路径: {}", content_opf_path);
        
        // 2. 解析 content.opf 获取元数据和目录
        let (metadata, manifest, spine, ncx_path) = self.parse_content_opf(&mut archive, &content_opf_path)?;
        debug!("元数据: {:?}", metadata);
        debug!("清单项数量: {}", manifest.len());
        debug!("阅读顺序数量: {}", spine.len());
        debug!("NCX 路径: {:?}", ncx_path);
        
        // 2.5 解析 NCX TOC（如果存在）
        let ncx_titles = if let Some(ref ncx) = ncx_path {
            self.parse_ncx(&mut archive, &content_opf_path, ncx)?
        } else {
            std::collections::HashMap::new()
        };
        debug!("NCX 标题映射: {:?}", ncx_titles);
        
        // 3. 按 spine 顺序解析章节内容
        let chapters = self.parse_chapters(&mut archive, &content_opf_path, &manifest, &spine, &ncx_titles)?;
        
        info!("EPUB 解析完成，共 {} 章", chapters.len());
        Ok((metadata, chapters))
    }

    /// 查找 content.opf 路径
    fn find_content_opf_path(&self, archive: &mut ZipArchive<fs::File>) -> Result<String, String> {
        // 读取 META-INF/container.xml
        let mut container_file = archive.by_name("META-INF/container.xml")
            .map_err(|e| format!("未找到 container.xml: {}", e))?;
        
        let mut container_xml = String::new();
        container_file.read_to_string(&mut container_xml)
            .map_err(|e| format!("读取 container.xml 失败: {}", e))?;
        
        // 解析 container.xml，提取 rootfile 路径
        let mut reader = XmlReader::from_reader(container_xml.as_bytes());
        let mut buf = Vec::new();
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                    if e.name().as_ref() == b"rootfile" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"full-path" {
                                return Ok(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("解析 container.xml 失败: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        
        Err("未找到 content.opf 路径".to_string())
    }

    /// 解析 content.opf 文件
    fn parse_content_opf(&self, archive: &mut ZipArchive<fs::File>, opf_path: &str) 
        -> Result<(super::types::BookMetadata, Vec<ManifestItem>, Vec<String>, Option<String>), String> {
        let mut opf_file = archive.by_name(opf_path)
            .map_err(|e| format!("未找到 content.opf: {}", e))?;
        
        let mut opf_content = String::new();
        opf_file.read_to_string(&mut opf_content)
            .map_err(|e| format!("读取 content.opf 失败: {}", e))?;
        
        let mut metadata = super::types::BookMetadata::default();
        let mut manifest = Vec::new();
        let mut spine = Vec::new();
        let mut ncx_path: Option<String> = None;
        
        let mut reader = XmlReader::from_reader(opf_content.as_bytes());
        let mut buf = Vec::new();
        let mut in_metadata = false;
        let mut current_tag = String::new();
        let mut cover_id: Option<String> = None;
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    match e.name().as_ref() {
                        b"metadata" => in_metadata = true,
                        b"manifest" => in_metadata = false,
                        b"spine" => in_metadata = false,
                        b"meta" if in_metadata => {
                            let mut meta_name = String::new();
                            let mut meta_content = String::new();
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"name" => meta_name = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"content" => meta_content = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"property" => {
                                        meta_name = String::from_utf8_lossy(&attr.value).to_string();
                                    }
                                    _ => {}
                                }
                            }
                            if meta_name == "cover" && !meta_content.is_empty() {
                                cover_id = Some(meta_content);
                            }
                        }
                        b"item" if !in_metadata => {
                            let mut id = String::new();
                            let mut href = String::new();
                            let mut media_type = String::new();
                            let mut properties = String::new();
                            
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"id" => id = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"href" => href = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"media-type" => media_type = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"properties" => properties = String::from_utf8_lossy(&attr.value).to_string(),
                                    _ => {}
                                }
                            }
                            
                            if properties.split_whitespace().any(|p| p == "cover-image") {
                                cover_id = Some(id.clone());
                            }
                            
                            // Detect NCX file (EPUB 2 TOC)
                            if media_type == "application/x-dtbncx+xml" {
                                ncx_path = Some(href.clone());
                            }
                            
                            manifest.push(ManifestItem { id, href, media_type });
                        }
                        b"itemref" if !in_metadata => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"idref" {
                                    spine.push(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                        _ => {
                            if in_metadata {
                                current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) if in_metadata => {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if (current_tag.ends_with(":title") || current_tag == "title") && metadata.title.is_none() {
                        metadata.title = Some(text);
                    } else if (current_tag.ends_with(":creator") || current_tag == "creator") && metadata.author.is_none() {
                        metadata.author = Some(text);
                    } else if (current_tag.ends_with(":language") || current_tag == "language") && metadata.language.is_none() {
                        metadata.language = Some(text);
                    } else if (current_tag.ends_with(":identifier") || current_tag == "identifier") && metadata.identifier.is_none() {
                        metadata.identifier = Some(text);
                    } else if (current_tag.ends_with(":description") || current_tag == "description") && metadata.description.is_none() {
                        metadata.description = Some(text);
                    } else if (current_tag.ends_with(":publisher") || current_tag == "publisher") && metadata.publisher.is_none() {
                        metadata.publisher = Some(text);
                    } else if (current_tag.ends_with(":date") || current_tag == "date") && metadata.date.is_none() {
                        metadata.date = Some(text);
                    } else if (current_tag.ends_with(":rights") || current_tag == "rights") && metadata.rights.is_none() {
                        metadata.rights = Some(text);
                    } else if current_tag.ends_with(":subject") || current_tag == "subject" {
                        metadata.subjects.push(text);
                    }
                }
                Ok(Event::Text(_)) => {}
                Ok(Event::End(ref e))
                    if e.name().as_ref() == b"metadata" => {
                        in_metadata = false;
                    }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("解析 content.opf 失败: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        
        if let Some(ref cid) = cover_id {
            if let Some(item) = manifest.iter().find(|i| i.id == *cid) {
                metadata.cover = Some(item.href.clone());
            }
        }

        Ok((metadata, manifest, spine, ncx_path))
    }

    /// 解析 NCX 目录文件 (EPUB 2 TOC)
    /// 返回 href -> title 的映射
    fn parse_ncx(&self, archive: &mut ZipArchive<fs::File>, opf_path: &str, ncx_href: &str) 
        -> Result<std::collections::HashMap<String, String>, String> {
        let base_path = std::path::Path::new(opf_path).parent().unwrap_or(std::path::Path::new(""));
        let ncx_path = base_path.join(ncx_href);
        let ncx_path_str = ncx_path.to_string_lossy().to_string();
        
        let mut ncx_file = archive.by_name(&ncx_path_str)
            .map_err(|e| format!("未找到 NCX 文件: {} ({})", ncx_path_str, e))?;
        
        let mut ncx_content = String::new();
        ncx_file.read_to_string(&mut ncx_content)
            .map_err(|e| format!("读取 NCX 文件失败: {}", e))?;
        
        let mut titles = std::collections::HashMap::new();
        let ncx_dir = std::path::Path::new(ncx_href)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        let mut reader = XmlReader::from_reader(ncx_content.as_bytes());
        let mut buf = Vec::new();
        let mut current_title: Option<String> = None;
        let mut in_nav_label = false;
        let mut in_text = false;
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    match e.name().as_ref() {
                        b"navLabel" => in_nav_label = true,
                        b"text" if in_nav_label => in_text = true,
                        b"content" => {
                            if let Some(ref title) = current_title {
                                Self::extract_ncx_src(e.attributes(), title, &mut titles, ncx_dir);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e))
                    if e.name().as_ref() == b"content" => {
                        if let Some(ref title) = current_title {
                            Self::extract_ncx_src(e.attributes(), title, &mut titles, ncx_dir);
                        }
                    }
                Ok(Event::Text(ref e)) if in_text => {
                    let text = e.unescape().unwrap_or_default().to_string();
                    current_title = Some(text.trim().to_string());
                }
                Ok(Event::End(ref e)) => {
                    match e.name().as_ref() {
                        b"navLabel" => in_nav_label = false,
                        b"text" => in_text = false,
                        b"navPoint" => {
                            current_title = None;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("解析 NCX 文件失败: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        
        Ok(titles)
    }

    fn extract_ncx_src(attrs: quick_xml::events::attributes::Attributes, title: &str,
                       titles: &mut std::collections::HashMap<String, String>,
                       ncx_dir: &str) {
        let mut src = String::new();
        for attr in attrs.flatten() {
            if attr.key.as_ref() == b"src" {
                src = String::from_utf8_lossy(&attr.value).to_string();
            }
        }
        if src.is_empty() {
            return;
        }
        let stripped = src.split('#').next().unwrap_or(&src).to_string();
        let title = title.to_string();

        let opf_relative = if stripped.starts_with('/') {
            Self::normalize_path(&stripped)
        } else {
            let combined = if ncx_dir.is_empty() {
                stripped.clone()
            } else {
                format!("{}/{}", ncx_dir, stripped)
            };
            Self::normalize_path(&combined)
        };
        if !opf_relative.is_empty() {
            titles.entry(opf_relative.clone()).or_insert_with(|| title.clone());
        }

        if let Some(name) = std::path::Path::new(&stripped).file_name() {
            let name = name.to_string_lossy().to_string();
            if name != stripped && name != opf_relative {
                titles.entry(name).or_insert(title);
            }
        }
    }

    fn normalize_path(path: &str) -> String {
        let mut stack: Vec<&str> = Vec::new();
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => { stack.pop(); }
                _ => stack.push(component),
            }
        }
        stack.join("/")
    }

    /// 解析章节内容
    fn parse_chapters(&self, archive: &mut ZipArchive<fs::File>, opf_path: &str, 
                       manifest: &[ManifestItem], spine: &[String],
                       ncx_titles: &std::collections::HashMap<String, String>) -> Result<Vec<super::types::Chapter>, String> {
        let mut chapters = Vec::new();
        let base_path = std::path::Path::new(opf_path).parent().unwrap_or(std::path::Path::new(""));
        
        for (index, itemref) in spine.iter().enumerate() {
            if let Some(item) = manifest.iter().find(|i| i.id == *itemref) {
                let file_path = base_path.join(&item.href);
                let file_path_str = file_path.to_string_lossy().to_string();
                
                if let Ok(mut chapter_file) = archive.by_name(&file_path_str) {
                    let mut content = String::new();
                    chapter_file.read_to_string(&mut content)
                        .map_err(|e| format!("读取章节文件失败: {}", e))?;
                    
                    let title = ncx_titles.get(&item.href)
                        .cloned()
                        .or_else(|| self.extract_title_from_html(&content))
                        .unwrap_or_else(|| format!("第 {} 章", index + 1));
                    let cleaned_content = self.clean_html_content(&content);
                    
                    chapters.push(super::types::Chapter {
                        title,
                        content: cleaned_content,
                        index,
                        href: Some(item.href.clone()),
                    });
                } else {
                    warn!("未找到章节文件: {}", file_path_str);
                }
            }
        }
        
        Ok(chapters)
    }

    /// 从 HTML 中提取标题
    fn extract_title_from_html(&self, html: &str) -> Option<String> {
        // 简化实现：查找 <title> 或第一个 <h1>
        let re_title = regex::Regex::new(r"<title>(.*?)</title>").ok()?;
        if let Some(cap) = re_title.captures(html) {
            return Some(cap[1].trim().to_string());
        }
        
        let re_h1 = regex::Regex::new(r"<h1[^>]*>(.*?)</h1>").ok()?;
        if let Some(cap) = re_h1.captures(html) {
            return Some(cap[1].trim().to_string());
        }
        
        None
    }

    /// 清洗 HTML 内容，提取纯文本
    fn clean_html_content(&self, html: &str) -> String {
        // 简化实现：移除 HTML 标签
        let re = regex::Regex::new(r"<[^>]+>").unwrap();
        let text = re.replace_all(html, " ");
        
        // 解码 HTML 实体
        let text = text.replace("&nbsp;", " ")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&");
        
        text.trim().to_string()
    }
}

/// Manifest 项
#[derive(Debug, Clone)]
struct ManifestItem {
    id: String,
    href: String,
    #[allow(dead_code)]
    media_type: String,
}

impl Default for EpubParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：快速解析 EPUB 文件
pub fn parse_epub_file<P: AsRef<Path>>(path: P) -> Result<(super::types::BookMetadata, Vec<super::types::Chapter>), String> {
    let parser = EpubParser::new();
    parser.parse_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    fn make_minimal_epub(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#).unwrap();

        zip.start_file("OEBPS/content.opf", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="book-id">
  <metadata>
    <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Test EPUB</dc:title>
    <dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">Test Author</dc:creator>
    <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="book-id">test-001</dc:identifier>
  </metadata>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>"#).unwrap();

        zip.start_file("OEBPS/chapter1.xhtml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter One</title></head>
<body><h1>Chapter One</h1><p>This is the content of chapter one.</p></body>
</html>"#).unwrap();

        zip.finish().unwrap();
    }

    #[test]
    fn test_epub_parse_minimal() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_minimal.epub");
        make_minimal_epub(&path);

        let (metadata, chapters) = EpubParser::new().parse_file(&path).unwrap();
        assert_eq!(metadata.title.as_deref(), Some("Test EPUB"));
        assert_eq!(metadata.author.as_deref(), Some("Test Author"));
        assert_eq!(chapters.len(), 1);
        assert!(chapters[0].title.contains("Chapter One"));
        assert!(chapters[0].content.contains("content of chapter one"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_epub_invalid_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_invalid.epub");
        std::fs::write(&path, b"not a zip file").unwrap();

        let result = EpubParser::new().parse_file(&path);
        assert!(result.is_err());

        std::fs::remove_file(&path).ok();
    }

    fn make_epub_with_ncx(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#).unwrap();

        zip.start_file("OEBPS/content.opf", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="book-id">
  <metadata>
    <dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">NCX Test EPUB</dc:title>
    <dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">NCX Author</dc:creator>
    <dc:language xmlns:dc="http://purl.org/dc/elements/1.1/">en</dc:language>
    <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="book-id">ncx-001</dc:identifier>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
  </spine>
</package>"#).unwrap();

        zip.start_file("OEBPS/toc.ncx", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx version="2005-1" xmlns="http://www.daisy.org/z3986/2005/ncx/">
  <navMap>
    <navPoint id="nav1" playOrder="1">
      <navLabel><text>Chapter From NCX</text></navLabel>
      <content src="chapter1.xhtml"/>
    </navPoint>
    <navPoint id="nav2" playOrder="2">
      <navLabel><text>Second Chapter From NCX</text></navLabel>
      <content src="chapter2.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#).unwrap();

        zip.start_file("OEBPS/chapter1.xhtml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Raw HTML Title 1</title></head>
<body><h1>Raw H1 1</h1><p>First chapter content.</p></body>
</html>"#).unwrap();

        zip.start_file("OEBPS/chapter2.xhtml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Raw HTML Title 2</title></head>
<body><h1>Raw H1 2</h1><p>Second chapter content.</p></body>
</html>"#).unwrap();

        zip.finish().unwrap();
    }

    #[test]
    fn test_epub_parse_ncx_titles() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_ncx.epub");
        make_epub_with_ncx(&path);

        let (_metadata, chapters) = EpubParser::new().parse_file(&path).unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Chapter From NCX");
        assert!(chapters[0].content.contains("First chapter content"));
        assert_eq!(chapters[1].title, "Second Chapter From NCX");
        assert!(chapters[1].content.contains("Second chapter content"));

        std::fs::remove_file(&path).ok();
    }
}
