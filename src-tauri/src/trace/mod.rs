use std::fs::{File, create_dir_all};
// use std::io::{BufReader, Read};
use memmap2::Mmap;
use arrow::temporal_conversions::MILLISECONDS;
use regex::Regex;
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use chrono::Local;
use rayon::prelude::*;

use std::path::PathBuf;
use std::sync::Arc;
use arrow::array::{ArrayRef, Float64Array, StringArray, UInt32Array, UInt64Array, BooleanArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use datafusion::prelude::*;

use std::time::Instant;
use rand::prelude::IndexedRandom;
use rand::rng;

use std::collections::HashMap;
use std::collections::BTreeMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// 새로운 ufs trace 정규식
static UFS_TRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)"
        ).unwrap()
});
// 새로운 block trace 정규식
static BLOCK_TRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        // r"^(?P<process>.*?)-(?P<pid>\d+)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>\S+)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)\s+\[(?P<comm>[^\]]+)\]$"
        // r"^(?P<process>.*?)-(?P<pid>\d+)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>\S+)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)(?:\s+\S+)?\s+\[(?P<comm>[^\]]+)\]$"
        r"^\s*(?P<process>.*?)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>.+?)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)(?:\s+\S+)?\s+\[(?P<comm>.*?)\]$"
        ).unwrap()
});

