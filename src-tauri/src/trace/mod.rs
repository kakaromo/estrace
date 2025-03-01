mod block;
mod export;
mod filter;
mod types;
mod ufs;
mod utils;

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;

// 타입들을 재내보냅니다
pub use types::*;

// 공통 상수와 정적 변수들
pub(crate) static UFS_CACHE: Lazy<Mutex<HashMap<String, Vec<UFS>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static BLOCK_CACHE: Lazy<Mutex<HashMap<String, Vec<Block>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// 정규식 패턴
static UFS_TRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)"
    ).unwrap()
});

static BLOCK_TRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?P<process>.*?)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>.+?)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)(?:\s+\S+)?\s+\[(?P<comm>.*?)\]$"
    ).unwrap()
});

// 샘플링 관련 상수
const MAX_PREVIEW_RECORDS: usize = 4_000_000;

// Tauri 명령 함수들
#[tauri::command]
pub async fn readtrace(logname: String) -> Result<String, String> {
    utils::readtrace(logname).await
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
