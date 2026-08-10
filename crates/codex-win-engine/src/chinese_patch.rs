//! Native Windows Codex Chinese menu patch.
//!
//! The official package already ships Simplified Chinese renderer resources,
//! but its Electron application and dynamically built tray menus still expose
//! English labels. This module injects the same runtime menu translator used by
//! YoRHaForever/codex-desktop-chinese-patch, augments the bundled zh-CN native
//! locale, and rewrites ASAR metadata without requiring Node.js on the user's PC.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{find_codex_app_asar, EngineError, InstalledWindowsCodex};

const MAX_ASAR_HEADER_BYTES: u32 = 32 * 1024 * 1024;
const MAX_CANDIDATE_JS_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_INTEGRITY_BLOCK_SIZE: usize = 4 * 1024 * 1024;
const PATCH_SCHEMA_VERSION: u32 = 2;
const MENU_PATCH_MARKER: &str = "__codexChineseMenuPatchV2";
const I18N_PATCH_MARKER: &str = "__codexForceI18nV1";
const I18N_GATE_SIGNATURE: &str = "get(`enable_i18n`,!1)";
const I18N_SOURCE_SIGNATURE: &str = "get(`locale_source`,`IDE`)";
const NATIVE_LOCALE_PATH: &str = "native-menu-locales/zh-CN.json";
const MENU_TRANSLATIONS_JSON: &str = include_str!("../resources/menu-translations-zh-CN.json");
const NATIVE_STRINGS_JSON: &str = include_str!("../resources/native-strings-zh-CN.json");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChinesePatchReport {
    /// `patched` | `already-patched`
    pub status: String,
    pub message: String,
    pub app_asar_path: String,
    pub backup_path: Option<String>,
    pub restart_required: bool,
}

#[derive(Debug)]
struct AsarArchive {
    header: Value,
    header_json: Vec<u8>,
    data_offset: u64,
}

#[derive(Debug, Clone)]
struct PackedFile {
    path: String,
    offset: u64,
    size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchState {
    NeedsPatch,
    AlreadyPatched,
}

#[derive(Debug)]
struct PatchAnalysis {
    state: PatchState,
    patches: Vec<FilePatch>,
}

#[derive(Debug)]
struct FilePatch {
    target: PackedFile,
    patched_bytes: Vec<u8>,
}

fn install_error(context: impl std::fmt::Display) -> EngineError {
    EngineError::Install(context.to_string())
}

fn read_u32(bytes: &[u8]) -> Result<u32, EngineError> {
    bytes
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| install_error("invalid ASAR integer"))
}

fn read_asar(path: &Path) -> Result<AsarArchive, EngineError> {
    let mut file = File::open(path)
        .map_err(|e| install_error(format!("open app.asar {}: {e}", path.display())))?;
    let archive_size = file
        .metadata()
        .map_err(|e| install_error(format!("read app.asar metadata: {e}")))?
        .len();
    let mut prefix = [0_u8; 8];
    file.read_exact(&mut prefix)
        .map_err(|e| install_error(format!("read app.asar header prefix: {e}")))?;
    let size_pickle_payload = read_u32(&prefix[..4])?;
    let header_size = read_u32(&prefix[4..])?;
    if size_pickle_payload != 4
        || !(8..=MAX_ASAR_HEADER_BYTES).contains(&header_size)
        || u64::from(header_size) > archive_size.saturating_sub(8)
    {
        return Err(install_error("unsupported or corrupt app.asar header"));
    }

    let mut header_pickle = vec![0_u8; header_size as usize];
    file.read_exact(&mut header_pickle)
        .map_err(|e| install_error(format!("read app.asar header: {e}")))?;
    let payload_size = read_u32(&header_pickle[..4])? as usize;
    let json_size = read_u32(&header_pickle[4..8])? as usize;
    if payload_size > header_pickle.len().saturating_sub(4)
        || json_size > payload_size.saturating_sub(4)
        || 8_usize
            .checked_add(json_size)
            .is_none_or(|end| end > header_pickle.len())
    {
        return Err(install_error("invalid app.asar header pickle"));
    }
    let header_json = header_pickle[8..8 + json_size].to_vec();
    let header: Value = serde_json::from_slice(&header_json)
        .map_err(|e| install_error(format!("parse app.asar header JSON: {e}")))?;
    if !header.get("files").is_some_and(Value::is_object) {
        return Err(install_error("app.asar header has no file tree"));
    }
    Ok(AsarArchive {
        header,
        header_json,
        data_offset: 8 + u64::from(header_size),
    })
}

fn packed_entry(entry: &Value, path: &str) -> Result<Option<PackedFile>, EngineError> {
    if entry.get("unpacked").and_then(Value::as_bool) == Some(true) {
        return Ok(None);
    }
    let Some(offset) = entry.get("offset").and_then(Value::as_str) else {
        return Ok(None);
    };
    let offset = offset
        .parse::<u64>()
        .map_err(|_| install_error(format!("invalid ASAR offset for {path}")))?;
    let size = entry
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| install_error(format!("invalid ASAR size for {path}")))?;
    Ok(Some(PackedFile {
        path: path.to_string(),
        offset,
        size,
    }))
}

fn collect_packed_files(
    files: &Map<String, Value>,
    parent: &str,
    out: &mut Vec<PackedFile>,
) -> Result<(), EngineError> {
    for (name, entry) in files {
        let path = if parent.is_empty() {
            name.clone()
        } else {
            format!("{parent}/{name}")
        };
        if let Some(children) = entry.get("files").and_then(Value::as_object) {
            collect_packed_files(children, &path, out)?;
        } else if let Some(file) = packed_entry(entry, &path)? {
            out.push(file);
        }
    }
    Ok(())
}

fn read_packed_file(
    archive_path: &Path,
    archive: &AsarArchive,
    entry: &PackedFile,
) -> Result<Vec<u8>, EngineError> {
    if entry.size > MAX_CANDIDATE_JS_BYTES {
        return Err(install_error(format!(
            "ASAR JavaScript entry is unexpectedly large: {}",
            entry.path
        )));
    }
    let absolute = archive
        .data_offset
        .checked_add(entry.offset)
        .ok_or_else(|| install_error("ASAR file offset overflow"))?;
    let mut file = File::open(archive_path)
        .map_err(|e| install_error(format!("open app.asar payload: {e}")))?;
    let archive_size = file
        .metadata()
        .map_err(|e| install_error(format!("read app.asar payload metadata: {e}")))?
        .len();
    if absolute
        .checked_add(entry.size)
        .is_none_or(|end| end > archive_size)
    {
        return Err(install_error(format!(
            "ASAR entry extends beyond archive: {}",
            entry.path
        )));
    }
    file.seek(SeekFrom::Start(absolute))
        .map_err(|e| install_error(format!("seek app.asar payload: {e}")))?;
    let mut bytes = vec![0_u8; entry.size as usize];
    file.read_exact(&mut bytes)
        .map_err(|e| install_error(format!("read {} from app.asar: {e}", entry.path)))?;
    Ok(bytes)
}

