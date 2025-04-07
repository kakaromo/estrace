// src-tauri/src/trace/mod.rs - Update to use dynamic patterns

mod block;
mod export;
mod filter;
pub mod patterns;
mod types;
mod ufs;
mod utils;

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

// 타입들을 재내보냅니다
pub use types::*;
// pub use patterns::*; 이 줄은 제거하거나 주석 처리 (사용되지 않는 import 경고)

// 공통 상수와 정적 변수들
pub(crate) static UFS_CACHE: Lazy<Mutex<HashMap<String, Vec<UFS>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static BLOCK_CACHE: Lazy<Mutex<HashMap<String, Vec<Block>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Pattern caches
pub(crate) static UFS_PATTERNS: Lazy<RwLock<HashMap<String, Regex>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
pub(crate) static BLOCK_PATTERNS: Lazy<RwLock<HashMap<String, Regex>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// Current active patterns (name -> compiled regex)
pub(crate) static ACTIVE_UFS_PATTERN: Lazy<RwLock<(String, Regex)>> = Lazy::new(|| {
    RwLock::new((
        "Default UFS Pattern".to_string(),
        Regex::new(
            r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*([-]?\d+)"
        ).unwrap()
    ))
});

pub(crate) static ACTIVE_BLOCK_PATTERN: Lazy<RwLock<(String, Regex)>> = Lazy::new(|| {
    RwLock::new((
        "Default Block Pattern".to_string(),
        Regex::new(
            r"^\s*(?P<process>.*?)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>.+?)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)(?:\s+\S+)?\s+\[(?P<comm>.*?)\]$"
        ).unwrap()
    ))
});

// 샘플링 관련 상수 - 기본값 설정
pub const DEFAULT_PREVIEW_RECORDS: usize = 500_000;

// initialize_patterns 함수를 직접 재내보내기
pub use patterns::initialize_patterns;

// Tauri 명령 함수들
#[tauri::command]
pub async fn readtrace(logname: String, maxrecords: Option<usize>) -> Result<String, String> {
    utils::readtrace(logname, maxrecords.unwrap_or(DEFAULT_PREVIEW_RECORDS)).await
}

#[tauri::command]
pub async fn starttrace(fname: String, logfolder: String) -> Result<TraceParseResult, String> {
    utils::starttrace(fname, logfolder).await
}

#[tauri::command]
pub async fn ufs_latencystats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
) -> Result<String, String> {
    ufs::latencystats(
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
    )
    .await
}

#[tauri::command]
pub async fn ufs_sizestats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    ufs::sizestats(
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
    )
    .await
}

#[tauri::command]
pub async fn block_latencystats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
    group: bool,
) -> Result<String, String> {
    block::latencystats(
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
        group,
    )
    .await
}

#[tauri::command]
pub async fn block_sizestats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    group: bool,
) -> Result<String, String> {
    block::sizestats(
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        group,
    )
    .await
}

#[tauri::command]
pub async fn ufs_continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    ufs::continuity_stats(logname, zoom_column, time_from, time_to, col_from, col_to).await
}

#[tauri::command]
pub async fn block_continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    block::continuity_stats(logname, zoom_column, time_from, time_to, col_from, col_to).await
}

#[tauri::command]
pub async fn export_to_csv(
    parquet_path: String,
    output_dir: Option<String>,
) -> Result<String, String> {
    export::export_to_csv(parquet_path, output_dir).await
}

#[tauri::command]
pub async fn filter_trace(
    logname: String,
    tracetype: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    maxrecords: Option<usize>,
) -> Result<String, String> {
    utils::filter_trace(
        logname,
        tracetype,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        maxrecords.unwrap_or(DEFAULT_PREVIEW_RECORDS),
    )
    .await
}

// Pattern management commands
#[tauri::command]
pub fn add_pattern(name: String, pattern_type: String, pattern: String) -> Result<(), String> {
    patterns::add_pattern(name, pattern_type, pattern)
}

