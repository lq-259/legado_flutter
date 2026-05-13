use std::path::Path;
use crate::HttpClient;

pub async fn download_to_file(
    client: &HttpClient,
    url: &str,
    output_path: &Path,
) -> Result<u64, Box<dyn std::error::Error>> {
    let response = client.get(url).await?;
    let bytes = response.bytes().await?;
    let size = bytes.len() as u64;
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(output_path, &bytes).await?;
    Ok(size)
}
