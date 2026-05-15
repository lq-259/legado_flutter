use crate::HttpClient;
use std::path::Path;

pub async fn download_to_file(
    client: &HttpClient,
    url: &str,
    output_path: &Path,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut response = client.get(url).await?;
    let max = client.config().max_response_bytes as usize;
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_name = format!(
        ".{}.{}.tmp",
        output_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("download"),
        uuid::Uuid::new_v4()
    );
    let tmp_path = output_path.with_file_name(tmp_name);
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut total: u64 = 0;
    let mut result: Result<(), Box<dyn std::error::Error>> = Ok(());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("下载失败: {}", e))?
    {
        let len = chunk.len() as u64;
        if total + len > max as u64 {
            result = Err(format!("下载文件超过上限 {} 字节", max).into());
            break;
        }
        if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await {
            result = Err(e.into());
            break;
        }
        total += len;
    }
    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e);
    }
    tokio::fs::rename(&tmp_path, output_path).await?;
    Ok(total)
}