#[derive(Serialize, Debug, Clone)]
pub struct UFS {
    time: f64,
    process: String,
    cpu: u32,
    action: String,
    tag: u32,    
    opcode: String,
    lba: u64,
    size: u32,
    groupid: u32,
    hwqid: u32,
    qd: u32,       // Queue Depth
    dtoc: f64,     // Device to Complete latency
    ctoc: f64,     // Complete to Complete latency 
    ctod: f64,      // Complete to Device latency
    continuous: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct Block {
    pub time: f64,
    pub process: String,
    pub cpu: u32,
    pub flags: String,    
    pub action: String,
    pub devmajor: u32,
    pub devminor: u32,
    pub io_type: String,
    pub extra: u32,
    pub sector: u64,
    pub size: u32,
    pub comm: String,
    qd: u32,       // Queue Depth
    dtoc: f64,     // Device to Complete latency
    ctoc: f64,     // Complete to Complete latency 
    ctod: f64,      // Complete to Device latency
    continuous: bool,
}

#[derive(Serialize, Debug, Clone)]
pub enum ChartValue {
    F64(f64),
    U32(u32),
    U64(u64),
}

impl ChartValue {
    // filtering 용도로 f64 값으로 변환 (u32, u64는 f64로 변환)
    pub fn as_f64(&self) -> f64 {
        match *self {
            ChartValue::F64(v) => v,
            ChartValue::U32(v) => v as f64,
            ChartValue::U64(v) => v as f64,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct ChartStat {
    pub time: f64,
    pub opcode: String,
    pub value: ChartValue,
}

pub(crate) static UFS_CACHE: Lazy<Mutex<HashMap<String, Vec<UFS>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static BLOCK_CACHE: Lazy<Mutex<HashMap<String, Vec<Block>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
#[derive(Serialize, Debug, Clone)]
pub struct TraceParseResult {
    pub missing_lines: Vec<usize>,
    pub ufs_parquet_filename: String,
    pub block_parquet_filename: String,
}

const MAX_PREVIEW_RECORDS: usize = 4_000_000;

fn sample_ufs(ufs_list: &[UFS]) -> Vec<UFS> {
    if ufs_list.len() <= MAX_PREVIEW_RECORDS {
        ufs_list.to_vec()
    } else {
        let mut rng = rng();
        ufs_list
            .choose_multiple(&mut rng, MAX_PREVIEW_RECORDS)
            .cloned()
            .collect()
    }
}

fn sample_block(block_list: &[Block]) -> Vec<Block> {
    if block_list.len() <= MAX_PREVIEW_RECORDS {
        block_list.to_vec()
    } else {
        let mut rng = rng();
        block_list
            .choose_multiple(&mut rng, MAX_PREVIEW_RECORDS)
            .cloned()
            .collect()
    }
}

#[tauri::command]
pub async fn readtrace(logname: String) -> Result<String, String> {
    // 캐시 확인: 두 캐시 모두 있는지 확인
    {
        let ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        let block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        
        if ufs_cache.contains_key(&logname) || block_cache.contains_key(&logname) {
            let result_json = serde_json::json!({
                "ufs": ufs_cache.get(&logname).unwrap_or(&Vec::new()),
                "block": block_cache.get(&logname).unwrap_or(&Vec::new())
            });
            println!("Cache hit for {}", logname);
            return Ok(result_json.to_string());
        }
    }
    
    let mut ufs_vec: Vec<UFS> = Vec::new();
    let mut block_vec: Vec<Block> = Vec::new();

    // logname에 쉼표가 있으면 각각의 파일 경로로 분리, 없으면 하나의 경로로 처리
    let files: Vec<String> = if logname.contains(',') {
        logname.split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        vec![logname.clone()]
    };

    // DataFusion context 생성 (비동기로 실행)
    let ctx = SessionContext::new();
    let overall_start = Instant::now();

    // 각 파일 처리: 파일명에 따라 ufs 또는 block으로 구분
    for file in files {
        let path = PathBuf::from(&file);
        if !path.is_file() {
            continue; // 파일이 아니면 건너뜁니다.
        }
        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            if fname.contains("ufs") && fname.ends_with(".parquet") {
                // UFS parquet 파일 읽기
                let ufs_fileread_start = Instant::now();
                let df = ctx
                    .read_parquet(path.to_str().ok_or("Invalid path")?, ParquetReadOptions::default())
                    .await
                    .map_err(|e| e.to_string())?;
                let ufs_fileread_duration = ufs_fileread_start.elapsed().as_secs_f64();
                println!("UFS file read duration: {:.3} seconds", ufs_fileread_duration);

                let ufs_batch_start = Instant::now();
                let batches = df.collect().await.map_err(|e| e.to_string())?;
                for batch in batches {
                    let num_rows = batch.num_rows();
                    let schema = batch.schema();
                    // 컬럼 인덱스 추출 및 배열 다운캐스팅 처리
                    let time_idx = schema.index_of("time").map_err(|e| e.to_string())?;
                    let process_idx = schema.index_of("process").map_err(|e| e.to_string())?;
                    let cpu_idx = schema.index_of("cpu").map_err(|e| e.to_string())?;
                    let action_idx = schema.index_of("action").map_err(|e| e.to_string())?;
                    let tag_idx = schema.index_of("tag").map_err(|e| e.to_string())?;
                    let opcode_idx = schema.index_of("opcode").map_err(|e| e.to_string())?;
                    let lba_idx = schema.index_of("lba").map_err(|e| e.to_string())?;
                    let size_idx = schema.index_of("size").map_err(|e| e.to_string())?;
                    let groupid_idx = schema.index_of("groupid").map_err(|e| e.to_string())?;
                    let hwqid_idx = schema.index_of("hwqid").map_err(|e| e.to_string())?;
                    let qd_idx = schema.index_of("qd").map_err(|e| e.to_string())?;
                    let dtoc_idx = schema.index_of("dtoc").map_err(|e| e.to_string())?;
                    let ctoc_idx = schema.index_of("ctoc").map_err(|e| e.to_string())?;
                    let ctod_idx = schema.index_of("ctod").map_err(|e| e.to_string())?;
                    let cont_idx = schema.index_of("continuous").map_err(|e| e.to_string())?;
            
                    let time_array = batch.column(time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'time'")?;
                    let process_array = batch.column(process_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'process'")?;
                    let cpu_array = batch.column(cpu_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'cpu'")?;
                    let action_array = batch.column(action_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'action'")?;
                    let tag_array = batch.column(tag_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'tag'")?;
                    let opcode_array = batch.column(opcode_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'opcode'")?;
                    let lba_array = batch.column(lba_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt64Array>()
                        .ok_or("Failed to downcast 'lba'")?;
                    let size_array = batch.column(size_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'size'")?;
                    let groupid_array = batch.column(groupid_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'groupid'")?;
                    let hwqid_array = batch.column(hwqid_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'hwqid'")?;
                    let qd_array = batch.column(qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'qd'")?;
                    let dtoc_array = batch.column(dtoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'dtoc'")?;
                    let ctoc_array = batch.column(ctoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctoc'")?;
                    let ctod_array = batch.column(ctod_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctod'")?;
                    let cont_array = batch.column(cont_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::BooleanArray>()
                        .ok_or("Failed to downcast 'continuous'")?;
            
                    for row in 0..num_rows {
                        ufs_vec.push(UFS {
                            time: time_array.value(row),
                            process: process_array.value(row).to_string(),
                            cpu: cpu_array.value(row),
                            action: action_array.value(row).to_string(),
                            tag: tag_array.value(row),
                            opcode: opcode_array.value(row).to_string(),
                            lba: lba_array.value(row),
                            size: size_array.value(row),
                            groupid: groupid_array.value(row),
                            hwqid: hwqid_array.value(row),
                            qd: qd_array.value(row),
                            dtoc: dtoc_array.value(row),
                            ctoc: ctoc_array.value(row),
                            ctod: ctod_array.value(row),
                            continuous: cont_array.value(row),
                        });
                    }                    
                }
                let ufs_batch_duration = ufs_batch_start.elapsed().as_secs_f64();
                println!("UFS batch processing duration: {:.3} seconds", ufs_batch_duration);
                // 캐시 세팅
                {
                    let mut ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
                    let ufspath = path.to_string_lossy().to_string();
                    println!("UFS cache insert: {}", ufspath);
                    ufs_cache.insert(ufspath, ufs_vec.clone());
                }
            } else if fname.contains("block") && fname.ends_with(".parquet") {
                let block_fileread_start = Instant::now();
                // Block parquet 파일 읽기
                let df = ctx
                    .read_parquet(path.to_str().ok_or("Invalid path")?, ParquetReadOptions::default())
                    .await
                    .map_err(|e| e.to_string())?;
                let block_fileread_duration = block_fileread_start.elapsed().as_secs_f64();
                println!("Block file read duration: {:.3} seconds", block_fileread_duration);

                let block_batch_start = Instant::now();
                let batches = df.collect().await.map_err(|e| e.to_string())?;
                for batch in batches {
                    let num_rows = batch.num_rows();
                    let schema = batch.schema();
                    let time_idx = schema.index_of("time").map_err(|e| e.to_string())?;
                    let process_idx = schema.index_of("process").map_err(|e| e.to_string())?;
                    let cpu_idx = schema.index_of("cpu").map_err(|e| e.to_string())?;
                    let flags_idx = schema.index_of("flags").map_err(|e| e.to_string())?;                    
                    let action_idx = schema.index_of("action").map_err(|e| e.to_string())?;
                    let devmajor_idx = schema.index_of("devmajor").map_err(|e| e.to_string())?;
                    let devminor_idx = schema.index_of("devminor").map_err(|e| e.to_string())?;
                    let io_type_idx = schema.index_of("io_type").map_err(|e| e.to_string())?;
                    let extra_idx = schema.index_of("extra").map_err(|e| e.to_string())?;
                    let sector_idx = schema.index_of("sector").map_err(|e| e.to_string())?;
                    let size_idx = schema.index_of("size").map_err(|e| e.to_string())?;
                    let comm_idx = schema.index_of("comm").map_err(|e| e.to_string())?;
                    let qd_idx = schema.index_of("qd").map_err(|e| e.to_string())?;
                    let dtoc_idx = schema.index_of("dtoc").map_err(|e| e.to_string())?;
                    let ctoc_idx = schema.index_of("ctoc").map_err(|e| e.to_string())?;
                    let ctod_idx = schema.index_of("ctod").map_err(|e| e.to_string())?;
                    let cont_idx = schema.index_of("continuous").map_err(|e| e.to_string())?;
                    
                    let time_array = batch.column(time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'time'")?;
                    let process_array = batch.column(process_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'process'")?;
                    let cpu_array = batch.column(cpu_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'cpu'")?;
                    let flags_array = batch.column(flags_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'flags'")?;                    
                    let action_array = batch.column(action_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'action'")?;
                    let devmajor_array = batch.column(devmajor_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'devmajor'")?;
                    let devminor_array = batch.column(devminor_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'devminor'")?;
                    let io_type_array = batch.column(io_type_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'io_type'")?;
                    let extra_array = batch.column(extra_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'extra'")?;
                    let sector_array = batch.column(sector_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt64Array>()
                        .ok_or("Failed to downcast 'sector'")?;
                    let size_array = batch.column(size_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'size'")?;
                    let comm_array = batch.column(comm_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'comm'")?;
                    let qd_array = batch.column(qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'qd'")?;
                    let dtoc_array = batch.column(dtoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'dtoc'")?;
                    let ctoc_array = batch.column(ctoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctoc'")?;
                    let ctod_array = batch.column(ctod_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctod'")?;
                    let cont_array = batch.column(cont_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::BooleanArray>()
                        .ok_or("Failed to downcast 'continuous'")?;
            
                    for row in 0..num_rows {
                        block_vec.push(Block {
                            time: time_array.value(row),
                            process: process_array.value(row).to_string(),
                            cpu: cpu_array.value(row),
                            flags: flags_array.value(row).to_string(),                            
                            action: action_array.value(row).to_string(),
                            devmajor: devmajor_array.value(row),
                            devminor: devminor_array.value(row),
                            io_type: io_type_array.value(row).to_string(),
                            extra: extra_array.value(row),
                            sector: sector_array.value(row),
                            size: size_array.value(row),
                            comm: comm_array.value(row).to_string(),
                            qd: qd_array.value(row),
                            dtoc: dtoc_array.value(row),
                            ctoc: ctoc_array.value(row),
                            ctod: ctod_array.value(row),
                            continuous: cont_array.value(row),
                        });
                    }
                }
                let block_batch_duration = block_batch_start.elapsed().as_secs_f64();
                println!("Block batch processing duration: {:.3} seconds", block_batch_duration);
                // 캐시 세팅
                {
                    let mut block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
                    let blockpath = path.to_string_lossy().to_string();
                    println!("Block cache insert: {}", blockpath);
                    block_cache.insert(blockpath, block_vec.clone());
                }
            }
        }
    }
    
    
    let sample_start = Instant::now();
    let sampleufs = sample_ufs(&ufs_vec);
    let sampleblock = sample_block(&block_vec);
    let sample_duration = sample_start.elapsed().as_secs_f64();
    println!("Sampling duration: {:.3} seconds", sample_duration);

    let json_start = Instant::now();
    // 결과 JSON: ufs와 block 모두 벡터로 포함 (파일이 없으면 빈 배열)
    let result_json = serde_json::json!({
        "ufs": sampleufs,
        "block": sampleblock
    });
    let json_duration = json_start.elapsed().as_secs_f64();
    println!("JSON serialization duration: {:.3} seconds", json_duration);
    let overall_duration = overall_start.elapsed().as_secs_f64();
    println!("Overall duration: {:.3} seconds", overall_duration);
    Ok(result_json.to_string())
}

fn parse_ufs_trace(line: &str) -> Result<UFS, String> {
    let caps = UFS_TRACE_RE.captures(line).ok_or("No match for UFS trace")?;
    let process = &caps[1];
    let cpu_str = &caps[2];
    let time_str = &caps[3];
    let action = &caps[4];
    let tag_str = &caps[5];
    let size_str = &caps[6];
    let lba_str = &caps[7];
    let opcode = &caps[8];
    let groupid_str = &caps[9];
    let hwqid_str = &caps[10];

    let time: f64 = time_str.parse::<f64>().map_err(|e| e.to_string())?;
    let cpu: u32 = cpu_str.parse::<u32>().map_err(|e| e.to_string())?;
    let tag: u32 = tag_str.parse::<u32>().map_err(|e| e.to_string())?;
    let size: i32 = size_str.parse::<i32>().map_err(|e| e.to_string())?;
    // byte를 4KB 단위로 변환 (4096 bytes = 4KB)
    let size: u32 = (size.abs() as u32) / 4096;
    let lba: u64 = lba_str.parse::<u64>().map_err(|e| e.to_string())?;
    let groupid: u32 = u32::from_str_radix(groupid_str, 16).map_err(|e| e.to_string())?;
    let hwqid: u32 = hwqid_str.parse::<u32>().map_err(|e| e.to_string())?;

    Ok(UFS {
        time,
        process: process.to_string(),
        cpu,
        action: action.to_string(),
        tag,
        opcode: opcode.to_string(),
        lba,
        size, // 이제 4KB 단위로 저장됨
        groupid,
        hwqid,
        qd: 0,
        dtoc: 0.0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

// block trace 파싱 함수
fn parse_block_trace(line: &str) -> Result<Block, String> {
    let caps = BLOCK_TRACE_RE.captures(line).ok_or("No match")?;
    let time = caps.name("time").and_then(|m| m.as_str().parse::<f64>().ok()).ok_or("time parse error")?;
    let process = caps.name("process").map(|m| m.as_str().to_string()).unwrap_or_default();
    // let pid = caps.name("pid").and_then(|m| m.as_str().parse::<u32>().ok()).ok_or("pid parse error")?;
    let cpu = caps.name("cpu").and_then(|m| m.as_str().parse::<u32>().ok()).ok_or("cpu parse error")?;
    let flags = caps.name("flags").map(|m| m.as_str().to_string()).unwrap_or_default();    
    let action = caps.name("action").map(|m| m.as_str().to_string()).unwrap_or_default();
    let devmajor = caps.name("devmajor").and_then(|m| m.as_str().parse::<u32>().ok()).ok_or("devmajor error")?;
    let devminor = caps.name("devminor").and_then(|m| m.as_str().parse::<u32>().ok()).ok_or("devminor error")?;
    let io_type = caps.name("io_type").map(|m| m.as_str().to_string()).unwrap_or_default();
    let extra = caps.name("extra").map_or(0, |m| m.as_str().parse().unwrap_or(0));
    let sector = if &caps["sector"] == "18446744073709551615" { 0 } else { caps["sector"].parse().unwrap_or(0) };
    let size = caps.name("size").and_then(|m| m.as_str().parse::<u32>().ok()).ok_or("size error")?;
    let comm = caps.name("comm").map(|m| m.as_str().to_string()).unwrap_or_default();

    Ok(Block {
        time,
        process,
        cpu,
        flags,        
        action,
        devmajor,
        devminor,
        io_type,
        extra,
        sector,
        size,
        comm,
        qd: 0,
        dtoc: 0.0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

fn ufs_bottom_half_latency_process(mut ufs_list: Vec<UFS>) -> Vec<UFS> {
    // time 기준으로 오름차순 정렬
    ufs_list.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    
    let mut req_times: HashMap<(u32, String), f64> = HashMap::new();
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    
    // 이전 send_req의 정보를 저장할 변수들
    let mut prev_send_req: Option<(u64, u32, String)> = None; // (lba, size, opcode)

    for ufs in ufs_list.iter_mut() {
        match ufs.action.as_str() {
            "send_req" => {
                // 연속성 체크: 이전 send_req가 있는 경우
                if let Some((prev_lba, prev_size, prev_opcode)) = prev_send_req {
                    let prev_end_addr = prev_lba + prev_size as u64;
                    // 현재 요청의 시작 주소가 이전 요청의 끝 주소와 같고, opcode가 같은 경우
                    ufs.continuous = ufs.lba == prev_end_addr && ufs.opcode == prev_opcode;
                } else {
                    ufs.continuous = false;
                }
                
                // 현재 send_req 정보 저장
                prev_send_req = Some((ufs.lba, ufs.size, ufs.opcode.clone()));

                req_times.insert((ufs.tag, ufs.opcode.clone()), ufs.time);
                current_qd += 1;
                if current_qd == 1 {
                    if let Some(t) = last_complete_qd0_time {
                        ufs.ctod = (ufs.time - t)*MILLISECONDS as f64;
                    }
                }
            },
            "complete_rsp" => {
                // complete_rsp는 continuous 체크하지 않음
                ufs.continuous = false;
                
                current_qd = current_qd.saturating_sub(1);
                if let Some(send_time) = req_times.remove(&(ufs.tag, ufs.opcode.clone())) {
                    ufs.dtoc = (ufs.time - send_time)*MILLISECONDS as f64;
                }
                if let Some(last) = last_complete_time {
                    ufs.ctoc = (ufs.time - last)*MILLISECONDS as f64;
                }
                if current_qd == 0 {
                    last_complete_qd0_time = Some(ufs.time);
                }
                last_complete_time = Some(ufs.time);
            },
            _ => {
                ufs.continuous = false;
            }
        }
        ufs.qd = current_qd;
    }
    ufs_list
}

fn block_bottom_half_latency_process(mut block_list: Vec<Block>) -> Vec<Block> {
    // time 기준으로 오름차순 정렬
    block_list.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    
    let mut req_times: HashMap<(u64, String), f64> = HashMap::new(); // (sector, io_type)을 key로 사용
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;

    // issue 횟수를 tracking하기 위한 HashMap
    let mut issue_counts: HashMap<(u64, String), u32> = HashMap::new();
    
    // 연속성 체크를 위한 변수
    let mut prev_end_sector: Option<u64> = None;
    let mut prev_io_type: Option<String> = None;

    for block in block_list.iter_mut() {
        // 기본적으로 continuous를 false로 설정
        block.continuous = false;

        // io_type에서 첫 글자로 read/write 판단
        let io_operation = if block.io_type.starts_with('R') {
            "read"
        } else if block.io_type.starts_with('W') {
            "write"
        } else {
            "other"
        };

        let key = (block.sector, io_operation.to_string());

        match block.action.as_str() {
            "block_rq_issue" => {  // d = issue                
                if io_operation != "other" {
                    // 첫 번째 issue인 경우에만 처리
                    if !issue_counts.contains_key(&key) {
                        // 연속성 체크 (read/write에 한함)
                        if let (Some(end_sector), Some(prev_type)) = (prev_end_sector, prev_io_type.as_ref()) {
                            if block.sector == end_sector && io_operation == prev_type {
                                block.continuous = true;
                            }
                        }
                        
                        // 현재 요청의 끝 sector 및 io_type 업데이트
                        prev_end_sector = Some(block.sector + block.size as u64);
                        prev_io_type = Some(io_operation.to_string());

                        // req_times.insert(key.clone(), block.time);
                        // current_qd += 1;
                        
                        // if current_qd == 1 {
                        //     if let Some(t) = last_complete_qd0_time {
                        //         block.ctod = (block.time - t) * MILLISECONDS as f64;
                        //     }
                        // }
                    }
                }
                // } else {
                //     // read/write가 아닌 경우, 연속성 판단은 하지 않지만 요청은 처리함
                //     req_times.insert(key.clone(), block.time);
                //     current_qd += 1;
                    
                //     if current_qd == 1 {
                //         if let Some(t) = last_complete_qd0_time {
                //             block.ctod = (block.time - t) * MILLISECONDS as f64;
                //         }
                //     }
                // }
                req_times.insert(key.clone(), block.time);
                current_qd += 1;
                
                if current_qd == 1 {
                    if let Some(t) = last_complete_qd0_time {
                        block.ctod = (block.time - t) * MILLISECONDS as f64;
                    }
                }
                *issue_counts.entry(key).or_insert(0) += 1;
            },
            "block_rq_complete" => {  // c = complete
                // complete는 항상 continuous = false
                block.continuous = false;
                
                if let Some(first_issue_time) = req_times.remove(&key) {
                    block.dtoc = (block.time - first_issue_time)*MILLISECONDS as f64;
                }
                if let Some(last) = last_complete_time {
                    block.ctoc = (block.time - last)*MILLISECONDS as f64;
                }
                
                // issue_counts에서 해당 요청 제거
                issue_counts.remove(&key);
                
                current_qd = current_qd.saturating_sub(1);
                if current_qd == 0 {
                    last_complete_qd0_time = Some(block.time);
                }
                last_complete_time = Some(block.time);
            },
            _ => {}
        }
        block.qd = current_qd;
    }
    block_list
}

/// Vec<UFS>를 Arrow RecordBatch로 변환하는 함수
fn ufs_to_record_batch(ufs_list: &[UFS]) -> Result<RecordBatch, String> {
    // 각 필드별로 Arrow 배열 생성
    let time_array = Float64Array::from(ufs_list.iter().map(|u| u.time).collect::<Vec<f64>>());
    let process_array = StringArray::from(ufs_list.iter().map(|u| u.process.clone()).collect::<Vec<String>>());
    let cpu_array = UInt32Array::from(ufs_list.iter().map(|u| u.cpu).collect::<Vec<u32>>());
    let action_array = StringArray::from(ufs_list.iter().map(|u| u.action.clone()).collect::<Vec<String>>());
    let tag_array = UInt32Array::from(ufs_list.iter().map(|u| u.tag).collect::<Vec<u32>>());
    let opcode_array = StringArray::from(ufs_list.iter().map(|u| u.opcode.clone()).collect::<Vec<String>>());
    let lba_array = UInt64Array::from(ufs_list.iter().map(|u| u.lba).collect::<Vec<u64>>());
    let size_array = UInt32Array::from(ufs_list.iter().map(|u| u.size).collect::<Vec<u32>>());
    let groupid_array = UInt32Array::from(ufs_list.iter().map(|u| u.groupid).collect::<Vec<u32>>());
    let hwqid_array = UInt32Array::from(ufs_list.iter().map(|u| u.hwqid).collect::<Vec<u32>>());
    let qd_array = UInt32Array::from(ufs_list.iter().map(|u| u.qd).collect::<Vec<u32>>());
    let dtoc_array = Float64Array::from(ufs_list.iter().map(|u| u.dtoc).collect::<Vec<f64>>());
    let ctoc_array = Float64Array::from(ufs_list.iter().map(|u| u.ctoc).collect::<Vec<f64>>());
    let ctod_array = Float64Array::from(ufs_list.iter().map(|u| u.ctod).collect::<Vec<f64>>());
    let continues_array = BooleanArray::from(ufs_list.iter().map(|u| u.continuous).collect::<Vec<bool>>());

    // 스키마 정의
    let schema = Arc::new(Schema::new(vec![
        Field::new("time", DataType::Float64, false),
        Field::new("process", DataType::Utf8, false),
        Field::new("cpu", DataType::UInt32, false),
        Field::new("action", DataType::Utf8, false),
        Field::new("tag", DataType::UInt32, false),
        Field::new("opcode", DataType::Utf8, false),
        Field::new("lba", DataType::UInt64, false),
        Field::new("size", DataType::UInt32, false),
        Field::new("groupid", DataType::UInt32, false),
        Field::new("hwqid", DataType::UInt32, false),
        Field::new("qd", DataType::UInt32, false),
        Field::new("dtoc", DataType::Float64, false),
        Field::new("ctoc", DataType::Float64, false),
        Field::new("ctod", DataType::Float64, false),
        Field::new("continuous", DataType::Boolean, false)
    ]));

    // RecordBatch 생성
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(time_array) as ArrayRef,
            Arc::new(process_array) as ArrayRef,
            Arc::new(cpu_array) as ArrayRef,
            Arc::new(action_array) as ArrayRef,
            Arc::new(tag_array) as ArrayRef,
            Arc::new(opcode_array) as ArrayRef,
            Arc::new(lba_array) as ArrayRef,
            Arc::new(size_array) as ArrayRef,
            Arc::new(groupid_array) as ArrayRef,
            Arc::new(hwqid_array) as ArrayRef,
            Arc::new(qd_array) as ArrayRef,
            Arc::new(dtoc_array) as ArrayRef,
            Arc::new(ctoc_array) as ArrayRef,
            Arc::new(ctod_array) as ArrayRef,
            Arc::new(continues_array) as ArrayRef
        ],
    ).map_err(|e| e.to_string())
}

fn block_to_record_batch(block_list: &[Block]) -> Result<RecordBatch, String> {
    let time_array = Float64Array::from(
        block_list.iter().map(|b| b.time).collect::<Vec<_>>()
    );
    let process_array = StringArray::from(
        block_list.iter().map(|b| b.process.clone()).collect::<Vec<_>>()
    );
    let cpu_array = UInt32Array::from(
        block_list.iter().map(|b| b.cpu).collect::<Vec<_>>()
    );
    let flags_array = StringArray::from(
        block_list.iter().map(|b| b.flags.clone()).collect::<Vec<_>>()
    );    
    let action_array = StringArray::from(
        block_list.iter().map(|b| b.action.clone()).collect::<Vec<_>>()
    );
    let devmajor_array = UInt32Array::from(
        block_list.iter().map(|b| b.devmajor).collect::<Vec<_>>()
    );
    let devminor_array = UInt32Array::from(
        block_list.iter().map(|b| b.devminor).collect::<Vec<_>>()
    );
    let io_type_array = StringArray::from(
        block_list.iter().map(|b| b.io_type.clone()).collect::<Vec<_>>()
    );
    let extra_array = UInt32Array::from(
        block_list.iter().map(|b| b.extra).collect::<Vec<_>>()
    );
    let sector_array = UInt64Array::from(
        block_list.iter().map(|b| b.sector).collect::<Vec<_>>()
    );
    let size_array = UInt32Array::from(
        block_list.iter().map(|b| b.size).collect::<Vec<_>>()
    );
    let comm_array = StringArray::from(
        block_list.iter().map(|b| b.comm.clone()).collect::<Vec<_>>()
    );
    let qd_array = UInt32Array::from(
        block_list.iter().map(|b| b.qd).collect::<Vec<u32>>()
    );
    let dtoc_array = Float64Array::from(
        block_list.iter().map(|b| b.dtoc).collect::<Vec<f64>>()
    );
    let ctoc_array = Float64Array::from(
        block_list.iter().map(|b| b.ctoc).collect::<Vec<f64>>()
    );
    let ctod_array = Float64Array::from(
        block_list.iter().map(|b| b.ctod).collect::<Vec<f64>>()
    );
    let continuous_array = BooleanArray::from(
        block_list.iter().map(|b| b.continuous).collect::<Vec<bool>>()
    );
    
    let schema = Arc::new(Schema::new(vec![
        Field::new("time", DataType::Float64, false),
        Field::new("process", DataType::Utf8, false),
        Field::new("cpu", DataType::UInt32, false),
        Field::new("flags", DataType::Utf8, false),        
        Field::new("action", DataType::Utf8, false),
        Field::new("devmajor", DataType::UInt32, false),
        Field::new("devminor", DataType::UInt32, false),
        Field::new("io_type", DataType::Utf8, false),
        Field::new("extra", DataType::UInt32, false),
        Field::new("sector", DataType::UInt64, false),
        Field::new("size", DataType::UInt32, false),
        Field::new("comm", DataType::Utf8, false),
        Field::new("qd", DataType::UInt32, false),
        Field::new("dtoc", DataType::Float64, false),
        Field::new("ctoc", DataType::Float64, false),
        Field::new("ctod", DataType::Float64, false),
        Field::new("continuous", DataType::Boolean, false),
    ]));
    
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(time_array) as ArrayRef,
            Arc::new(process_array) as ArrayRef,
            Arc::new(cpu_array) as ArrayRef,
            Arc::new(flags_array) as ArrayRef,            
            Arc::new(action_array) as ArrayRef,
            Arc::new(devmajor_array) as ArrayRef,
            Arc::new(devminor_array) as ArrayRef,
            Arc::new(io_type_array) as ArrayRef,
            Arc::new(extra_array) as ArrayRef,
            Arc::new(sector_array) as ArrayRef,
            Arc::new(size_array) as ArrayRef,
            Arc::new(comm_array) as ArrayRef,
            Arc::new(qd_array) as ArrayRef,
            Arc::new(dtoc_array) as ArrayRef,
            Arc::new(ctoc_array) as ArrayRef,
            Arc::new(ctod_array) as ArrayRef,
            Arc::new(continuous_array) as ArrayRef,
        ],
    )
    .map_err(|e| e.to_string())
}

fn save_block_to_parquet(block_traces: &[Block], logfolder: String, fname: String, timestamp: &str) -> Result<String, String> {  
    let stem = PathBuf::from(&fname)
        .file_stem()
        .ok_or("잘못된 파일 이름")?
        .to_string_lossy()
        .to_string(); 
    let mut folder_path = PathBuf::from(logfolder);
    folder_path.push(&stem);
    create_dir_all(&folder_path).map_err(|e| e.to_string())?;

    let block_filename = format!("{}_block.parquet", timestamp);
    let mut path = folder_path;
    path.push(block_filename.clone());

    let batch = block_to_record_batch(block_traces)?;
    let schema = batch.schema();
    let file = File::create(&path).map_err(|e| e.to_string())?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None)
        .map_err(|e| e.to_string())?;
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

// fn generate_parquet_filename() -> String {
//     let now = Local::now();
//     format!("{}_ufs.parquet", now.format("%Y%m%d_%H%M%S"))
// }

// fn save_to_parquet(ufs_list: &[UFS], logfolder: String) -> Result<String, String> {
//     let filename = generate_parquet_filename();
//     let mut path = PathBuf::from(logfolder);
//     path.push(&filename);  // 참조로 변경

//     let batch = ufs_to_record_batch(ufs_list)?;
//     let schema = batch.schema();
//     let file = File::create(path).map_err(|e| e.to_string())?;
    
//     let mut writer = ArrowWriter::try_new(file, schema.clone(), None)
//         .map_err(|e| e.to_string())?;
    
//     writer.write(&batch).map_err(|e| e.to_string())?;
//     writer.close().map_err(|e| e.to_string())?;

//     Ok(filename)  // 생성된 파일명 반환
// }

fn save_ufs_to_parquet(ufs_list: &[UFS], logfolder: String, fname: String, timestamp: &str) -> Result<String, String> {  
    // logfolder 내에 stem 폴더 생성
    let stem = PathBuf::from(&fname)
        .file_stem()
        .ok_or("Invalid filename")?
        .to_string_lossy()
        .to_string();

    let mut folder_path = PathBuf::from(logfolder);
    folder_path.push(&stem);
    create_dir_all(&folder_path).map_err(|e| e.to_string())?;

    let ufs_filename = format!("{}_ufs.parquet", timestamp);
    let mut path = folder_path;
    path.push(&ufs_filename);

    let batch = ufs_to_record_batch(ufs_list)?;
    let schema = batch.schema();
    let file = File::create(&path).map_err(|e| e.to_string())?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None)
        .map_err(|e| e.to_string())?;
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn starttrace(fname: String, logfolder: String) -> Result<TraceParseResult, String> {
    let result = spawn_blocking(move || {
        let overall_start = Instant::now();
        let fileread_start = Instant::now();
        let file = File::open(&fname).map_err(|e| e.to_string())?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };
        let content = std::str::from_utf8(&mmap).map_err(|e| e.to_string())?;
        let fileread_time = fileread_start.elapsed();
        println!("File read time: {:?}", fileread_time);

        // 수정된 정규식 패턴 - [cpu] 앞의 모든 문자열을 process로 캡처
        // let pattern = r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)";
        // let re = Regex::new(pattern).map_err(|e| e.to_string())?;

        let parsing_start = Instant::now();

        // 기존 반복문 대신 아래와 같이 파싱을 병렬 처리합니다.
        let lines: Vec<&str> = content.lines().collect();

        let (ufs_vec, block_vec, missing_vec): (Vec<UFS>, Vec<Block>, Vec<usize>) = lines
            .par_iter()
            .enumerate()
            .map(|(i, line)| {
                let line_number = i + 1;
                if line.trim().is_empty() {
                    return (Vec::new(), Vec::new(), vec![line_number]);
                } 
                if let Ok(ufs) = parse_ufs_trace(line) {
                    (vec![ufs], Vec::new(), Vec::new())
                } else if let Ok(block) = parse_block_trace(line) {
                    (Vec::new(), vec![block], Vec::new())
                } else {
                    (Vec::new(), Vec::new(), vec![line_number])
                }
            })
            .reduce(
                || (Vec::new(), Vec::new(), Vec::new()),
                |(mut acc_ufs, mut acc_block, mut acc_missing), (ufs_vec, block_vec, missing_vec)| {
                    acc_ufs.extend(ufs_vec);
                    acc_block.extend(block_vec);
                    acc_missing.extend(missing_vec);
                    (acc_ufs, acc_block, acc_missing)
                },
            );
        // ufs_vec, block_vec에 파싱된 결과가, missing_vec에 파싱 실패한 줄 번호가 들어갑니다.
        let ufs_list = ufs_vec;
        let block_list = block_vec;
        let missing_lines = missing_vec;
        let parsing_time = parsing_start.elapsed();
        println!("Parsing time: {:?}", parsing_time);
        let bottom_half_start = Instant::now();
        // Bottom half: latency 계산 처리
        let processed_ufs_list = ufs_bottom_half_latency_process(ufs_list);
        let processed_block_list = block_bottom_half_latency_process(block_list);
        let bottom_half_time = bottom_half_start.elapsed();
        println!("Bottom half time: {:?}", bottom_half_time);


        // let save_parquet_start = Instant::now();
        // // 파싱된 결과를 parquet 파일로 저장
        // let filename = save_to_parquet(&processed_ufs_list, logfolder)?;
        // let save_parquet_time = save_parquet_start.elapsed();
        // println!("Save parquet time: {:?}", save_parquet_time);

        // 공통 timestamp 생성
        let now = Local::now();
        let timestamp = now.format("%Y%m%d_%H%M%S").to_string();

        let save_parquet_start = Instant::now();
        // 파싱된 UFS 로그를 parquet 파일로 저장 (지정한 폴더 내에)
        let ufs_parquet_filename = save_ufs_to_parquet(&processed_ufs_list, logfolder.clone(), fname.clone(), &timestamp)?;
        let ufs_save_time = save_parquet_start.elapsed();
        println!("Save UFS parquet time: {:?}", ufs_save_time);

        let block_save_start = Instant::now();
        // Block trace 로그를 parquet 파일로 저장 (같은 폴더 내에)
        let block_parquet_filename = save_block_to_parquet(&processed_block_list, logfolder.clone(), fname.clone(), &timestamp)?;
        let block_save_time = block_save_start.elapsed();
        println!("Save Block trace parquet time: {:?}", block_save_time);

        let overall_time = overall_start.elapsed();
        println!("Overall time: {:?}", overall_time);
        Ok(TraceParseResult {
            missing_lines,
            ufs_parquet_filename,
            block_parquet_filename,
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    result
}
#[derive(Serialize, Debug, Clone)]
pub struct LatencySummary {
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub median: f64,
    pub std_dev: f64,
    pub _99th: f64,
    pub _99_9th: f64,
    pub _99_99th: f64,
    pub _99_999th: f64,
    pub _99_9999th: f64,
    pub percentiles: HashMap<String, f64>,
}

// summary를 opcode별로 저장하도록 수정
#[derive(Serialize, Debug, Clone)]
pub struct LatencyStats {
    pub latency_counts: BTreeMap<String, BTreeMap<String, usize>>,
    pub summary: Option<BTreeMap<String, LatencySummary>>,
}

// 백분위수 계산을 위한 헬퍼 함수 수정
fn calculate_percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let n = sorted_values.len();
    let rank = ((n - 1) as f64 * percentile).floor() as usize;
    if rank + 1 < n {
        let weight = ((n - 1) as f64 * percentile) - rank as f64;
        sorted_values[rank] * (1.0 - weight) + sorted_values[rank + 1] * weight
    } else {
        sorted_values[rank]
    }
}

// 통계 계산을 위한 헬퍼 함수 추가
fn calculate_statistics(values: &mut Vec<f64>) -> LatencySummary {
    values.sort_by(|a, b| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal));
    
    let n = values.len();
    if n == 0 {
        return LatencySummary {
            sum: 0.0,            
            min: 0.0,
            max: 0.0,
            avg: 0.0,
            median: 0.0,
            std_dev: 0.0,
            _99th: 0.0,
            _99_9th: 0.0,
            _99_99th: 0.0,
            _99_999th: 0.0,
            _99_9999th: 0.0,
            percentiles: HashMap::new(),
        };
    }

    let sum: f64 = values.iter().sum();    
    let avg = sum / n as f64;
    
    // 분산과 표준편차 계산
    let variance = values.iter()
        .map(|&x| (x - avg).powi(2))
        .sum::<f64>() / n as f64;
    let std_dev = variance.sqrt();
    
    // 중앙값 계산
    let median = if n % 2 == 0 {
        (values[n/2 - 1] + values[n/2]) / 2.0
    } else {
        values[n/2]
    };

    // 백분위수 계산
    let percents = [0.99, 0.999, 0.9999, 0.99999, 0.999999];
    let percentile_names = ["99", "99.9", "99.99", "99.999", "99.9999"];
    let mut percentiles = HashMap::with_capacity(percents.len());
    
    for (&p, name) in percents.iter().zip(percentile_names.iter()) {
        let value = calculate_percentile(values, p);
        percentiles.insert(format!("{}th", name), value);
    }

    LatencySummary {
        sum,
        min: values[0],
        max: values[n-1],
        avg,
        median,
        std_dev,
        _99th: percentiles["99th"],
        _99_9th: percentiles["99.9th"],
        _99_99th: percentiles["99.99th"],
        _99_999th: percentiles["99.999th"],
        _99_9999th: percentiles["99.9999th"],
        percentiles,
    }
}

/// 시간 문자열을 밀리초 단위의 숫자로 변환하는 함수
fn parse_time_to_ms(time_str: &str) -> Result<f64, String> {
    let mut num_str = String::new();
    let mut unit_str = String::new();
    
    // 숫자와 단위 분리
    for c in time_str.chars() {
        if c.is_digit(10) || c == '.' {
            num_str.push(c);
        } else {
            unit_str.push(c);
        }
    }

    let number: f64 = num_str.parse::<f64>().map_err(|e| e.to_string())?;
    
    // 단위에 따른 변환 (모든 케이스 처리)
    let ms = match unit_str.as_str() {
        "ms" => number,  // 밀리초는 그대로
        "s" => number * 1000.0,  // 초를 밀리초로
        "us" => number / 1000.0,  // 마이크로초를 밀리초로
        "ns" => number / 1_000_000.0,  // 나노초를 밀리초로
        other => return Err(format!("Unsupported time unit: {}", other))
    };

    Ok(ms)
}

#[tauri::command]
pub async fn latencystats(
    logname: String,
    column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>  // 문자열 타입으로 변경
) -> Result<String, String> {
    // threshold 문자열을 숫자로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &thresholds {
        let ms = parse_time_to_ms(&t)?;
        threshold_values.push(ms);
    }

    // 캐시된 데이터 가져오기 및 필터링
    let cache_key = format!("{}", logname);
    println!("Cache key: {}", cache_key);
    let cached_ufs_list = {
        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(&cache_key).ok_or("Cache not found")?.clone()
    };

    // 시간 필터링
    let time_filtered: Vec<UFS>;
    if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        time_filtered = cached_ufs_list.into_iter()
            .filter(|ufs| ufs.time >= t_from && ufs.time <= t_to)
            .collect();
    } else {
        time_filtered = cached_ufs_list;
    }

    // ChartStat 생성 
    let mut chart_stats = match column.as_str() {
        "dtoc" | "ctoc" => time_filtered
            .iter()
            .filter(|ufs| ufs.action == "complete_rsp")
            .map(|ufs| ChartStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: if column == "dtoc" {
                    ChartValue::F64(ufs.dtoc)
                } else {
                    ChartValue::F64(ufs.ctoc)
                },
            })
            .collect::<Vec<_>>(),
        "ctod" => time_filtered
            .iter()
            .filter(|ufs| ufs.action == "send_req")
            .map(|ufs| ChartStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: ChartValue::F64(ufs.ctod),
            })
            .collect::<Vec<_>>(),
        _ => return Err("Invalid column".to_string()),
    };

    // value 범위 필터링
    if let Some(v_from) = col_from {
        if v_from != 0.0 {
            chart_stats.retain(|s| s.value.as_f64() >= v_from);
        }
    }
    if let Some(v_to) = col_to {
        if v_to != 0.0 {
            chart_stats.retain(|s| s.value.as_f64() <= v_to);
        }
    }

    // 시간순 정렬
    chart_stats.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());

    // opcode별 latency count 초기화
    let mut latency_counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    
    // 모든 opcode에 대해 threshold 구간 초기화
    let unique_opcodes: Vec<String> = chart_stats.iter()
        .map(|stat| stat.opcode.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for opcode in &unique_opcodes {
        let mut ranges = BTreeMap::new();
        if threshold_values.is_empty() {
            ranges.insert("전체".to_string(), 0);
        } else {
            // 첫 번째 구간
            ranges.insert(format!("≤ {}", thresholds[0]), 0);
            
            // 중간 구간들
            for i in 0..thresholds.len()-1 {
                // let key = format!("{} < v ≤ {}", thresholds[i], thresholds[i+1]);
                let key = format!("{} < v ≤ {}", thresholds[i], thresholds[i+1]);
                ranges.insert(key, 0);
            }
            
            // 마지막 구간
            ranges.insert(format!("99_> {}", thresholds.last().unwrap()), 0);
        }
        latency_counts.insert(opcode.clone(), ranges);
    }

    // 각 데이터에 대해 해당하는 opcode의 구간 카운트 증가
    for stat in &chart_stats {
        let latency = stat.value.as_f64();
        let range_key = if threshold_values.is_empty() {
            "전체".to_string()
        } else if latency <= threshold_values[0] {
            format!("01_≤ {}", thresholds[0])
        } else if latency > *threshold_values.last().unwrap() {
            format!("99_> {}", thresholds.last().unwrap())
        } else {
            // threshold_values.windows(2)
            //     .zip(thresholds.windows(2))
            //     .find(|(vals, _)| latency > vals[0] && latency <= vals[1])
            //     // .map(|(_, units)| format!("{} < v ≤ {}", units[0], units[1]))
            //     .map(|(_, units)| format!("> {}", units[0]))
            //     .unwrap_or_default();
            let mut key = String::new();
            // enumerate를 통해 인덱스를 활용하여 접두어 생성
            for (i, vals) in threshold_values.windows(2).enumerate() {
                if latency > vals[0] && latency <= vals[1] {
                    key = format!("{} < v ≤ {}", thresholds[i], thresholds[i+1]);
                    break;
                }
            }
            key
        };

        if let Some(opcode_ranges) = latency_counts.get_mut(&stat.opcode) {
            if let Some(count) = opcode_ranges.get_mut(&range_key) {
                *count += 1;
            }
        }
    }

    // opcode별 그룹핑
    let mut opcode_groups = BTreeMap::new();
    for stat in &chart_stats {
        opcode_groups.entry(stat.opcode.clone())
            .or_insert_with(Vec::new)
            .push(stat.value.as_f64());
    }

    // 각 그룹별로 통계 계산
    let mut summary_map = BTreeMap::new();
    for (opcode, mut values) in opcode_groups {
        let summary = calculate_statistics(&mut values);
        summary_map.insert(opcode, summary);
    }

    let result = LatencyStats {
        latency_counts,
        summary: Some(summary_map)
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[derive(Serialize, Debug, Clone)]
pub struct SizeStats {
    pub opcode_stats: BTreeMap<String, BTreeMap<u32, usize>>,
    pub total_counts: BTreeMap<String, usize>,
}

#[tauri::command]
pub async fn sizestats(
    logname: String,
    column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    // 캐시된 데이터 가져오기
    let cache_key = format!("{}", logname);
    let cached_ufs_list = {
        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(&cache_key).ok_or("Cache not found")?.clone()
    };

    // 관심있는 opcode들
    let target_opcodes = ["0x2a", "0x28", "0x42"];

    // 시간 필터링
    let time_filtered: Vec<UFS> = match (time_from, time_to) {
        (Some(t_from), Some(t_to)) => cached_ufs_list
            .into_iter()
            .filter(|ufs| ufs.time >= t_from && ufs.time <= t_to)
            .collect(),
        _ => cached_ufs_list,
    };

    // column에 따른 필터링 적용
    let filtered_ufs: Vec<&UFS> = time_filtered
        .iter()
        .filter(|ufs| {
            let (is_valid, value) = match column.as_str() {
                "dtoc" => (ufs.action == "complete_rsp", ufs.dtoc),
                "ctoc" => (ufs.action == "complete_rsp", ufs.ctoc),
                "ctod" => (ufs.action == "send_req", ufs.ctod),
                "lba" => (ufs.action == "send_req", ufs.lba as f64),
                _ => return false,
            };

            if !is_valid {
                return false;
            }

            // col_from, col_to 필터링
            match (col_from, col_to) {
                (Some(from), Some(to)) if from != 0.0 && to != 0.0 => value >= from && value <= to,
                (Some(from), _) if from != 0.0 => value >= from,
                (_, Some(to)) if to != 0.0 => value <= to,
                _ => true,
            }
        })
        .filter(|ufs| target_opcodes.contains(&ufs.opcode.as_str()))
        .collect();

    // opcode별 size 통계
    let mut opcode_stats: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut total_counts: BTreeMap<String, usize> = BTreeMap::new();

    // 각 opcode에 대한 빈 HashMap 초기화
    for opcode in target_opcodes.iter() {
        opcode_stats.insert(opcode.to_string(), BTreeMap::new());
        total_counts.insert(opcode.to_string(), 0);
    }

    // size별 count 계산
    for ufs in filtered_ufs {
        if let Some(size_counts) = opcode_stats.get_mut(&ufs.opcode) {
            *size_counts.entry(ufs.size).or_insert(0) += 1;
            *total_counts.get_mut(&ufs.opcode).unwrap() += 1;
        }
    }

    let result = SizeStats {
        opcode_stats,
        total_counts,
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

fn normalize_io_type(io: &str) -> String {
    io.chars().next().unwrap_or_default().to_string()
}

#[tauri::command]
pub async fn block_latencystats(
    logname: String,
    column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
    group: bool
) -> Result<String, String> {
    // threshold 문자열을 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &thresholds {
        let ms = parse_time_to_ms(&t)?;
        threshold_values.push(ms);
    }

    // block 캐시에서 데이터를 가져오기
    let cache_key = logname;
    let cached_block_list = {
        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(&cache_key).ok_or("Block cache not found")?.clone()
    };

    // 시간 범위 필터링
    let time_filtered: Vec<Block> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        cached_block_list.into_iter().filter(|b| b.time >= t_from && b.time <= t_to).collect()
    } else {
        cached_block_list
    };

    // column에 따라 적절한 action 및 값을 선택
    let chart_stats: Vec<ChartStat> = match column.as_str() {
        "dtoc" | "ctoc" => time_filtered
            .iter()
            .filter(|b| b.action == "block_rq_complete")
            .map(|b| ChartStat {
                time: b.time,
                // grouping key로 io_type 사용
                opcode: if group { normalize_io_type(&b.io_type) } else { b.io_type.clone() },
                value: if column == "dtoc" {
                    ChartValue::F64(b.dtoc)
                } else {
                    ChartValue::F64(b.ctoc)
                },
            })
            .collect(),
        "ctod" => time_filtered
            .iter()
            .filter(|b| b.action == "block_rq_issue")
            .map(|b| ChartStat {
                time: b.time,
                opcode: if group { normalize_io_type(&b.io_type) } else { b.io_type.clone() },
                value: ChartValue::F64(b.ctod),
            })
            .collect(),
        _ => return Err("Invalid column for block latencystats".to_string()),
    };

    // value 범위 필터링
    let mut filtered_stats = chart_stats;
    if let Some(v_from) = col_from {
        if v_from != 0.0 {
            filtered_stats.retain(|s| s.value.as_f64() >= v_from);
        }
    }
    if let Some(v_to) = col_to {
        if v_to != 0.0 {
            filtered_stats.retain(|s| s.value.as_f64() <= v_to);
        }
    }
    filtered_stats.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());

    // io_type별 latency count 초기화
    let unique_io_types: Vec<String> = filtered_stats
        .iter()
        .map(|s| s.opcode.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut latency_counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for io in &unique_io_types {
        let mut ranges = BTreeMap::new();
        if threshold_values.is_empty() {
            ranges.insert("전체".to_string(), 0);
        } else {
            ranges.insert(format!("≤ {}", thresholds[0]), 0);
            for i in 0..thresholds.len()-1 {
                let key = format!("{} < v ≤ {}", thresholds[i], thresholds[i+1]);
                ranges.insert(key, 0);
            }
            ranges.insert(format!("> {}", thresholds.last().unwrap()), 0);
        }
        latency_counts.insert(io.clone(), ranges);
    }

    // 각 데이터에 대해 해당 io_type의 구간 카운트 증가
    for stat in &filtered_stats {
        let latency = stat.value.as_f64();
        let range_key = if threshold_values.is_empty() {
            "전체".to_string()
        } else if latency <= threshold_values[0] {
            format!("≤ {}", thresholds[0])
        } else if latency > *threshold_values.last().unwrap() {
            format!("> {}", thresholds.last().unwrap())
        } else {
            // let mut key = String::new();
            // for win in threshold_values.windows(2).zip(thresholds.windows(2)) {
            //     let (vals, units) = win;
            //     if latency > vals[0] && latency <= vals[1] {
            //         key = format!("{} < v ≤ {}", units[0], units[1]);                    
            //         break;
            //     }
            // }
            // key
            let mut key = String::new();
            // enumerate를 통해 인덱스를 활용하여 접두어 생성
            for (i, vals) in threshold_values.windows(2).enumerate() {
                if latency > vals[0] && latency <= vals[1] {
                    key = format!("{:02}_{} < v ≤ {}", i+2, thresholds[i], thresholds[i+1]);
                    break;
                }
            }
            key
        };

        if let Some(io_counts) = latency_counts.get_mut(&stat.opcode) {
            if let Some(count) = io_counts.get_mut(&range_key) {
                *count += 1;
            }
        }
    }

    // io_type별 그룹핑 후 통계 계산
    let mut io_groups = BTreeMap::new();
    for stat in &filtered_stats {
        io_groups.entry(stat.opcode.clone())
            .or_insert_with(Vec::new)
            .push(stat.value.as_f64());
    }

    let mut summary_map = BTreeMap::new();
    for (io, mut values) in io_groups {
        let summary = calculate_statistics(&mut values);
        summary_map.insert(io, summary);
    }

    let result = LatencyStats {
        latency_counts,
        summary: Some(summary_map),
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn block_sizestats(
    logname: String,
    column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    group: bool,
) -> Result<String, String> {
    // block 캐시에서 데이터 가져오기
    let cache_key = logname;
    let cached_block_list = {
        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(&cache_key).ok_or("Block cache not found")?.clone()
    };

    // 시간 범위 필터링
    let time_filtered: Vec<Block> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        cached_block_list.into_iter().filter(|b| b.time >= t_from && b.time <= t_to).collect()
    } else {
        cached_block_list
    };

    // column에 따라 필터링: "dtoc", "ctoc", "ctod", "sector" 등 (필요한 경우 확장)
    let filtered_blocks: Vec<&Block> = time_filtered.iter().filter(|b| {
        let (is_valid, value) = match column.as_str() {
            "dtoc" => (b.action == "block_rq_complete", b.dtoc),
            "ctoc" => (b.action == "block_rq_complete", b.ctoc),
            "ctod" => (b.action == "block_rq_issue", b.ctod),
            "sector" => (b.action == "block_rq_issue", b.sector as f64),
            _ => return false,
        };
        if !is_valid {
            return false;
        }
        match (col_from, col_to) {
            (Some(from), Some(to)) if from != 0.0 && to != 0.0 => value >= from && value <= to,
            (Some(from), _) if from != 0.0 => value >= from,
            (_, Some(to)) if to != 0.0 => value <= to,
            _ => true,
        }
    }).collect();

    // block trace에서는 opcode 대신 io_type으로 그룹핑
    let target_io_types: Vec<String> = filtered_blocks
        .iter()
        .map(|b| if group { normalize_io_type(&b.io_type) } else { b.io_type.clone() },)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut io_stats: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut total_counts: BTreeMap<String, usize> = BTreeMap::new();

    // 각 io_type별로 빈 카운트 맵 초기화
    for io in &target_io_types {
        io_stats.insert(io.clone(), BTreeMap::new());
        total_counts.insert(io.clone(), 0);
    }

    // size 기준 count 계산 (필요시 다른 column으로 확장 가능)
    for block in filtered_blocks {
        if let Some(size_counts) = io_stats.get_mut(&block.io_type) {
            *size_counts.entry(block.size).or_insert(0) += 1;
            *total_counts.get_mut(&block.io_type).unwrap() += 1;
        }
    }

    let result = SizeStats {
        // UFS에서 사용한 필드명이 opcode_stats이지만, block에서는 io_type을 사용합니다.
        opcode_stats: io_stats,
        total_counts,
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}