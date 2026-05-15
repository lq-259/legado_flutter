//! # core-parser - 格式解析引擎
//!
//! 负责解析各种电子书格式（TXT/EPUB/UMD/MOBI），对应原Legado的`modules/book/`和`help/book/`。
//! 使用scraper处理HTML/CSS选择，quick-xml处理XML/EPub结构，encoding_rs处理中文编码。

pub mod cleaner;
pub mod epub;
pub mod txt;
pub mod types;
pub mod umd;

// 重新导出主要类型，方便上层调用
pub use cleaner::{CleanerConfig, ContentCleaner};
pub use epub::EpubParser;
pub use txt::{TxtParser, TxtParserConfig};
pub use types::{BookMetadata, Chapter, EpubData};
pub use umd::UmdParser;

impl Chapter {
    pub fn new(title: &str, content: &str, index: usize) -> Self {
        Self {
            title: title.to_string(),
            content: content.to_string(),
            index,
            href: None,
        }
    }
}
