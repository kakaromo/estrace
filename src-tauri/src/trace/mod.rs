// src-tauri/src/trace/mod.rs - Update to use dynamic patterns

mod block;
mod export;
mod filter;
pub mod patterns;
mod types;
mod ufs;
mod ufscustom;
mod utils;
mod constants;
mod parser_highperf; // 고성능 파서 추가

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use serde::Serialize;
use crate::trace::utils::{TraceDataBytes, TraceLengths};
use tauri::Window;
use lazy_static::lazy_static;
use tauri::Emitter;

// 타입들을 재내보냅니다
pub use types::*;
// pub use patterns::*; 이 줄은 제거하거나 주석 처리 (사용되지 않는 import 경고)

// 공통 상수와 정적 변수들
pub(crate) static UFS_CACHE: Lazy<Mutex<HashMap<String, Vec<UFS>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static BLOCK_CACHE: Lazy<Mutex<HashMap<String, Vec<Block>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static UFSCUSTOM_CACHE: Lazy<Mutex<HashMap<String, Vec<UFSCUSTOM>>>> =
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

pub(crate) static ACTIVE_UFSCUSTOM_PATTERN: Lazy<RwLock<(String, Regex)>> = Lazy::new(|| {
    RwLock::new((
        "Default UFS Custom Pattern".to_string(),
        Regex::new(
            r"^(?P<opcode>0x[0-9a-f]+),(?P<lba>\d+),(?P<size>\d+),(?P<start_time>\d+(?:\.\d+)?),(?P<end_time>\d+(?:\.\d+)?)$"
        ).unwrap()
    ))
});

// 샘플링 관련 상수 - 기본값 설정
pub const DEFAULT_PREVIEW_RECORDS: usize = 500_000;

// initialize_patterns 함수를 직접 재내보내기
pub use patterns::initialize_patterns;

// 진행 상태 이벤트를 위한 구조체 추가
#[derive(Clone, Debug, Serialize)]
pub struct ProgressEvent {
    pub stage: String,
    pub progress: f32,          // 0.0 ~ 100.0
    pub current: u64,
    pub total: u64,
    pub message: String,
    pub eta_seconds: f32,      // 예상 남은 시간(초)
    pub processing_speed: f32, // 처리 속도(항목/초)
}

// 작업 취소를 위한 글로벌 상태
lazy_static! {
    pub static ref CANCEL_SIGNAL: Mutex<bool> = Mutex::new(false);
}

// Tauri 명령 함수들
#[tauri::command]
pub async fn readtrace(logname: String, maxrecords: Option<usize>) -> Result<TraceDataBytes, String> {
    utils::readtrace(logname, maxrecords.unwrap_or(DEFAULT_PREVIEW_RECORDS)).await
}

#[tauri::command]
pub async fn readtrace_highperf(logname: String, window: tauri::Window) -> Result<String, String> {
    // 고성능 파서 사용
    match parser_highperf::parse_log_file_highperf(&logname, Some(&window)) {
        Ok((ufs_traces, block_traces, ufscustom_traces)) => {
            Ok(format!(
                "고성능 파싱 완료: UFS={}, Block={}, UFSCUSTOM={}",
                ufs_traces.len(),
                block_traces.len(),
                ufscustom_traces.len()
            ))
        }
        Err(e) => Err(format!("파싱 실패: {}", e)),
    }
}

#[tauri::command]
pub async fn readtrace_to_files(logname: String, maxrecords: Option<usize>) -> Result<utils::TraceFilePaths, String> {
    utils::readtrace_to_files(logname, maxrecords.unwrap_or(DEFAULT_PREVIEW_RECORDS)).await
}

#[tauri::command]
pub async fn trace_lengths(logname: String) -> Result<TraceLengths, String> {
    utils::trace_lengths(logname).await
}

#[tauri::command]
pub async fn starttrace(fname: String, logfolder: String, window: Window) -> Result<TraceParseResult, String> {
    // 작업 시작 시 취소 신호 초기화
    {
        let mut cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
        *cancel = false;
    }
    utils::starttrace(fname, logfolder, window).await
}

#[allow(clippy::too_many_arguments)]
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
) -> Result<Vec<u8>, String> {
    use ufs::UfsLatencyStatsParams;
    
    ufs::latencystats(UfsLatencyStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
    })
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
) -> Result<Vec<u8>, String> {
    use ufs::UfsSizeStatsParams;
    
    ufs::sizestats(UfsSizeStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
    })
    .await
}

#[tauri::command]
pub async fn ufs_allstats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
) -> Result<Vec<u8>, String> {
    use ufs::UfsAllStatsParams;
    
    ufs::allstats(UfsAllStatsParams {
        logname,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
    }, thresholds).await
}

#[allow(clippy::too_many_arguments)]
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
) -> Result<Vec<u8>, String> {
    use block::LatencyStatsParams;
    
    block::latencystats(LatencyStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
        group,
    })
    .await
}

