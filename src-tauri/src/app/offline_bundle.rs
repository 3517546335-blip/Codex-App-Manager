use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use codex_mac_engine::{Appcast, AppcastItem, Delta, Enclosure};
use serde::Deserialize;

use crate::domain::manifest::MirrorEndpoints;
use crate::errors::AppError;

static OFFLINE_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacOfflineMetadata {
    architecture: String,
    build: u64,
    short_version: String,
    minimum_system_version: Option<String>,
    pub_date: Option<String>,
    package_file: String,
    content_length: u64,
    ed_signature: String,
}

pub fn initialize(root: PathBuf) -> Result<(), AppError> {
    if !root.is_dir() {
        return Err(AppError::Internal(format!(
            "内置离线资源目录不存在：{}",
            root.display()
        )));
    }
    OFFLINE_ROOT.set(root).map_err(|existing| {
        AppError::Internal(format!(
            "内置离线资源目录已初始化：{}",
            existing.display()
        ))
    })
}

pub fn is_initialized() -> bool {
    OFFLINE_ROOT.get().is_some()
}

fn root() -> Result<&'static Path, AppError> {
    OFFLINE_ROOT
        .get()
        .map(PathBuf::as_path)
        .ok_or_else(|| AppError::Internal("内置离线资源尚未初始化".to_string()))
}

fn directory_file_url(path: &Path) -> Result<String, AppError> {
    url::Url::from_directory_path(path)
        .map(|url| url.to_string())
        .map_err(|_| AppError::Internal(format!("无法生成离线目录 URL：{}", path.display())))
}

fn file_url(path: &Path) -> Result<String, AppError> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| AppError::Internal(format!("无法生成离线文件 URL：{}", path.display())))
}

pub fn windows_endpoints() -> Result<MirrorEndpoints, AppError> {
    let windows = root()?.join("windows");
    let required = [
        windows.join("latest/manifest"),
        windows.join("latest/checksums"),
        windows.join("latest/win-x64"),
    ];
    if let Some(missing) = required.iter().find(|path| !path.is_file()) {
        return Err(AppError::Internal(format!(
            "Windows 内置离线资源不完整：{}",
            missing.display()
        )));
    }
    Ok(MirrorEndpoints::from_base_url(
        directory_file_url(&windows)?.trim_end_matches('/'),
    ))
}

fn normalize_macos_architecture(architecture: &str) -> Option<&'static str> {
    match architecture {
        "aarch64" | "arm64" => Some("arm64"),
        "x86_64" | "x64" => Some("x86_64"),
        _ => None,
    }
}

pub fn mac_appcast(architecture: &str) -> Result<(String, Appcast), AppError> {
    let macos = root()?.join("macos");
    let metadata_path = macos.join("metadata.json");
    let metadata: MacOfflineMetadata = serde_json::from_slice(
        &std::fs::read(&metadata_path).map_err(|e| {
            AppError::Internal(format!(
                "读取 macOS 离线元数据失败 {}：{e}",
                metadata_path.display()
            ))
        })?,
    )
    .map_err(|e| AppError::Internal(format!("解析 macOS 离线元数据失败：{e}")))?;

    let expected_arch = normalize_macos_architecture(architecture).ok_or_else(|| {
        AppError::Internal(format!("不支持的 macOS 离线架构：{architecture}"))
    })?;
    if metadata.architecture != expected_arch {
        return Err(AppError::Internal(format!(
            "macOS 内置离线包架构不匹配：需要 {expected_arch}，实际 {}",
            metadata.architecture
        )));
    }
    if metadata.ed_signature.trim().is_empty() || metadata.content_length == 0 {
        return Err(AppError::Internal(
            "macOS 内置离线包缺少签名或长度".to_string(),
        ));
    }

    let package_path = macos.join(&metadata.package_file);
    let actual_length = std::fs::metadata(&package_path)
        .map_err(|e| {
            AppError::Internal(format!(
                "读取 macOS 内置离线包失败 {}：{e}",
                package_path.display()
            ))
        })?
        .len();
    if actual_length != metadata.content_length {
        return Err(AppError::Internal(format!(
            "macOS 内置离线包长度不匹配：{actual_length} != {}",
            metadata.content_length
        )));
    }

    let source = format!("offline://embedded/macos/{expected_arch}");
    let appcast = Appcast {
        items: vec![AppcastItem {
            build: metadata.build,
            short_version: metadata.short_version,
            minimum_system_version: metadata.minimum_system_version,
            pub_date: metadata.pub_date,
            full: Enclosure {
                url: file_url(&package_path)?,
                length: metadata.content_length,
                ed_signature: Some(metadata.ed_signature),
            },
            deltas: Vec::<Delta>::new(),
        }],
    };
    Ok((source, appcast))
}

#[cfg(test)]
mod tests {
    use super::normalize_macos_architecture;

    #[test]
    fn normalizes_supported_macos_architectures() {
        for (raw, expected) in [
            ("aarch64", "arm64"),
            ("arm64", "arm64"),
            ("x86_64", "x86_64"),
            ("x64", "x86_64"),
        ] {
            assert_eq!(normalize_macos_architecture(raw), Some(expected));
        }
        assert_eq!(normalize_macos_architecture("armv7"), None);
    }
}