fn embedded_object(source: &str, label: &str) -> Result<Map<String, Value>, EngineError> {
    serde_json::from_str::<Value>(source)
        .map_err(|e| install_error(format!("parse embedded {label}: {e}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| install_error(format!("embedded {label} is not a JSON object")))
}

fn menu_patch_is_valid(text: &str) -> bool {
    text.contains(MENU_PATCH_MARKER)
        && text.contains(".Menu.buildFromTemplate=(...e)=>")
        && text.contains("t.submenu&&__walk(t.submenu)")
}

fn patch_main_menu(content: &[u8]) -> Result<Vec<u8>, EngineError> {
    let text = std::str::from_utf8(content)
        .map_err(|e| install_error(format!("main application bundle is not UTF-8: {e}")))?;
    if text.contains(MENU_PATCH_MARKER) {
        if menu_patch_is_valid(text) {
            return Ok(content.to_vec());
        }
        return Err(install_error(
            "Windows 中文菜单标记存在，但注入内容不完整；已停止修改",
        ));
    }

    let pattern = Regex::new(
        r"(?P<call>(?P<receiver>[A-Za-z_$][A-Za-z0-9_$]*)\.Menu\.setApplicationMenu\((?P<menu>[A-Za-z_$][A-Za-z0-9_$]*)\))",
    )
    .map_err(|e| install_error(format!("compile application-menu matcher: {e}")))?;
    let matches = pattern.captures_iter(text).collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(install_error(format!(
            "当前 Codex 主菜单安装位置应为 1 处，实际找到 {} 处；已停止修改",
            matches.len()
        )));
    }
    let captures = &matches[0];
    let matched = captures
        .name("call")
        .ok_or_else(|| install_error("application-menu matcher lost the target call"))?;
    let receiver = captures
        .name("receiver")
        .ok_or_else(|| install_error("application-menu matcher lost the Electron receiver"))?
        .as_str();
    let menu = captures
        .name("menu")
        .ok_or_else(|| install_error("application-menu matcher lost the menu variable"))?
        .as_str();
    let translations = serde_json::to_string(&Value::Object(embedded_object(
        MENU_TRANSLATIONS_JSON,
        "menu translations",
    )?))
    .map_err(|e| install_error(format!("serialize menu translations: {e}")))?;

    let mut injection = String::new();
    injection.push_str("(()=>{const ");
    injection.push_str(MENU_PATCH_MARKER);
    injection.push('=');
    injection.push_str(&translations);
    injection.push_str(";const __walk=e=>{for(const t of e.items??[]){const e=(t.label??``).replaceAll(`&`,``);t.label=");
    injection.push_str(MENU_PATCH_MARKER);
    injection.push_str("[e]??t.label,t.submenu&&__walk(t.submenu)}};const __build=");
    injection.push_str(receiver);
    injection.push_str(".Menu.buildFromTemplate.bind(");
    injection.push_str(receiver);
    injection.push_str(".Menu);");
    injection.push_str(receiver);
    injection.push_str(
        ".Menu.buildFromTemplate=(...e)=>{const t=__build(...e);return __walk(t),t};__walk(",
    );
    injection.push_str(menu);
    injection.push_str(")})(),");

    let mut patched = String::with_capacity(text.len() + injection.len());
    patched.push_str(&text[..matched.start()]);
    patched.push_str(&injection);
    patched.push_str(matched.as_str());
    patched.push_str(&text[matched.end()..]);
    if !menu_patch_is_valid(&patched) {
        return Err(install_error("Windows 中文菜单注入后的结构校验失败"));
    }
    Ok(patched.into_bytes())
}

fn native_locale_is_valid(locale: &Map<String, Value>) -> Result<bool, EngineError> {
    let additions = embedded_object(NATIVE_STRINGS_JSON, "native locale additions")?;
    Ok(additions
        .iter()
        .all(|(key, expected)| locale.get(key) == Some(expected)))
}

fn patch_native_locale(content: &[u8]) -> Result<Vec<u8>, EngineError> {
    let mut locale = serde_json::from_slice::<Value>(content)
        .map_err(|e| install_error(format!("parse native zh-CN locale: {e}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| install_error("native zh-CN locale is not a JSON object"))?;
    for (key, value) in embedded_object(NATIVE_STRINGS_JSON, "native locale additions")? {
        locale.insert(key, value);
    }
    serde_json::to_vec(&Value::Object(locale))
        .map_err(|e| install_error(format!("serialize native zh-CN locale: {e}")))
}

fn renderer_i18n_patch_is_valid(text: &str) -> bool {
    text.contains(I18N_PATCH_MARKER) && text.contains("=!0/*__codexForceI18nV1*/,")
}

fn renderer_i18n_bundle_candidate(text: &str) -> bool {
    text.contains(I18N_PATCH_MARKER)
        || (text.contains(I18N_GATE_SIGNATURE) && text.contains(I18N_SOURCE_SIGNATURE))
}

fn patch_renderer_i18n(content: &[u8]) -> Result<Vec<u8>, EngineError> {
    let text = std::str::from_utf8(content)
        .map_err(|e| install_error(format!("renderer application bundle is not UTF-8: {e}")))?;
    if text.contains(I18N_PATCH_MARKER) {
        if renderer_i18n_patch_is_valid(text) {
            return Ok(content.to_vec());
        }
        return Err(install_error(
            "Windows Webview 中文标记存在，但启用内容不完整；已停止修改",
        ));
    }
    let gate_count = text.matches(I18N_GATE_SIGNATURE).count();
    if gate_count != 1 {
        return Err(install_error(format!(
            "当前 Codex Webview 中文开关应为 1 处，实际找到 {gate_count} 处；已停止修改"
        )));
    }

    let pattern = Regex::new(
        r"let (?P<enabled>[A-Za-z_$][A-Za-z0-9_$]*)=(?P<flag>[A-Za-z_$][A-Za-z0-9_$]*),(?P<source>[A-Za-z_$][A-Za-z0-9_$]*)=(?P<config>[A-Za-z_$][A-Za-z0-9_$]*)\?\.get\(`locale_source`,`IDE`\)",
    )
    .map_err(|e| install_error(format!("compile renderer i18n matcher: {e}")))?;
    let matches = pattern.captures_iter(text).collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(install_error(format!(
            "当前 Codex Webview 语言解析位置应为 1 处，实际找到 {} 处；已停止修改",
            matches.len()
        )));
    }
    let captures = &matches[0];
    let matched = captures
        .get(0)
        .ok_or_else(|| install_error("renderer i18n matcher lost the target"))?;
    let enabled = captures
        .name("enabled")
        .ok_or_else(|| install_error("renderer i18n matcher lost the enabled variable"))?
        .as_str();
    let source = captures
        .name("source")
        .ok_or_else(|| install_error("renderer i18n matcher lost the locale source variable"))?
        .as_str();
    let config = captures
        .name("config")
        .ok_or_else(|| install_error("renderer i18n matcher lost the config variable"))?
        .as_str();
    let replacement = format!(
        "let {enabled}=!0/*{I18N_PATCH_MARKER}*/,{source}={config}?.get(`locale_source`,`IDE`)"
    );
    let mut patched = String::with_capacity(text.len() + replacement.len() - matched.len());
    patched.push_str(&text[..matched.start()]);
    patched.push_str(&replacement);
    patched.push_str(&text[matched.end()..]);
    if !renderer_i18n_patch_is_valid(&patched) {
        return Err(install_error(
            "Windows Webview 中文开关注入后的结构校验失败",
        ));
    }
    Ok(patched.into_bytes())
}

fn analyze_patch(archive_path: &Path, archive: &AsarArchive) -> Result<PatchAnalysis, EngineError> {
    let mut files = Vec::new();
    collect_packed_files(
        archive
            .header
            .get("files")
            .and_then(Value::as_object)
            .ok_or_else(|| install_error("app.asar header has no file tree"))?,
        "",
        &mut files,
    )?;

    let main_candidates = files
        .iter()
        .filter(|entry| entry.path.starts_with(".vite/build/main-") && entry.path.ends_with(".js"))
        .cloned()
        .collect::<Vec<_>>();
    if main_candidates.len() != 1 {
        return Err(install_error(format!(
            "当前 Codex 应包含 1 个主进程 bundle，实际找到 {} 个；已停止修改",
            main_candidates.len()
        )));
    }
    let main = main_candidates[0].clone();
    let locale = files
        .iter()
        .find(|entry| entry.path == NATIVE_LOCALE_PATH)
        .cloned()
        .ok_or_else(|| install_error("当前 Codex 未内置 native-menu-locales/zh-CN.json"))?;
    let mut renderer_candidates = Vec::new();
    for entry in files.iter().filter(|entry| {
        entry.path.starts_with("webview/assets/")
            && entry.path.ends_with(".js")
            && entry.size <= MAX_CANDIDATE_JS_BYTES
    }) {
        let bytes = read_packed_file(archive_path, archive, entry)?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if renderer_i18n_bundle_candidate(text) {
            renderer_candidates.push((entry.clone(), bytes));
        }
    }
    if renderer_candidates.len() != 1 {
        return Err(install_error(format!(
            "当前 Codex 应包含 1 个 Webview 中文初始化 bundle，实际找到 {} 个；已停止修改",
            renderer_candidates.len()
        )));
    }
    let (renderer, renderer_bytes) = renderer_candidates.pop().expect("checked length");
    let renderer_path = renderer.path.clone();

    let main_bytes = read_packed_file(archive_path, archive, &main)?;
    let locale_bytes = read_packed_file(archive_path, archive, &locale)?;
    let patched_main = patch_main_menu(&main_bytes)?;
    let patched_locale = patch_native_locale(&locale_bytes)?;
    let patched_renderer = patch_renderer_i18n(&renderer_bytes)?;
    let mut patches = Vec::new();
    if patched_main != main_bytes {
        patches.push(FilePatch {
            target: main,
            patched_bytes: patched_main,
        });
    }
    if patched_locale != locale_bytes {
        patches.push(FilePatch {
            target: locale,
            patched_bytes: patched_locale,
        });
    }
    if patched_renderer != renderer_bytes {
        patches.push(FilePatch {
            target: renderer,
            patched_bytes: patched_renderer,
        });
    }

    let main_valid = std::str::from_utf8(
        if let Some(patch) = patches
            .iter()
            .find(|patch| patch.target.path.starts_with(".vite/build/main-"))
        {
            &patch.patched_bytes
        } else {
            &main_bytes
        },
    )
    .is_ok_and(menu_patch_is_valid);
    let locale_value = serde_json::from_slice::<Value>(
        if let Some(patch) = patches
            .iter()
            .find(|patch| patch.target.path == NATIVE_LOCALE_PATH)
        {
            &patch.patched_bytes
        } else {
            &locale_bytes
        },
    )
    .map_err(|e| install_error(format!("verify patched native zh-CN locale: {e}")))?;
    let locale_valid = locale_value
        .as_object()
        .is_some_and(|locale| native_locale_is_valid(locale).unwrap_or(false));
    let renderer_valid = std::str::from_utf8(
        if let Some(patch) = patches
            .iter()
            .find(|patch| patch.target.path == renderer_path)
        {
            &patch.patched_bytes
        } else {
            &renderer_bytes
        },
    )
    .is_ok_and(renderer_i18n_patch_is_valid);
    if !main_valid || !locale_valid || !renderer_valid {
        return Err(install_error("Windows 中文菜单补丁内容校验失败"));
    }

    Ok(PatchAnalysis {
        state: if patches.is_empty() {
            PatchState::AlreadyPatched
        } else {
            PatchState::NeedsPatch
        },
        patches,
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn integrity_value(bytes: &[u8], block_size: usize) -> Value {
    let block_size = block_size.max(1);
    let blocks = if bytes.is_empty() {
        vec![Value::String(encode_hex(&Sha256::digest(bytes)))]
    } else {
        bytes
            .chunks(block_size)
            .map(|block| Value::String(encode_hex(&Sha256::digest(block))))
            .collect()
    };
    let mut integrity = Map::new();
    integrity.insert("algorithm".to_string(), Value::String("SHA256".to_string()));
    integrity.insert(
        "hash".to_string(),
        Value::String(encode_hex(&Sha256::digest(bytes))),
    );
    integrity.insert(
        "blockSize".to_string(),
        Value::Number((block_size as u64).into()),
    );
    integrity.insert("blocks".to_string(), Value::Array(blocks));
    Value::Object(integrity)
}

fn update_header_entries(
    files: &mut Map<String, Value>,
    parent: &str,
    offsets: &HashMap<String, u64>,
    patched_files: &HashMap<String, Vec<u8>>,
) -> Result<(), EngineError> {
    for (name, entry) in files {
        let path = if parent.is_empty() {
            name.clone()
        } else {
            format!("{parent}/{name}")
        };
        if let Some(children) = entry.get_mut("files").and_then(Value::as_object_mut) {
            update_header_entries(children, &path, offsets, patched_files)?;
            continue;
        }
        if entry.get("unpacked").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if entry.get("offset").and_then(Value::as_str).is_none() {
            continue;
        }
        let object = entry
            .as_object_mut()
            .ok_or_else(|| install_error(format!("invalid ASAR entry for {path}")))?;
        let offset = offsets
            .get(&path)
            .ok_or_else(|| install_error(format!("missing recalculated ASAR offset for {path}")))?;
        object.insert("offset".to_string(), Value::String(offset.to_string()));
        if let Some(patched_bytes) = patched_files.get(&path) {
            object.insert(
                "size".to_string(),
                Value::Number((patched_bytes.len() as u64).into()),
            );
            let block_size = object
                .get("integrity")
                .and_then(|value| value.get("blockSize"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(DEFAULT_INTEGRITY_BLOCK_SIZE);
            object.insert(
                "integrity".to_string(),
                integrity_value(patched_bytes, block_size),
            );
        }
    }
    Ok(())
}

fn align_four(value: usize) -> Result<usize, EngineError> {
    value
        .checked_add((4 - value % 4) % 4)
        .ok_or_else(|| install_error("ASAR header size overflow"))
}

fn header_pickle(header: &Value) -> Result<Vec<u8>, EngineError> {
    let json = serde_json::to_vec(header)
        .map_err(|e| install_error(format!("serialize patched ASAR header: {e}")))?;
    let aligned_json = align_four(json.len())?;
    let payload_size = 4_usize
        .checked_add(aligned_json)
        .ok_or_else(|| install_error("ASAR header payload overflow"))?;
    let total_size = 4_usize
        .checked_add(payload_size)
        .ok_or_else(|| install_error("ASAR header pickle overflow"))?;
    let payload_u32 = u32::try_from(payload_size)
        .map_err(|_| install_error("ASAR header payload is too large"))?;
    let json_u32 =
        u32::try_from(json.len()).map_err(|_| install_error("ASAR header JSON is too large"))?;
    let mut out = vec![0_u8; total_size];
    out[..4].copy_from_slice(&payload_u32.to_le_bytes());
    out[4..8].copy_from_slice(&json_u32.to_le_bytes());
    out[8..8 + json.len()].copy_from_slice(&json);
    Ok(out)
}

fn copy_exact(source: &mut File, destination: &mut File, bytes: u64) -> Result<(), EngineError> {
    let copied = io::copy(&mut source.take(bytes), destination)
        .map_err(|e| install_error(format!("copy app.asar payload: {e}")))?;
    if copied != bytes {
        return Err(install_error(
            "app.asar ended while streaming patched payload",
        ));
    }
    Ok(())
}

fn write_patched_asar(
    source_path: &Path,
    destination_path: &Path,
    mut archive: AsarArchive,
    analysis: &PatchAnalysis,
) -> Result<(), EngineError> {
    if analysis.patches.is_empty() {
        return Err(install_error(
            "patch output requested for an already-patched archive",
        ));
    }
    let mut files = Vec::new();
    collect_packed_files(
        archive
            .header
            .get("files")
            .and_then(Value::as_object)
            .ok_or_else(|| install_error("app.asar header has no file tree"))?,
        "",
        &mut files,
    )?;
    files.sort_by_key(|entry| entry.offset);

    let patched_files = analysis
        .patches
        .iter()
        .map(|patch| (patch.target.path.clone(), patch.patched_bytes.clone()))
        .collect::<HashMap<_, _>>();
    if patched_files.len() != analysis.patches.len() {
        return Err(install_error("duplicate ASAR patch target"));
    }
    let mut offsets = HashMap::with_capacity(files.len());
    let mut delta = 0_i128;
    for entry in &files {
        let shifted = (entry.offset as i128)
            .checked_add(delta)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| {
                install_error(format!("ASAR offset shift overflow for {}", entry.path))
            })?;
        offsets.insert(entry.path.clone(), shifted);
        if let Some(bytes) = patched_files.get(&entry.path) {
            delta = delta
                .checked_add(bytes.len() as i128 - entry.size as i128)
                .ok_or_else(|| install_error("ASAR cumulative offset shift overflow"))?;
        }
    }
    update_header_entries(
        archive
            .header
            .get_mut("files")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| install_error("app.asar header has no mutable file tree"))?,
        "",
        &offsets,
        &patched_files,
    )?;
    let header_pickle = header_pickle(&archive.header)?;
    if header_pickle.len() > MAX_ASAR_HEADER_BYTES as usize {
        return Err(install_error(
            "patched app.asar header exceeds safety limit",
        ));
    }
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| install_error(format!("create Chinese patch work directory: {e}")))?;
    }

    let mut source = File::open(source_path)
        .map_err(|e| install_error(format!("open original app.asar: {e}")))?;
    let mut destination = File::create(destination_path)
        .map_err(|e| install_error(format!("create patched app.asar: {e}")))?;
    destination
        .write_all(&4_u32.to_le_bytes())
        .and_then(|_| destination.write_all(&(header_pickle.len() as u32).to_le_bytes()))
        .and_then(|_| destination.write_all(&header_pickle))
        .map_err(|e| install_error(format!("write patched app.asar header: {e}")))?;

    source
        .seek(SeekFrom::Start(archive.data_offset))
        .map_err(|e| install_error(format!("seek original app.asar data: {e}")))?;
    let mut patches = analysis.patches.iter().collect::<Vec<_>>();
    patches.sort_by_key(|patch| patch.target.offset);
    let mut cursor = 0_u64;
    for patch in patches {
        if patch.target.offset < cursor {
            return Err(install_error("overlapping ASAR patch targets"));
        }
        copy_exact(&mut source, &mut destination, patch.target.offset - cursor)?;
        destination
            .write_all(&patch.patched_bytes)
            .map_err(|e| install_error(format!("write {} patch: {e}", patch.target.path)))?;
        cursor = patch
            .target
            .offset
            .checked_add(patch.target.size)
            .ok_or_else(|| install_error("ASAR patch target end overflow"))?;
        source
            .seek(SeekFrom::Start(
                archive
                    .data_offset
                    .checked_add(cursor)
                    .ok_or_else(|| install_error("ASAR source seek overflow"))?,
            ))
            .map_err(|e| install_error(format!("seek after {}: {e}", patch.target.path)))?;
    }
    io::copy(&mut source, &mut destination)
        .map_err(|e| install_error(format!("copy remaining app.asar payload: {e}")))?;
    destination
        .flush()
        .and_then(|_| destination.sync_all())
        .map_err(|e| install_error(format!("flush patched app.asar: {e}")))?;
    Ok(())
}

fn inspect_patch_state(path: &Path) -> Result<PatchState, EngineError> {
    let archive = read_asar(path)?;
    Ok(analyze_patch(path, &archive)?.state)
}

pub(crate) fn codex_chinese_patch_is_active(installed: &InstalledWindowsCodex) -> bool {
    find_codex_app_asar(Path::new(&installed.path)).and_then(|path| inspect_patch_state(&path).ok())
        == Some(PatchState::AlreadyPatched)
}

#[cfg(test)]
fn sha256_file(path: &Path) -> Result<String, EngineError> {
    let mut file = File::open(path)
        .map_err(|e| install_error(format!("open file for SHA-256 {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| install_error(format!("hash {}: {e}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(encode_hex(&hasher.finalize()))
}

fn copy_backup_once(source: &Path, backup: &Path) -> Result<(), EngineError> {
    if backup.is_file() {
        return Ok(());
    }
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| install_error(format!("create Chinese patch backup directory: {e}")))?;
    }
    let temporary = backup.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let copy_result = (|| {
        let mut source_file = File::open(source)
            .map_err(|e| install_error(format!("open original app.asar for backup: {e}")))?;
        let mut backup_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| install_error(format!("create original app.asar backup: {e}")))?;
        io::copy(&mut source_file, &mut backup_file)
            .map_err(|e| install_error(format!("back up original app.asar: {e}")))?;
        backup_file
            .flush()
            .and_then(|_| backup_file.sync_all())
            .map_err(|e| install_error(format!("flush original app.asar backup: {e}")))
    })();
    if let Err(err) = copy_result {
        let _ = fs::remove_file(&temporary);
        return Err(err);
    }
    match fs::rename(&temporary, backup) {
        Ok(()) => Ok(()),
        Err(_err) if backup.is_file() => {
            let _ = fs::remove_file(&temporary);
            log::info!(
                "Chinese patch backup appeared concurrently path={}",
                backup.display()
            );
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&temporary);
            Err(install_error(format!(
                "commit original app.asar backup: {err}"
            )))
        }
    }
}

fn replace_portable(target: &Path, staged: &Path) -> Result<(), EngineError> {
    let parent = target
        .parent()
        .ok_or_else(|| install_error("app.asar has no parent directory"))?;
    let id = uuid::Uuid::new_v4();
    let incoming = parent.join(format!("app.asar.codex-manager-new-{id}"));
    let previous = parent.join(format!("app.asar.codex-manager-old-{id}"));
    fs::copy(staged, &incoming).map_err(|e| {
        install_error(format!(
            "stage patched app.asar beside portable install: {e}"
        ))
    })?;
    OpenOptions::new()
        .write(true)
        .open(&incoming)
        .and_then(|file| file.sync_all())
        .map_err(|e| install_error(format!("flush portable app.asar staging file: {e}")))?;
    fs::rename(target, &previous)
        .map_err(|e| install_error(format!("move original portable app.asar aside: {e}")))?;
    if let Err(err) = fs::rename(&incoming, target) {
        let _ = fs::rename(&previous, target);
        let _ = fs::remove_file(&incoming);
        return Err(install_error(format!(
            "install patched portable app.asar: {err}"
        )));
    }
    match inspect_patch_state(target) {
        Ok(PatchState::AlreadyPatched) => {}
        Ok(PatchState::NeedsPatch) | Err(_) => {
            let _ = fs::remove_file(target);
            let _ = fs::rename(&previous, target);
            return Err(install_error(
                "portable app.asar verification failed; original archive was restored",
            ));
        }
    }
    let _ = fs::remove_file(previous);
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchedCopyMarker {
    #[serde(default)]
    patch_schema: u32,
    source_app: String,
    source_version: String,
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn source_app_root(app_asar: &Path) -> Result<PathBuf, EngineError> {
    app_asar
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| install_error("resources/app.asar has no application root"))
}

fn patched_apps_root(manager_data_dir: &Path) -> PathBuf {
    manager_data_dir.join("chinese-patch").join("apps")
}

/// Close every managed Chinese copy before config or payload changes. Failure
/// is returned to the caller so it can log the condition while still honoring
/// a best-effort one-click configuration request.
pub fn close_patched_chinese_processes(
    manager_data_dir: &Path,
    timeout_secs: u64,
) -> Result<(), EngineError> {
    let root = patched_apps_root(manager_data_dir);
    if !root.exists() {
        return Ok(());
    }
    crate::windows_process::close_codex_processes_for_root(timeout_secs, &root)
}

fn patched_copy_root(
    installed: &InstalledWindowsCodex,
    manager_data_dir: &Path,
    source_app: &Path,
) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(normalized_path_key(source_app).as_bytes());
    hasher.update([0]);
    hasher.update(installed.version.as_bytes());
    let digest = encode_hex(&hasher.finalize());
    patched_apps_root(manager_data_dir).join(format!(
        "{}-{}",
        safe_version_component(&installed.version),
        &digest[..16]
    ))
}

fn marker_path(copy_root: &Path) -> PathBuf {
    copy_root.join(".codex-manager-zh-cn.json")
}

fn write_marker(
    copy_root: &Path,
    installed: &InstalledWindowsCodex,
    source_app: &Path,
) -> Result<(), EngineError> {
    let marker = PatchedCopyMarker {
        patch_schema: PATCH_SCHEMA_VERSION,
        source_app: source_app.display().to_string(),
        source_version: installed.version.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|e| install_error(format!("serialize Chinese copy marker: {e}")))?;
    let path = marker_path(copy_root);
    let mut file = File::create(&path)
        .map_err(|e| install_error(format!("create Chinese copy marker: {e}")))?;
    file.write_all(&bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|e| install_error(format!("write Chinese copy marker: {e}")))
}

fn marker_matches(copy_root: &Path, installed: &InstalledWindowsCodex, source_app: &Path) -> bool {
    let Ok(bytes) = fs::read(marker_path(copy_root)) else {
        return false;
    };
    let Ok(marker) = serde_json::from_slice::<PatchedCopyMarker>(&bytes) else {
        return false;
    };
    marker.patch_schema == PATCH_SCHEMA_VERSION
        && marker.source_version == installed.version
        && normalized_path_key(Path::new(&marker.source_app)) == normalized_path_key(source_app)
}

fn copy_file_contents(
    source: &Path,
    destination: &Path,
    buffer: &mut [u8],
) -> Result<(), EngineError> {
    let mut source_file = File::open(source).map_err(|e| {
        install_error(format!(
            "open application file for streaming {}: {e}",
            source.display()
        ))
    })?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| {
            install_error(format!(
                "create streamed application file {}: {e}",
                destination.display()
            ))
        })?;
    loop {
        let read = source_file.read(buffer).map_err(|e| {
            install_error(format!(
                "read application file while streaming {}: {e}",
                source.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        destination_file.write_all(&buffer[..read]).map_err(|e| {
            install_error(format!(
                "write streamed application file {}: {e}",
                destination.display()
            ))
        })?;
    }
    destination_file.flush().map_err(|e| {
        install_error(format!(
            "flush streamed application file {}: {e}",
            destination.display()
        ))
    })
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), EngineError> {
    let mut buffer = vec![0_u8; 1024 * 1024];
    copy_directory_with_buffer(source, destination, &mut buffer)
}

fn copy_directory_with_buffer(
    source: &Path,
    destination: &Path,
    buffer: &mut [u8],
) -> Result<(), EngineError> {
    fs::create_dir_all(destination).map_err(|e| {
        install_error(format!(
            "create Chinese application copy {}: {e}",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|e| {
        install_error(format!(
            "read application directory {}: {e}",
            source.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| install_error(format!("read application directory entry: {e}")))?;
        let file_type = entry
            .file_type()
            .map_err(|e| install_error(format!("read application entry type: {e}")))?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_with_buffer(&entry.path(), &target, buffer)?;
        } else if file_type.is_file() {
            // Copy bytes rather than invoking Windows CopyFile. CopyFile tries
            // to carry EFS metadata out of WindowsApps and can fail with
            // ERROR_ENCRYPTION_FAILED (6000) on otherwise readable files.
            copy_file_contents(&entry.path(), &target, buffer)?;
        } else {
            return Err(install_error(format!(
                "unsupported application entry while copying: {}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn sync_exe_asar_integrity(app_root: &Path, app_asar: &Path) -> Result<(), EngineError> {
    const MARKERS: &[&[u8]] = &[
        br#"{"file":"resources\\app.asar","alg":"SHA256","value":""#,
        br#"{"file":"resources\/app.asar","alg":"SHA256","value":""#,
        br#"{"file":"resources/app.asar","alg":"SHA256","value":""#,
    ];

    let exe = crate::portable::installed_app_exe(app_root).ok_or_else(|| {
        install_error(format!(
            "中文副本中未找到 ChatGPT.exe / Codex.exe：{}",
            app_root.display()
        ))
    })?;
    let mut bytes = fs::read(&exe)
        .map_err(|e| install_error(format!("read copied ChatGPT executable: {e}")))?;
    let Some((offset, marker)) = MARKERS
        .iter()
        .find_map(|marker| find_subslice(&bytes, marker).map(|offset| (offset, *marker)))
    else {
        return Ok(());
    };
    let hash_offset = offset + marker.len();
    let hash_end = hash_offset + 64;
    if hash_end > bytes.len()
        || !bytes[hash_offset..hash_end]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        log::warn!(
            "copied ChatGPT executable has an invalid app.asar integrity field path={}",
            exe.display()
        );
        return Ok(());
    }

    let archive = read_asar(app_asar)?;
    let header_hash = encode_hex(&Sha256::digest(&archive.header_json));
    if bytes[hash_offset..hash_end] == *header_hash.as_bytes() {
        return Ok(());
    }
    bytes[hash_offset..hash_end].copy_from_slice(header_hash.as_bytes());
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&exe)
        .map_err(|e| install_error(format!("open copied ChatGPT executable for update: {e}")))?;
    file.write_all(&bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|e| install_error(format!("update copied ChatGPT integrity hash: {e}")))
}

fn remove_stale_copies(apps_root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(apps_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !path.is_dir() {
            continue;
        }
        if let Err(err) = crate::portable::remove_directory_all_with_retry(
            "remove stale Chinese application copy",
            &path,
        ) {
            log::info!(
                "stale Chinese application copy remains path={} error={err}",
                path.display()
            );
        }
    }
}

fn activate_chinese_copy(work_root: &Path, copy_root: &Path) -> Result<(), EngineError> {
    if !work_root.is_dir() {
        return Err(install_error(format!(
            "activate Chinese application copy: source directory is missing: {}",
            work_root.display()
        )));
    }
    if copy_root.exists() {
        return Err(install_error(format!(
            "activate Chinese application copy: destination already exists: {}",
            copy_root.display()
        )));
    }
    let parent = copy_root.parent().ok_or_else(|| {
        install_error(format!(
            "activate Chinese application copy: destination has no parent: {}",
            copy_root.display()
        ))
    })?;
    if !parent.is_dir() {
        return Err(install_error(format!(
            "activate Chinese application copy: destination parent is missing: {}",
            parent.display()
        )));
    }

    crate::portable::rename_directory_with_retry(
        "activate Chinese application copy",
        work_root,
        copy_root,
    )
    .map_err(|err| {
        install_error(format!(
            "activate Chinese application copy: source={} destination={} raw_os_error={:?}: {err}",
            work_root.display(),
            copy_root.display(),
            err.raw_os_error()
        ))
    })
}

fn safe_version_component(version: &str) -> String {
    let value = version
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "unknown".to_string()
    } else {
        value
    }
}

/// Resolve the version-matched writable Chinese copy for a registered MSIX.
/// Portable installs are patched in place and therefore do not need an
/// alternate launch target.
pub fn patched_chinese_install(
    installed: &InstalledWindowsCodex,
    manager_data_dir: &Path,
) -> Option<InstalledWindowsCodex> {
    if installed.source != "msix" {
        return None;
    }
    let source_asar = find_codex_app_asar(Path::new(&installed.path))?;
    let source_app = source_app_root(&source_asar).ok()?;
    let copy_root = patched_copy_root(installed, manager_data_dir, &source_app);
    if !marker_matches(&copy_root, installed, &source_app) {
        return None;
    }
    let app_root = copy_root.join("app");
    find_codex_app_asar(&app_root)?;
    if crate::portable::installed_app_exe(&app_root).is_none() {
        return None;
    }
    Some(InstalledWindowsCodex {
        path: app_root.display().to_string(),
        version: installed.version.clone(),
        arch: installed.arch.clone(),
        source: "portable".to_string(),
        // Keep the Store identity so the copied runtime can reuse the MSIX
        // user-data directory when it is launched outside the package sandbox.
        package_family_name: installed.package_family_name.clone(),
        installed_at: installed.installed_at,
    })
}

fn patch_msix_copy(
    installed: &InstalledWindowsCodex,
    manager_data_dir: &Path,
    source_asar: &Path,
) -> Result<ChinesePatchReport, EngineError> {
    let source_app = source_app_root(source_asar)?;
    if let Some(existing) = patched_chinese_install(installed, manager_data_dir) {
        let app_asar = find_codex_app_asar(Path::new(&existing.path))
            .ok_or_else(|| install_error("version-matched Chinese copy lost resources/app.asar"))?;
        return Ok(ChinesePatchReport {
            status: "already-patched".to_string(),
            message: "Windows 中文副本已经启用。".to_string(),
            app_asar_path: app_asar.display().to_string(),
            backup_path: None,
            restart_required: true,
        });
    }

    let apps_root = patched_apps_root(manager_data_dir);
    fs::create_dir_all(&apps_root)
        .map_err(|e| install_error(format!("create Chinese applications directory: {e}")))?;
    if let Err(err) = close_patched_chinese_processes(manager_data_dir, 8) {
        log::warn!(
            "could not close an existing Chinese ChatGPT copy root={} error={err}",
            apps_root.display()
        );
    }

    let copy_root = patched_copy_root(installed, manager_data_dir, &source_app);
    let work_root = apps_root.join(format!(
        ".work-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let work_app = work_root.join("app");
    let mut preserve_failed_work = false;
    let result = (|| {
        copy_directory(&source_app, &work_app)?;
        let copied_asar = find_codex_app_asar(&work_app).ok_or_else(|| {
            install_error(format!(
                "中文副本中未找到 resources/app.asar：{}",
                work_app.display()
            ))
        })?;
        let archive = read_asar(&copied_asar)?;
        let analysis = analyze_patch(&copied_asar, &archive)?;
        if analysis.state == PatchState::NeedsPatch {
            let staged_asar = work_root.join("app.asar.patched");
            write_patched_asar(&copied_asar, &staged_asar, archive, &analysis)?;
            replace_portable(&copied_asar, &staged_asar)?;
            let _ = fs::remove_file(staged_asar);
        }
        if inspect_patch_state(&copied_asar)? != PatchState::AlreadyPatched {
            return Err(install_error("中文副本 app.asar 校验失败"));
        }
        sync_exe_asar_integrity(&work_app, &copied_asar)?;
        write_marker(&work_root, installed, &source_app)?;

        if copy_root.exists() {
            crate::portable::remove_directory_all_with_retry(
                "replace stale Chinese application copy",
                &copy_root,
            )
            .map_err(|e| install_error(format!("remove stale Chinese application copy: {e}")))?;
        }
        if let Err(err) = activate_chinese_copy(&work_root, &copy_root) {
            preserve_failed_work = true;
            return Err(err);
        }
        remove_stale_copies(&apps_root, &copy_root);

        let installed_asar = find_codex_app_asar(&copy_root.join("app"))
            .ok_or_else(|| install_error("activated Chinese copy lost resources/app.asar"))?;
        Ok(ChinesePatchReport {
            status: "patched".to_string(),
            message: "Windows 中文副本已创建并启用。".to_string(),
            app_asar_path: installed_asar.display().to_string(),
            backup_path: None,
            restart_required: true,
        })
    })();
    if result.is_err() && preserve_failed_work && work_root.is_dir() {
        log::error!(
            "retaining failed Chinese application copy for recovery path={}",
            work_root.display()
        );
    } else if result.is_err() {
        let _ = crate::portable::remove_directory_all_with_retry(
            "clean failed Chinese application copy",
            &work_root,
        );
    }
    result
}

/// Patch the detected Windows Codex installation after its processes have been
/// closed. Registered MSIX packages remain untouched: their application
/// payload is copied to a user-writable, versioned directory and patched there.
pub fn patch_codex_chinese(
    installed: &InstalledWindowsCodex,
    manager_data_dir: &Path,
) -> Result<ChinesePatchReport, EngineError> {
    let install_root = PathBuf::from(&installed.path);
    let app_asar = find_codex_app_asar(&install_root).ok_or_else(|| {
        install_error(format!(
            "未在 Codex 安装目录找到 resources/app.asar：{}",
            install_root.display()
        ))
    })?;
    if installed.source == "msix" {
        return patch_msix_copy(installed, manager_data_dir, &app_asar);
    }

    let archive = read_asar(&app_asar)?;
    let analysis = analyze_patch(&app_asar, &archive)?;
    if analysis.state == PatchState::AlreadyPatched {
        return Ok(ChinesePatchReport {
            status: "already-patched".to_string(),
            message: "Windows 中文化已经启用。".to_string(),
            app_asar_path: app_asar.display().to_string(),
            backup_path: None,
            restart_required: true,
        });
    }

    let patch_root = manager_data_dir.join("chinese-patch");
    let work_dir = patch_root.join(format!("work-{}", uuid::Uuid::new_v4()));
    let staged = work_dir.join("app.asar");
    fs::create_dir_all(&work_dir)
        .map_err(|e| install_error(format!("create Chinese patch work directory: {e}")))?;
    let result = (|| {
        write_patched_asar(&app_asar, &staged, archive, &analysis)?;
        if inspect_patch_state(&staged)? != PatchState::AlreadyPatched {
            return Err(install_error("staged Chinese app.asar verification failed"));
        }

        let backup = app_asar.with_extension("asar.bak");
        copy_backup_once(&app_asar, &backup)?;
        replace_portable(&app_asar, &staged)?;
        Ok(ChinesePatchReport {
            status: "patched".to_string(),
            message: "Windows 中文化已写入，重新启动 ChatGPT 后生效。".to_string(),
            app_asar_path: app_asar.display().to_string(),
            backup_path: Some(backup.display().to_string()),
            restart_required: true,
        })
    })();
    let _ = fs::remove_dir_all(work_dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codex-chinese-patch-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn file_entry(offset: u64, bytes: &[u8]) -> Value {
        let mut entry = Map::new();
        entry.insert(
            "size".to_string(),
            Value::Number((bytes.len() as u64).into()),
        );
        entry.insert("offset".to_string(), Value::String(offset.to_string()));
        entry.insert(
            "integrity".to_string(),
            integrity_value(bytes, DEFAULT_INTEGRITY_BLOCK_SIZE),
        );
        Value::Object(entry)
    }

    fn directory(files: Map<String, Value>) -> Value {
        let mut entry = Map::new();
        entry.insert("files".to_string(), Value::Object(files));
        Value::Object(entry)
    }

    #[test]
    fn activates_chinese_copy_into_an_empty_destination() {
        let root = temp_dir("activate-copy");
        let work = root.join(".work-test");
        let active = root.join("active");
        fs::create_dir_all(&work).unwrap();
        fs::write(work.join("marker"), b"ready").unwrap();

        activate_chinese_copy(&work, &active).unwrap();

        assert!(!work.exists());
        assert_eq!(fs::read(active.join("marker")).unwrap(), b"ready");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_activate_over_an_existing_copy() {
        let root = temp_dir("activate-existing-copy");
        let work = root.join(".work-test");
        let active = root.join("active");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&active).unwrap();

        let error = activate_chinese_copy(&work, &active).unwrap_err();

        assert!(error.to_string().contains("destination already exists"));
        assert!(work.is_dir());
        assert!(active.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    const ORIGINAL_MAIN: &[u8] = b"const It={items:[]};l.Menu.setApplicationMenu(It);";
    const ORIGINAL_RENDERER: &[u8] =
        b"o=a?.get(`enable_i18n`,!1);let s=o,c=a?.get(`locale_source`,`IDE`);";
    const CACHED_RENDERER: &[u8] = b"let o;t[0]===a?o=t[1]:(o=a?.get(`enable_i18n`,!1),t[0]=a,t[1]=o);let s=o,c=a?.get(`locale_source`,`IDE`),l=localeOverride;";

    fn write_fixture_with_renderer(
        path: &Path,
        main: &[u8],
        locale: &[u8],
        renderer_name: &str,
        renderer: &[u8],
        extra_assets: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let package = br#"{"name":"openai-codex-electron","version":"1.2.3"}"#;
        let tail = b"tail-content".to_vec();
        let main_offset = package.len() as u64;
        let locale_offset = main_offset + main.len() as u64;

        let mut build = Map::new();
        build.insert("main-test.js".to_string(), file_entry(main_offset, main));
        let mut vite = Map::new();
        vite.insert("build".to_string(), directory(build));
        let mut native_locales = Map::new();
        native_locales.insert("zh-CN.json".to_string(), file_entry(locale_offset, locale));
        let mut assets = Map::new();
        let mut next_offset = locale_offset + locale.len() as u64;
        assets.insert(renderer_name.to_string(), file_entry(next_offset, renderer));
        next_offset += renderer.len() as u64;
        for (name, bytes) in extra_assets {
            assets.insert((*name).to_string(), file_entry(next_offset, bytes));
            next_offset += bytes.len() as u64;
        }
        assets.insert("tail.js".to_string(), file_entry(next_offset, &tail));
        let mut webview = Map::new();
        webview.insert("assets".to_string(), directory(assets));
        let mut root_files = Map::new();
        root_files.insert("package.json".to_string(), file_entry(0, package));
        root_files.insert(".vite".to_string(), directory(vite));
        root_files.insert("native-menu-locales".to_string(), directory(native_locales));
        root_files.insert("webview".to_string(), directory(webview));
        let mut root = Map::new();
        root.insert("files".to_string(), Value::Object(root_files));
        let header = header_pickle(&Value::Object(root)).unwrap();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(path).unwrap();
        file.write_all(&4_u32.to_le_bytes()).unwrap();
        file.write_all(&(header.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(&header).unwrap();
        file.write_all(package).unwrap();
        file.write_all(main).unwrap();
        file.write_all(locale).unwrap();
        file.write_all(renderer).unwrap();
        for (_, bytes) in extra_assets {
            file.write_all(bytes).unwrap();
        }
        file.write_all(&tail).unwrap();
        file.flush().unwrap();
        tail
    }

    fn write_fixture(path: &Path, main: &[u8], locale: &[u8], renderer: &[u8]) -> Vec<u8> {
        write_fixture_with_renderer(path, main, locale, "app-main-test.js", renderer, &[])
    }

    fn extract_entry(path: &Path, entry_path: &str) -> Vec<u8> {
        let archive = read_asar(path).unwrap();
        let mut files = Vec::new();
        collect_packed_files(
            archive.header.get("files").unwrap().as_object().unwrap(),
            "",
            &mut files,
        )
        .unwrap();
        let entry = files
            .into_iter()
            .find(|entry| entry.path == entry_path)
            .unwrap();
        read_packed_file(path, &archive, &entry).unwrap()
    }

    #[test]
    fn rewrites_menu_and_locale_and_preserves_following_files() {
        let root = temp_dir("rewrite");
        let source = root.join("source.asar");
        let patched = root.join("patched.asar");
        let tail = write_fixture(&source, ORIGINAL_MAIN, b"{}", ORIGINAL_RENDERER);

        let archive = read_asar(&source).unwrap();
        let analysis = analyze_patch(&source, &archive).unwrap();
        assert_eq!(analysis.state, PatchState::NeedsPatch);
        write_patched_asar(&source, &patched, archive, &analysis).unwrap();

        assert_eq!(
            inspect_patch_state(&patched).unwrap(),
            PatchState::AlreadyPatched
        );
        assert_eq!(
            extract_entry(&patched, "webview/assets/tail.js"),
            tail,
            "entries after both enlarged files must retain valid offsets"
        );
        assert!(
            String::from_utf8(extract_entry(&patched, ".vite/build/main-test.js"))
                .unwrap()
                .contains(MENU_PATCH_MARKER)
        );
        let locale =
            serde_json::from_slice::<Value>(&extract_entry(&patched, NATIVE_LOCALE_PATH)).unwrap();
        assert!(native_locale_is_valid(locale.as_object().unwrap()).unwrap());
        assert!(
            String::from_utf8(extract_entry(&patched, "webview/assets/app-main-test.js"))
                .unwrap()
                .contains(I18N_PATCH_MARKER)
        );
        assert_eq!(
            crate::read_codex_app_version_from_asar(&patched).as_deref(),
            Some("1.2.3")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_i18n_initialization_after_it_moves_to_app_initial_bundle() {
        let root = temp_dir("app-initial");
        let source = root.join("source.asar");
        let patched = root.join("patched.asar");
        let app_main = b"import{start}from'./app-initial-test.js';start();";
        let general_settings = b"const settings=['enable_i18n','locale_source'];";
        write_fixture_with_renderer(
            &source,
            ORIGINAL_MAIN,
            b"{}",
            "app-initial-test.js",
            CACHED_RENDERER,
            &[
                ("app-main-test.js", app_main),
                ("general-settings-test.js", general_settings),
            ],
        );

        let archive = read_asar(&source).unwrap();
        let analysis = analyze_patch(&source, &archive).unwrap();
        let renderer_patch = analysis
            .patches
            .iter()
            .find(|patch| {
                patch
                    .patched_bytes
                    .windows(I18N_PATCH_MARKER.len())
                    .any(|window| window == I18N_PATCH_MARKER.as_bytes())
            })
            .unwrap();
        assert_eq!(
            renderer_patch.target.path,
            "webview/assets/app-initial-test.js"
        );
        write_patched_asar(&source, &patched, archive, &analysis).unwrap();
        assert_eq!(
            inspect_patch_state(&patched).unwrap(),
            PatchState::AlreadyPatched
        );
        assert_eq!(
            extract_entry(&patched, "webview/assets/app-main-test.js"),
            app_main
        );
        assert_eq!(
            extract_entry(&patched, "webview/assets/general-settings-test.js"),
            general_settings
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patches_real_asar_when_fixture_path_is_provided() {
        let Some(source) = std::env::var_os("CODEX_REAL_APP_ASAR") else {
            return;
        };
        let source = PathBuf::from(source);
        let root = temp_dir("real-asar");
        let explicit_output = std::env::var_os("CODEX_REAL_PATCH_OUTPUT").map(PathBuf::from);
        let patched = explicit_output
            .clone()
            .unwrap_or_else(|| root.join("app.asar"));
        fs::create_dir_all(patched.parent().unwrap()).unwrap();
        let source_version = crate::read_codex_app_version_from_asar(&source);

        let archive = read_asar(&source).unwrap();
        let analysis = analyze_patch(&source, &archive).unwrap();
        assert_eq!(analysis.state, PatchState::NeedsPatch);
        assert_eq!(analysis.patches.len(), 3);
        write_patched_asar(&source, &patched, archive, &analysis).unwrap();
        assert_eq!(
            inspect_patch_state(&patched).unwrap(),
            PatchState::AlreadyPatched
        );
        assert_eq!(
            crate::read_codex_app_version_from_asar(&patched),
            source_version
        );
        if explicit_output.is_none() {
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn detects_already_patched_archive_without_rewriting() {
        let root = temp_dir("already");
        let source = root.join("app.asar");
        let main = patch_main_menu(ORIGINAL_MAIN).unwrap();
        let locale = patch_native_locale(b"{}").unwrap();
        let renderer = patch_renderer_i18n(ORIGINAL_RENDERER).unwrap();
        write_fixture(&source, &main, &locale, &renderer);
        assert_eq!(
            inspect_patch_state(&source).unwrap(),
            PatchState::AlreadyPatched
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_missing_or_ambiguous_menu_install_call() {
        let root = temp_dir("reject");
        let missing = root.join("missing.asar");
        write_fixture(&missing, b"const It={items:[]};", b"{}", ORIGINAL_RENDERER);
        assert!(inspect_patch_state(&missing)
            .unwrap_err()
            .to_string()
            .contains("实际找到 0"));

        let duplicate = root.join("duplicate.asar");
        write_fixture(
            &duplicate,
            b"l.Menu.setApplicationMenu(It);l.Menu.setApplicationMenu(It);",
            b"{}",
            ORIGINAL_RENDERER,
        );
        assert!(inspect_patch_state(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("实际找到 2"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn portable_patch_keeps_a_pristine_backup_and_is_idempotent() {
        let root = temp_dir("portable");
        let install = root.join("Codex");
        let asar = install.join("resources/app.asar");
        write_fixture(&asar, ORIGINAL_MAIN, b"{}", ORIGINAL_RENDERER);
        let original_hash = sha256_file(&asar).unwrap();
        let installed = InstalledWindowsCodex {
            path: install.display().to_string(),
            version: "1.2.3".to_string(),
            arch: Some("x64".to_string()),
            source: "portable".to_string(),
            package_family_name: None,
            installed_at: None,
        };

        let first = patch_codex_chinese(&installed, &root.join("data")).unwrap();
        assert_eq!(first.status, "patched");
        let backup = PathBuf::from(first.backup_path.unwrap());
        assert_eq!(sha256_file(&backup).unwrap(), original_hash);
        let second = patch_codex_chinese(&installed, &root.join("data")).unwrap();
        assert_eq!(second.status, "already-patched");
        assert_eq!(sha256_file(&backup).unwrap(), original_hash);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn msix_patch_uses_a_versioned_writable_copy_and_leaves_source_untouched() {
        let root = temp_dir("msix-copy");
        let package = root.join("WindowsApps/OpenAI.Codex_1.2.3.0_x64/app");
        let asar = package.join("resources/app.asar");
        write_fixture(&asar, ORIGINAL_MAIN, b"{}", ORIGINAL_RENDERER);
        let original_hash = sha256_file(&asar).unwrap();
        let exe_marker = format!(
            "prefix{{\"file\":\"resources/app.asar\",\"alg\":\"SHA256\",\"value\":\"{}suffix",
            "a".repeat(64)
        );
        fs::write(package.join("ChatGPT.exe"), exe_marker).unwrap();
        fs::write(package.join("helper.dll"), b"runtime").unwrap();
        let installed = InstalledWindowsCodex {
            path: root
                .join("WindowsApps/OpenAI.Codex_1.2.3.0_x64")
                .display()
                .to_string(),
            version: "1.2.3".to_string(),
            arch: Some("x64".to_string()),
            source: "msix".to_string(),
            package_family_name: Some("OpenAI.Codex_test".to_string()),
            installed_at: None,
        };
        let data = root.join("data");

        let first = patch_codex_chinese(&installed, &data).unwrap();
        assert_eq!(first.status, "patched");
        assert_eq!(first.backup_path, None);
        assert_eq!(sha256_file(&asar).unwrap(), original_hash);
        assert_eq!(inspect_patch_state(&asar).unwrap(), PatchState::NeedsPatch);

        let patched = patched_chinese_install(&installed, &data).unwrap();
        assert_eq!(patched.source, "portable");
        assert_eq!(
            patched.package_family_name.as_deref(),
            Some("OpenAI.Codex_test")
        );
        assert_ne!(
            normalized_path_key(Path::new(&patched.path)),
            normalized_path_key(&package)
        );
        let patched_asar = find_codex_app_asar(Path::new(&patched.path)).unwrap();
        assert_eq!(
            inspect_patch_state(&patched_asar).unwrap(),
            PatchState::AlreadyPatched
        );
        assert!(Path::new(&patched.path).join("helper.dll").is_file());
        let copied_exe = fs::read(Path::new(&patched.path).join("ChatGPT.exe")).unwrap();
        assert!(!copied_exe
            .windows(64)
            .any(|window| window == "a".repeat(64).as_bytes()));

        let second = patch_codex_chinese(&installed, &data).unwrap();
        assert_eq!(second.status, "already-patched");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn application_copy_does_not_carry_readonly_source_attributes() {
        let root = temp_dir("stream-copy");
        let source = root.join("source/app");
        let destination = root.join("destination/app");
        fs::create_dir_all(source.join("nested")).unwrap();
        let source_file = source.join("nested/runtime.manifest");
        fs::write(&source_file, b"runtime-manifest").unwrap();
        let mut permissions = fs::metadata(&source_file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&source_file, permissions).unwrap();

        copy_directory(&source, &destination).unwrap();

        let copied_file = destination.join("nested/runtime.manifest");
        assert_eq!(fs::read(&copied_file).unwrap(), b"runtime-manifest");
        assert!(!fs::metadata(&copied_file).unwrap().permissions().readonly());
        let _ = fs::remove_dir_all(root);
    }
}