#[tauri::command]
pub fn set_active_pattern(name: String, pattern_type: String) -> Result<(), String> {
    patterns::set_active_pattern(name, pattern_type)
}

#[tauri::command]
pub fn get_patterns(pattern_type: Option<String>) -> Result<String, String> {
    patterns::get_patterns(pattern_type)
}

#[tauri::command]
pub fn get_active_patterns() -> Result<String, String> {
    patterns::get_active_patterns()
}

#[tauri::command]
pub fn delete_pattern(name: String, pattern_type: String) -> Result<(), String> {
    patterns::delete_pattern(name, pattern_type)
}

/**
 * 이미 파싱된 트레이스를 다시 파싱하는 함수
 * 패턴이 변경되었거나, 파싱이 제대로 되지 않은 경우에 사용
 */
#[tauri::command]
pub async fn reparse_trace(
    id: i64,
    logfile_path: String,
    logfolder: String,
) -> Result<String, String> {
    // 로그 파일 존재 여부 확인
    let path = std::path::Path::new(&logfile_path);
    if !path.exists() {
        return Err(format!("로그 파일이 존재하지 않습니다: {}", logfile_path));
    }

    // 캐시 초기화 - 해당 ID에 대한 캐시 삭제
    {
        let mut ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        ufs_cache.remove(&id.to_string());
    }
    {
        let mut block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        block_cache.remove(&id.to_string());
    }

    // 로그 파일 다시 파싱
    let result = utils::starttrace(logfile_path, logfolder).await?;

    // 파싱 결과를 JSON으로 반환
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// 파일 및 폴더 삭제 관련 명령
#[tauri::command]
pub fn delete_parquet_files(file_paths: Vec<String>) -> Result<(), String> {
    let mut _success_count = 0;
    let mut error_messages = Vec::new();

    for path in file_paths {
        if path.trim().is_empty() {
            continue;
        }

        let file_path = std::path::Path::new(&path);
        if file_path.exists() {
            match std::fs::remove_file(file_path) {
                Ok(_) => {
                    println!("Successfully deleted file: {}", path);
                    _success_count += 1;
                }
                Err(e) => {
                    let error_msg = format!("Failed to delete file {}: {}", path, e);
                    println!("Warning: {}", error_msg);
                    error_messages.push(error_msg);
                }
            }
        } else {
            println!("File does not exist: {}", path);
        }
    }

    if !error_messages.is_empty() {
        return Err(error_messages.join("; "));
    }

    Ok(())
}

#[tauri::command]
pub fn delete_folder(folder_path: String) -> Result<(), String> {
    if folder_path.trim().is_empty() {
        return Ok(());
    }

    let path = std::path::Path::new(&folder_path);
    
    // 폴더 존재 여부 먼저 확인
    if !path.exists() {
        println!("Folder does not exist: {}", folder_path);
        return Ok(());
    }
    
    // 실제로 디렉토리인지 확인
    if !path.is_dir() {
        println!("Path exists but is not a directory: {}", folder_path);
        return Ok(());
    }

    // 삭제 시도
    println!("Attempting to delete folder: {}", folder_path);
    match std::fs::remove_dir_all(path) {
        Ok(_) => {
            println!("Successfully deleted folder: {}", folder_path);
            Ok(())
        }
        Err(e) => {
            // 더 자세한 에러 메시지
            let error_msg = format!("Failed to delete folder {}: {} (Error type: {:?})", 
                folder_path, e, e.kind());
            println!("Error: {}", error_msg);
            
            // Windows에서는 일부 조건에서 폴더 삭제가 지연될 수 있으므로
            // 권한 관련 문제인지 확인
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Err(format!("권한 거부: 폴더 {} 삭제 실패 - 다른 프로세스가 사용 중일 수 있음", folder_path))
            } else {
                Err(error_msg)
            }
        }
    }
}

#[tauri::command]
pub fn test_regex_pattern(text: String, pattern: String) -> Result<String, String> {
    patterns::test_regex_pattern(text, pattern)
}