#[allow(clippy::too_many_arguments)]
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
) -> Result<Vec<u8>, String> {
    use block::SizeStatsParams;
    
    block::sizestats(SizeStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        group,
    })
    .await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn block_allstats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
    group: bool,
) -> Result<Vec<u8>, String> {
    use block::AllStatsParams;
    
    block::allstats(AllStatsParams {
        logname,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
        group,
    }).await
}

#[tauri::command]
pub async fn ufs_continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
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
) -> Result<Vec<u8>, String> {
    block::continuity_stats(logname, zoom_column, time_from, time_to, col_from, col_to).await
}

// UFSCUSTOM 통계 명령어들
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ufscustom_latencystats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
) -> Result<Vec<u8>, String> {
    use ufscustom::UfscustomLatencyStatsParams;
    
    ufscustom::latencystats(UfscustomLatencyStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        thresholds,
    })
    .await
}

#[tauri::command]
pub async fn ufscustom_sizestats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
    use ufscustom::UfscustomSizeStatsParams;
    
    ufscustom::sizestats(UfscustomSizeStatsParams {
        logname,
        column,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
    })
    .await
}

#[tauri::command]
pub async fn ufscustom_allstats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
) -> Result<Vec<u8>, String> {
    use ufscustom::UfscustomAllStatsParams;
    
    ufscustom::allstats(UfscustomAllStatsParams {
        logname,
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
    }, thresholds).await
}

#[tauri::command]
pub async fn ufscustom_continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
    ufscustom::continuity_stats(logname, zoom_column, time_from, time_to, col_from, col_to).await
}

#[tauri::command]
pub async fn export_to_csv(
    parquet_path: String,
    output_dir: Option<String>,
    time_from: Option<f64>,
    time_to: Option<f64>,
    zoom_column: Option<String>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<String>, String> {
    let filter = if time_from.is_some() || time_to.is_some() || col_from.is_some() || col_to.is_some() {
        Some(export::FilterParams {
            time_from,
            time_to,
            zoom_column,
            col_from,
            col_to,
        })
    } else {
        None
    };
    
    export::export_to_csv(parquet_path, output_dir, filter).await
}

#[allow(clippy::too_many_arguments)]
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
) -> Result<TraceDataBytes, String> {
    use utils::FilterTraceParams;
    
    utils::filter_trace(FilterTraceParams {
        logname,
        tracetype, 
        zoom_column,
        time_from,
        time_to,
        col_from,
        col_to,
        max_records: maxrecords.unwrap_or(DEFAULT_PREVIEW_RECORDS),
    })
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
    window: tauri::Window,
) -> Result<String, String> {
    // 로그 파일 존재 여부 확인
    let path = std::path::Path::new(&logfile_path);
    if !path.exists() {
        return Err(format!("로그 파일이 존재하지 않습니다: {}", logfile_path));
    }

    // 작업 시작 시 취소 신호 초기화
    {
        let mut cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
        *cancel = false;
    }
    
    // 진행 상태 초기 이벤트 전송
    let _ = window.emit("trace-progress", ProgressEvent {
        stage: "init".to_string(),
        progress: 0.0,
        current: 0,
        total: 100,
        message: "재파싱 준비 중...".to_string(),
        eta_seconds: 0.0,
        processing_speed: 0.0,
    });

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
    let result = utils::starttrace(logfile_path, logfolder, window).await?;

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
            let error_msg = format!(
                "Failed to delete folder {}: {} (Error type: {:?})",
                folder_path,
                e,
                e.kind()
            );
            println!("Error: {}", error_msg);

            // Windows에서는 일부 조건에서 폴더 삭제가 지연될 수 있으므로
            // 권한 관련 문제인지 확인
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Err(format!(
                    "권한 거부: 폴더 {} 삭제 실패 - 다른 프로세스가 사용 중일 수 있음",
                    folder_path
                ))
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

// Tauri 명령 - 진행 중인 작업 취소
#[tauri::command]
pub fn cancel_trace_process() -> Result<bool, String> {
    let mut cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
    *cancel = true;
    Ok(true)
}

// Tauri 명령 - 취소 신호 초기화
#[tauri::command]
pub fn reset_cancel_signal() -> Result<bool, String> {
    let mut cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
    *cancel = false;
    Ok(true)
}

// Tauri 명령 - 작업 상태 확인
#[tauri::command]
pub fn check_cancel_status() -> Result<bool, String> {
    let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
    Ok(*cancel)
}

// Tauri 명령 - 캐시 초기화
#[tauri::command]
pub async fn clear_all_cache() -> Result<String, String> {
    utils::clear_all_cache().await
}

// Tauri 명령 - 임시 Arrow 파일 정리
#[tauri::command]
pub async fn cleanup_temp_arrow_files(max_age_hours: u64) -> Result<usize, String> {
    // utils에서 구현된 함수 호출 (DB 경로는 자동으로 찾음)
    utils::cleanup_temp_arrow_files_impl(max_age_hours).await
}
