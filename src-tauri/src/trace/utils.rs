use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::PathBuf;

use chrono::Local;
use datafusion::prelude::*;
use memmap2::Mmap;
use rand::prelude::*;
use rayon::prelude::*;
use tauri::async_runtime::spawn_blocking;
use tauri::Emitter;

use datafusion::functions_aggregate::expr_fn::count;

use serde::Serialize;

// 수정된 imports - 올바른 경로 사용
use arrow::datatypes::DataType;
use datafusion::arrow::array::{Float64Array, Float32Array, Int64Array, UInt64Array, UInt32Array, StringViewArray};
use datafusion::logical_expr::{col, lit};
use serde_json::json;

use crate::trace::block::{block_bottom_half_latency_process, save_block_to_parquet};
use crate::trace::ufs::{save_ufs_to_parquet, ufs_bottom_half_latency_process};
use crate::trace::{Block, LatencySummary, TraceParseResult, BLOCK_CACHE, UFS, UFS_CACHE, ProgressEvent, CANCEL_SIGNAL};

use crate::trace::filter::{filter_block_data, filter_ufs_data};

use super::{ACTIVE_BLOCK_PATTERN, ACTIVE_UFS_PATTERN};

// 샘플링 결과를 담는 구조체
#[derive(Serialize, Debug, Clone)]
pub struct SamplingInfo<T> {
    pub data: Vec<T>,
    pub total_count: usize,
    pub sampled_count: usize,
    pub sampling_ratio: f64,
}

// 샘플링 함수들 - max_records 매개변수 추가
pub fn sample_ufs(ufs_list: &[UFS], max_records: usize) -> SamplingInfo<UFS> {
    let total_count = ufs_list.len();

    if total_count <= max_records {
        // 샘플링이 필요 없는 경우
        SamplingInfo {
            data: ufs_list.to_vec(),
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    } else {
        // 샘플링이 필요한 경우
        let mut rng = rand::thread_rng();
        let sampled_data = ufs_list
            .choose_multiple(&mut rng, max_records)
            .cloned()
            .collect();

        let sampling_ratio = (max_records as f64 / total_count as f64) * 100.0;

        SamplingInfo {
            data: sampled_data,
            total_count,
            sampled_count: max_records,
            sampling_ratio,
        }
    }
}

pub fn sample_block(block_list: &[Block], max_records: usize) -> SamplingInfo<Block> {
    let total_count = block_list.len();

    if total_count <= max_records {
        // 샘플링이 필요 없는 경우
        SamplingInfo {
            data: block_list.to_vec(),
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    } else {
        // 샘플링이 필요한 경우
        let mut rng = rand::thread_rng();
        let sampled_data = block_list
            .choose_multiple(&mut rng, max_records)
            .cloned()
            .collect();

        let sampling_ratio = (max_records as f64 / total_count as f64) * 100.0;

        SamplingInfo {
            data: sampled_data,
            total_count,
            sampled_count: max_records,
            sampling_ratio,
        }
    }
}

// 백분위수 계산을 위한 헬퍼 함수
pub fn calculate_percentile(sorted_values: &[f64], percentile: f64) -> f64 {
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

// 통계 계산을 위한 헬퍼 함수
pub fn calculate_statistics(values: &mut Vec<f64>) -> LatencySummary {
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
            percentiles: HashMap::new(),
        };
    }

    let sum: f64 = values.iter().sum();
    let avg = sum / n as f64;

    // 분산과 표준편차 계산
    let variance = values.iter().map(|&x| (x - avg).powi(2)).sum::<f64>() / n as f64;
    let std_dev = variance.sqrt();

    // 중앙값 계산
    let median = if n % 2 == 0 {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
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
        max: values[n - 1],
        avg,
        median,
        std_dev,
        percentiles,
    }
}

/// 시간 문자열을 밀리초 단위의 숫자로 변환하는 함수
pub fn parse_time_to_ms(time_str: &str) -> Result<f64, String> {
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
        "ms" => number,               // 밀리초는 그대로
        "s" => number * 1000.0,       // 초를 밀리초로
        "us" => number / 1000.0,      // 마이크로초를 밀리초로
        "ns" => number / 1_000_000.0, // 나노초를 밀리초로
        other => return Err(format!("Unsupported time unit: {}", other)),
    };

    Ok(ms)
}

// io_type의 첫 글자만 사용하는 정규화 함수
pub fn normalize_io_type(io: &str) -> String {
    io.chars().next().unwrap_or_default().to_string()
}

// 구간 키 생성 함수 - latencystats에서 중복 사용
pub fn create_range_key(latency: f64, threshold_values: &[f64], thresholds: &[String]) -> String {
    if threshold_values.is_empty() {
        "전체".to_string()
    } else if latency <= threshold_values[0] {
        format!("01_≤ {}", thresholds[0])
    } else if latency > *threshold_values.last().unwrap() {
        format!("99_> {}", thresholds.last().unwrap())
    } else {
        // 중간 구간 결정
        for (i, vals) in threshold_values.windows(2).enumerate() {
            if latency > vals[0] && latency <= vals[1] {
                return format!("{:02}_{} < v ≤ {}", i + 2, thresholds[i], thresholds[i + 1]);
            }
        }
        String::new() // 매칭되는 구간이 없는 경우
    }
}

// 구간 매핑 초기화 함수
pub fn initialize_ranges(thresholds: &[String]) -> BTreeMap<String, usize> {
    let mut ranges = BTreeMap::new();
    if thresholds.is_empty() {
        ranges.insert("전체".to_string(), 0);
    } else {
        // 첫 번째 구간
        ranges.insert(format!("01_≤ {}", thresholds[0]), 0);

        // 중간 구간들
        for i in 0..thresholds.len() - 1 {
            let key = format!("{:02}_{} < v ≤ {}", i + 2, thresholds[i], thresholds[i + 1]);
            ranges.insert(key, 0);
        }

        // 마지막 구간
        ranges.insert(format!("99_> {}", thresholds.last().unwrap()), 0);
    }
    ranges
}

// readtrace 함수 - max_records 매개변수 추가
pub async fn readtrace(logname: String, max_records: usize) -> Result<String, String> {
    // 캐시 확인: 두 캐시 모두 있는지 확인
    {
        let ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        let block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;

        if ufs_cache.contains_key(&logname) || block_cache.contains_key(&logname) {
            // Create longer-lived empty vectors to use as default values
            let empty_ufs_vec: Vec<UFS> = Vec::new();
            let empty_block_vec: Vec<Block> = Vec::new();

            let ufs_data = ufs_cache.get(&logname).unwrap_or(&empty_ufs_vec);
            let block_data = block_cache.get(&logname).unwrap_or(&empty_block_vec);

            // 캐시된 데이터를 샘플링
            let ufs_sample_info = sample_ufs(ufs_data, max_records);
            let block_sample_info = sample_block(block_data, max_records);

            let result_json = serde_json::json!({
                "ufs": {
                    "data": ufs_sample_info.data,
                    "total_count": ufs_sample_info.total_count,
                    "sampled_count": ufs_sample_info.sampled_count,
                    "sampling_ratio": ufs_sample_info.sampling_ratio
                },
                "block": {
                    "data": block_sample_info.data,
                    "total_count": block_sample_info.total_count,
                    "sampled_count": block_sample_info.sampled_count,
                    "sampling_ratio": block_sample_info.sampled_count
                }
            });
            return Ok(result_json.to_string());
        }
    }

    let mut ufs_vec: Vec<UFS> = Vec::new();
    let mut block_vec: Vec<Block> = Vec::new();

    // logname에 쉼표가 있으면 각각의 파일 경로로 분리, 없으면 하나의 경로로 처리
    let files: Vec<String> = if logname.contains(',') {
        logname.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![logname.clone()]
    };

    // DataFusion context 생성 및 옵션 설정
    let config = SessionConfig::new()
        .with_batch_size(8192);  // 메모리 효율성을 위해 배치 크기 조정
    
    // 최신 DataFusion 버전에 맞게 SessionContext 생성
    let ctx = SessionContext::new_with_config(config);

    // 각 파일 처리: 파일명에 따라 ufs 또는 block으로 구분
    for file in files {
        let path = PathBuf::from(&file);
        if !path.is_file() {
            continue; // 파일이 아니면 건너뜁니다.
        }

        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            if fname.contains("ufs") && fname.ends_with(".parquet") {
                // UFS parquet 파일 읽기 - 메모리 효율적인 스트리밍 방식으로
                println!("Processing UFS file: {}", path.display());
                
                // 메타데이터를 통한 파일 크기 확인
                let file_size = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata.len(),
                    Err(_) => 0,
                };
                println!("File size: {} bytes", file_size);

                // 최신 DataFusion API에 맞게 수정
                let read_options = ParquetReadOptions::default();
                
                let df = ctx
                    .read_parquet(
                        path.to_str().ok_or("Invalid path")?,
                        read_options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;

                // UFS 배치 처리
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

                    // 각 칼럼 배열 다운캐스팅
                    let time_array = batch
                        .column(time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'time'")?;
                    let process_array = batch
                        .column(process_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'process'")?;
                    let cpu_array = batch
                        .column(cpu_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'cpu'")?;
                    let action_array = batch
                        .column(action_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'action'")?;
                    let tag_array = batch
                        .column(tag_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'tag'")?;
                    let opcode_array = batch
                        .column(opcode_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'opcode'")?;
                    let lba_array = batch
                        .column(lba_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt64Array>()
                        .ok_or("Failed to downcast 'lba'")?;
                    let size_array = batch
                        .column(size_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'size'")?;
                    let groupid_array = batch
                        .column(groupid_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'groupid'")?;
                    let hwqid_array = batch
                        .column(hwqid_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'hwqid'")?;
                    let qd_array = batch
                        .column(qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'qd'")?;
                    let dtoc_array = batch
                        .column(dtoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'dtoc'")?;
                    let ctoc_array = batch
                        .column(ctoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctoc'")?;
                    let ctod_array = batch
                        .column(ctod_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctod'")?;
                    let cont_array = batch
                        .column(cont_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::BooleanArray>()
                        .ok_or("Failed to downcast 'continuous'")?;

                    // 배열에서 값을 추출하여 UFS 객체 생성
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

                // 캐시에 저장
                {
                    let mut ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
                    let ufspath = path.to_string_lossy().to_string();
                    ufs_cache.insert(ufspath, ufs_vec.clone());
                }
            } else if fname.contains("block") && fname.ends_with(".parquet") {
                // block parquet 파일 읽기 - 메모리 효율적인 스트리밍 방식으로
                println!("Processing block file: {}", path.display());
                
                // 메타데이터를 통한 파일 크기 확인
                let file_size = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata.len(),
                    Err(_) => 0,
                };
                println!("File size: {} bytes", file_size);

                // 최신 DataFusion API에 맞게 수정
                let read_options = ParquetReadOptions::default();
                
                let df = ctx
                    .read_parquet(
                        path.to_str().ok_or("Invalid path")?,
                        read_options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;

                let batches = df.collect().await.map_err(|e| e.to_string())?;
                for batch in batches {
                    let num_rows = batch.num_rows();
                    let schema = batch.schema();

                    // 컬럼 인덱스 추출 및 배열 다운캐스팅 처리
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

                    // 각 칼럼 배열 다운캐스팅
                    let time_array = batch
                        .column(time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'time'")?;
                    let process_array = batch
                        .column(process_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'process'")?;
                    let cpu_array = batch
                        .column(cpu_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'cpu'")?;
                    let flags_array = batch
                        .column(flags_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'flags'")?;
                    let action_array = batch
                        .column(action_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'action'")?;
                    let devmajor_array = batch
                        .column(devmajor_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'devmajor'")?;
                    let devminor_array = batch
                        .column(devminor_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'devminor'")?;
                    let io_type_array = batch
                        .column(io_type_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'io_type'")?;
                    let extra_array = batch
                        .column(extra_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'extra'")?;
                    let sector_array = batch
                        .column(sector_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt64Array>()
                        .ok_or("Failed to downcast 'sector'")?;
                    let size_array = batch
                        .column(size_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'size'")?;
                    let comm_array = batch
                        .column(comm_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::StringViewArray>()
                        .ok_or("Failed to downcast 'comm'")?;
                    let qd_array = batch
                        .column(qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'qd'")?;
                    let dtoc_array = batch
                        .column(dtoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'dtoc'")?;
                    let ctoc_array = batch
                        .column(ctoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctoc'")?;
                    let ctod_array = batch
                        .column(ctod_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'ctod'")?;
                    let cont_array = batch
                        .column(cont_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::BooleanArray>()
                        .ok_or("Failed to downcast 'continuous'")?;

                    // 배열에서 값을 추출하여 Block 객체 생성
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

                // 캐시에 저장
                {
                    let mut block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
                    let blockpath = path.to_string_lossy().to_string();
                    block_cache.insert(blockpath, block_vec.clone());
                }
            }
        }
    }

    // 데이터가 많은 경우 샘플링하여 반환
    let ufs_sample_info = sample_ufs(&ufs_vec, max_records);
    let block_sample_info = sample_block(&block_vec, max_records);

    // JSON으로 직렬화하여 반환
    let result_json = serde_json::json!({
        "ufs": {
            "data": ufs_sample_info.data,
            "total_count": ufs_sample_info.total_count,
            "sampled_count": ufs_sample_info.sampled_count,
            "sampling_ratio": ufs_sample_info.sampling_ratio
        },
        "block": {
            "data": block_sample_info.data,
            "total_count": block_sample_info.total_count,
            "sampled_count": block_sample_info.sampled_count,
            "sampling_ratio": block_sample_info.sampled_count
        }
    });

    Ok(result_json.to_string())
}

// 로그 파일 파싱 및 parquet 저장 함수
pub async fn starttrace(fname: String, logfolder: String, window: tauri::Window) -> Result<TraceParseResult, String> {
    let result = spawn_blocking(move || {
        // 파일 정보 확인
        let file_meta = match std::fs::metadata(&fname) {
            Ok(meta) => meta,
            Err(e) => return Err(format!("파일 메타데이터 읽기 실패: {}", e)),
        };
        
        // 파일 크기 확인 및 출력
        let file_size = file_meta.len();
        println!("로그 파일 크기: {} bytes ({:.2} GB)", file_size, file_size as f64 / 1_073_741_824.0);
        
        // 진행 상태 초기 이벤트 전송
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "init".to_string(),
            progress: 0.0,
            current: 0,
            total: 100,
            message: "로그 파일 분석 시작".to_string(),
            eta_seconds: 0.0,
            processing_speed: 0.0,
        });

        // 메모리 맵 방식 또는 일반 파일 읽기 선택
        let content: String;
        
        if file_size > 1_000_000_000 {  // 1GB 이상은 스트리밍 방식으로 처리
            println!("대용량 파일 감지: 스트리밍 방식으로 처리합니다");
            
            // 파일 라인 수 예측 (샘플링)
            let sample_size = 1024 * 1024;  // 1MB 샘플
            let file = File::open(&fname).map_err(|e| e.to_string())?;
            let mut sample_buffer = vec![0; sample_size.min(file_size as usize)];
            let mut reader = std::io::BufReader::new(file);
            use std::io::Read;
            let read_bytes = reader.read(&mut sample_buffer).map_err(|e| e.to_string())?;
            
            // 샘플에서 라인 수 계산
            let sample_lines = sample_buffer[..read_bytes].iter().filter(|&&b| b == b'\n').count();
            let estimated_lines = (sample_lines as f64 / read_bytes as f64) * file_size as f64;
            println!("예상 라인 수: {:.0}", estimated_lines);
            
            // 진행 상태 업데이트: 파일 읽기 시작
            let _ = window.emit("trace-progress", ProgressEvent {
                stage: "reading".to_string(),
                progress: 0.0,
                current: 0,
                total: estimated_lines as u64,
                message: format!("파일 읽기 중... (예상 라인 수: {:.0})", estimated_lines),
                eta_seconds: 0.0,
                processing_speed: 0.0,
            });
            
            // 전체 파일 읽기
            content = std::fs::read_to_string(&fname).map_err(|e| e.to_string())?;
        } else {
            // 1GB 미만은 메모리 맵 사용
            let file = File::open(&fname).map_err(|e| e.to_string())?;
            let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };
            
            // 파일 내용 UTF-8로 변환
            content = match std::str::from_utf8(&mmap) {
                Ok(c) => c.to_string(),
                Err(e) => return Err(format!("File is not valid UTF-8: {}", e)),
            };
        }

        // 청크 크기 최적화: 파일 크기에 따라 조정
        let chunk_size = if file_size > 10_000_000_000 {  // 10GB 이상
            250_000  // 더 큰 청크
        } else if file_size > 1_000_000_000 {  // 1GB 이상
            150_000  // 중간 크기 청크
        } else {
            100_000  // 기본 청크 크기
        };
        
        println!("Chunk Size: {} 라인씩 처리", chunk_size);

        let mut ufs_list = Vec::new();
        let mut block_list = Vec::new();
        let mut missing_lines = Vec::new();

        // 라인별 병렬 처리
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        println!("All Line Count: {}", total_lines);

        // 현재 활성화된 패턴 가져오기
        let active_ufs_pattern = match ACTIVE_UFS_PATTERN.read() {
            Ok(pattern) => pattern,
            Err(e) => return Err(format!("UFS 패턴 로드 실패: {}", e)),
        };

        let active_block_pattern = match ACTIVE_BLOCK_PATTERN.read() {
            Ok(pattern) => pattern,
            Err(e) => return Err(format!("Block 패턴 로드 실패: {}", e)),
        };

        // 진행 상황 표시용 변수
        let mut last_progress = 0;
        let start_time = std::time::Instant::now();
        
        // 진행 상태 업데이트: 파싱 시작
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "parsing".to_string(),
            progress: 0.0,
            current: 0,
            total: total_lines as u64,
            message: "로그 파싱 시작".to_string(),
            eta_seconds: 0.0,
            processing_speed: 0.0,
        });

        // 청크 단위 처리 (메모리 효율성)
        for (chunk_index, chunk_start) in (0..total_lines).step_by(chunk_size).enumerate() {
            // 작업 취소 신호 확인
            {
                let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
                if *cancel {
                    return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
                }
            }
            
            // 진행 상황 업데이트 (5% 단위로)
            let current_progress = (chunk_start * 100) / total_lines;
            if current_progress >= last_progress + 5 {
                let elapsed = start_time.elapsed();
                let elapsed_secs = elapsed.as_secs_f64();
                let lines_per_sec = chunk_start as f64 / elapsed_secs;
                let remaining_lines = total_lines - chunk_start;
                let remaining_secs = remaining_lines as f64 / lines_per_sec;
                
                println!(
                    "진행 상황: {}% (처리 속도: {:.0} lines/s, 남은 시간: {:.1}분)",
                    current_progress,
                    lines_per_sec,
                    remaining_secs / 60.0
                );
                
                // 프론트엔드에 진행 상태 전송
                let _ = window.emit("trace-progress", ProgressEvent {
                    stage: "parsing".to_string(),
                    progress: current_progress as f32,
                    current: chunk_start as u64,
                    total: total_lines as u64,
                    message: format!("로그 파싱 중... ({}%)", current_progress),
                    eta_seconds: remaining_secs as f32,
                    processing_speed: lines_per_sec as f32,
                });
                
                last_progress = current_progress;
            }

            // 청크 수집
            let chunk_end = std::cmp::min(chunk_start + chunk_size, total_lines);
            let chunk_slice = &lines[chunk_start..chunk_end];

            // 청크 병렬 처리
            let chunk_results: (Vec<UFS>, Vec<Block>, Vec<usize>) = chunk_slice
                .par_iter()
                .enumerate()
                .map(|(i, &line)| {
                    let line_number = chunk_start + i + 1; // 실제 라인 번호 계산
                    if line.trim().is_empty() {
                        return (Vec::new(), Vec::new(), vec![line_number]);
                    }

                    // UFS 패턴으로 파싱 시도
                    let ufs_caps = active_ufs_pattern.1.captures(line);
                    if let Some(caps) = ufs_caps {
                        if let Ok(ufs) = parse_ufs_trace_with_caps(&caps) {
                            return (vec![ufs], Vec::new(), Vec::new());
                        }
                    }

                    // Block 패턴으로 파싱 시도
                    let block_caps = active_block_pattern.1.captures(line);
                    if let Some(caps) = block_caps {
                        if let Ok(block) = parse_block_trace_with_caps(&caps) {
                            return (Vec::new(), Vec::new(), vec![line_number]);
                        }
                    }

                    // 어떤 패턴과도 일치하지 않음
                    (Vec::new(), Vec::new(), vec![line_number])
                })
                .reduce(
                    || {
                        (
                            Vec::with_capacity(chunk_size / 4),  // 메모리 사용 최적화
                            Vec::with_capacity(chunk_size / 4),
                            Vec::new(),
                        )
                    },
                    |(mut acc_ufs, mut acc_block, mut acc_missing),
                     (ufs_vec, block_vec, missing_vec)| {
                        acc_ufs.extend(ufs_vec);
                        acc_block.extend(block_vec);
                        acc_missing.extend(missing_vec);
                        (acc_ufs, acc_block, acc_missing)
                    },
                );

            // 결과를 메인 벡터에 추가
            ufs_list.extend(chunk_results.0);
            block_list.extend(chunk_results.1);
            
            // missing_lines가 너무 많으면 처음 1000개만 저장 (메모리 절약)
            if missing_lines.len() < 1000 {
                missing_lines.extend(chunk_results.2);
            } else if missing_lines.len() == 1000 && !chunk_results.2.is_empty() {
                missing_lines.push(0); // 표시용 센티널 값
            }
            
            // 메모리 사용량 정보 (10청크 단위로만 표시)
            if chunk_index % 10 == 0 {
                let ufs_mem = (std::mem::size_of::<UFS>() * ufs_list.capacity()) as f64 / 1_048_576.0;
                let block_mem = (std::mem::size_of::<Block>() * block_list.capacity()) as f64 / 1_048_576.0;
                println!("메모리 사용량 - UFS: {:.1} MB, Block: {:.1} MB", ufs_mem, block_mem);
            }
        }

        println!("파싱 완료: UFS 이벤트 {}, Block 이벤트 {}, 미인식 라인 {}",
                 ufs_list.len(), block_list.len(), 
                 if missing_lines.len() > 1000 { 
                     format!("1000+") 
                 } else { 
                     format!("{}", missing_lines.len())
                 });
        
        // 진행 상태 업데이트: latency 계산 시작
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "latency".to_string(),
            progress: 0.0,
            current: 0,
            total: 100,
            message: "latency 메트릭 계산 중...".to_string(),
            eta_seconds: 0.0,
            processing_speed: 0.0,
        });
        
        println!("latency 메트릭 계산 중...");
        
        // 메모리 최적화를 위한 용량 조정
        ufs_list.shrink_to_fit();
        block_list.shrink_to_fit();

        // Bottom half: latency 계산 처리
        println!("UFS latency 처리 시작...");
        
        // 작업 취소 확인
        {
            let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
            if *cancel {
                return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
            }
        }
        
        // UFS latency 처리
        let ufs_start = std::time::Instant::now();
        let processed_ufs_list = ufs_bottom_half_latency_process(ufs_list);
        let ufs_elapsed = ufs_start.elapsed().as_secs_f32();
        
        // 진행 상태 업데이트: UFS 처리 완료
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "latency".to_string(),
            progress: 40.0,
            current: 40,
            total: 100,
            message: format!("UFS latency 처리 완료 (소요시간: {:.1}초)", ufs_elapsed),
            eta_seconds: ufs_elapsed * 1.5, // Block 처리 예상 시간: UFS의 1.5배
            processing_speed: if ufs_elapsed > 0.0 { processed_ufs_list.len() as f32 / ufs_elapsed } else { 0.0 },
        });
        
        // 작업 취소 확인
        {
            let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
            if *cancel {
                return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
            }
        }
        
        // Block latency 처리
        println!("Block latency 처리 시작...");
        let block_start = std::time::Instant::now();
        let processed_block_list = block_bottom_half_latency_process(block_list);
        let block_elapsed = block_start.elapsed().as_secs_f32();
        
        // 진행 상태 업데이트: Block 처리 완료
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "latency".to_string(),
            progress: 80.0,
            current: 80,
            total: 100,
            message: format!("Block latency 처리 완료 (소요시간: {:.1}초)", block_elapsed),
            eta_seconds: 10.0, // 파일 저장에 약 10초 소요 예상
            processing_speed: if block_elapsed > 0.0 { processed_block_list.len() as f32 / block_elapsed } else { 0.0 },
        });

        // 공통 timestamp 생성
        let now = Local::now();
        let timestamp = now.format("%Y%m%d_%H%M%S").to_string();

        // 진행 상태 업데이트: 파일 저장 시작
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "saving".to_string(),
            progress: 80.0,
            current: 80,
            total: 100,
            message: "Parquet 파일 저장 시작...".to_string(),
            eta_seconds: 10.0,
            processing_speed: 0.0,
        });
        
        println!("Parquet 파일 저장 시작...");
        
        // 작업 취소 확인
        {
            let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
            if *cancel {
                return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
            }
        }
        
        // 파싱된 UFS 로그를 parquet 파일로 저장
        let ufs_parquet_filename = if !processed_ufs_list.is_empty() {
            println!("UFS Parquet 저장 중 ({} 이벤트)...", processed_ufs_list.len());
            
            // 진행 상태 업데이트: UFS 파일 저장 중
            let _ = window.emit("trace-progress", ProgressEvent {
                stage: "saving".to_string(),
                progress: 85.0,
                current: 85,
                total: 100,
                message: format!("UFS Parquet 저장 중 ({} 이벤트)...", processed_ufs_list.len()),
                eta_seconds: 5.0,
                processing_speed: 0.0,
            });
            
            save_ufs_to_parquet(
                &processed_ufs_list,
                logfolder.clone(),
                fname.clone(),
                &timestamp,
            )?
        } else {
            String::new()
        };

        // 작업 취소 확인
        {
            let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
            if *cancel {
                return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
            }
        }
        
        // Block trace 로그를 parquet 파일로 저장
        let block_parquet_filename = if !processed_block_list.is_empty() {
            println!("Block Parquet 저장 중 ({} 이벤트)...", processed_block_list.len());
            
            // 진행 상태 업데이트: Block 파일 저장 중
            let _ = window.emit("trace-progress", ProgressEvent {
                stage: "saving".to_string(),
                progress: 95.0,
                current: 95,
                total: 100,
                message: format!("Block Parquet 저장 중 ({} 이벤트)...", processed_block_list.len()),
                eta_seconds: 2.0,
                processing_speed: 0.0,
            });
            
            save_block_to_parquet(
                &processed_block_list,
                logfolder.clone(),
                fname.clone(),
                &timestamp,
            )?
        } else {
            String::new()
        };
        
        println!("처리 완료!");
        let total_elapsed = start_time.elapsed().as_secs_f64();
        println!("총 처리 시간: {:.1}초 ({:.1}분)", total_elapsed, total_elapsed / 60.0);
        
        // 완료 이벤트
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "complete".to_string(),
            progress: 100.0,
            current: 100,
            total: 100,
            message: format!("처리 완료! (총 소요시간: {:.1}초)", total_elapsed),
            eta_seconds: 0.0,
            processing_speed: (total_lines as f32 / total_elapsed as f32),
        });

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

// Captures가 이미 있는 경우 UFS 파싱 (중복 코드 방지)
pub fn parse_ufs_trace_with_caps(caps: &regex::Captures) -> Result<UFS, String> {
    // Named captures 사용
    let time = caps
        .name("time")
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .ok_or("time parse error")?;
    let process = caps
        .name("process")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let cpu = caps
        .name("cpu")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("cpu parse error")?;
    let action = caps
        .name("command")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let tag = caps
        .name("tag")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("tag parse error")?;
    let size_str = caps.name("size").ok_or("size field missing")?.as_str();
    let size: i32 = size_str.parse::<i32>().map_err(|e| e.to_string())?;
    // byte를 4KB 단위로 변환 (4096 bytes = 4KB)
    let size: u32 = (size.abs() as u32) / 4096;

    // LBA 처리 - 터무니 없는 값(최대값) 체크
    let lba_str = caps.name("lba").map(|m| m.as_str()).unwrap_or("0");
    let lba = if lba_str == "18446744073709551615" || lba_str == "4294967295" {
        0 // 최대값은 0으로 처리
    } else {
        lba_str.parse().unwrap_or(0)
    };

    let opcode = caps
        .name("opcode")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let groupid = caps
        .name("group_id")
        .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
        .ok_or("group_id parse error")?;
    let hwqid = caps
        .name("hwq_id")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("hwq_id parse error")?;

    Ok(UFS {
        time,
        process,
        cpu,
        action,
        tag,
        opcode,
        lba,
        size,
        groupid,
        hwqid,
        qd: 0,
        dtoc: 0.0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

// Captures가 이미 있는 경우 Block 파싱 (중복 코드 방지)
pub fn parse_block_trace_with_caps(caps: &regex::Captures) -> Result<Block, String> {
    // Named captures 사용
    let time = caps
        .name("time")
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .ok_or("time parse error")?;
    let process = caps
        .name("process")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let cpu = caps
        .name("cpu")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("cpu parse error")?;
    let flags = caps
        .name("flags")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let action = caps
        .name("action")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let devmajor = caps
        .name("devmajor")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("devmajor error")?;
    let devminor = caps
        .name("devminor")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("devminor error")?;
    let io_type = caps
        .name("io_type")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let extra = caps
        .name("extra")
        .map_or(0, |m| m.as_str().parse().unwrap_or(0));

    // For sector, we need to handle the special case of max value
    let sector_str = caps.name("sector").map(|m| m.as_str()).unwrap_or("0");
    let sector = if sector_str == "18446744073709551615" {
        0 // 최대값은 0으로 처리
    } else {
        sector_str.parse().unwrap_or(0)
    };

    let size = caps
        .name("size")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or("size error")?;
    let comm = caps
        .name("comm")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

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

pub async fn filter_trace(
    logname: String,
    tracetype: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    max_records: usize,
) -> Result<String, String> {
    // 필터링 및 샘플링 결과를 저장할 변수
    let result = match tracetype.as_str() {
        "ufs" => {
            // UFS 데이터 필터링
            let ufs_vec =
                filter_ufs_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

            // UFS 데이터 샘플링
            let ufs_sample_info = sample_ufs(&ufs_vec, max_records);

            // 샘플링 정보를 JSON으로 직렬화하여 반환
            serde_json::json!({
                "data": ufs_sample_info.data,
                "total_count": ufs_sample_info.total_count,
                "sampled_count": ufs_sample_info.sampled_count,
                "sampling_ratio": ufs_sample_info.sampling_ratio,
                "type": "ufs"
            })
            .to_string()
        }
        "block" => {
            // Block 데이터 필터링
            let block_vec =
                filter_block_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

            // Block 데이터 샘플링
            let block_sample_info = sample_block(&block_vec, max_records);

            // 샘플링 정보를 JSON으로 직렬화하여 반환
            serde_json::json!({
                "data": block_sample_info.data,
                "total_count": block_sample_info.total_count,
                "sampled_count": block_sample_info.sampled_count,
                "sampling_ratio": block_sample_info.sampled_count,
                "type": "block"
            })
            .to_string()
        }
        _ => return Err("Unsupported trace type".to_string()),
    };

    Ok(result)
}

// Parquet 파일에서 타일 기반으로 데이터를 로드하는 함수
// 타일링 기법으로 대용량 데이터를 효율적으로 처리
pub async fn load_tile_data(
    parquet_file: String,
    tile_key: String,
    x_axis_key: String,
    y_axis_key: String,
    legend_key: String,
    x_range: Vec<f64>,
    y_range: Vec<f64>,
    zoom: u32,
) -> Result<String, String> {
    println!("타일 데이터 요청: {}, 줌 레벨: {}", tile_key, zoom);
    
    // 타일 좌표 파싱
    let parts: Vec<&str> = tile_key.split('/').collect();
    if parts.len() != 3 {
        return Err("잘못된 타일 키 형식".to_string());
    }
    
    let tile_x = parts[1].parse::<u32>().map_err(|e| e.to_string())?;
    let tile_y = parts[2].parse::<u32>().map_err(|e| e.to_string())?;
    
    // 타일의 실제 범위 계산 (WebMercator 좌표계 기준)
    let max_tile = 2u32.pow(zoom);
    let tile_size = 1.0 / max_tile as f64; // 정규화된 타일 크기
    
    let tile_min_x = tile_x as f64 * tile_size;
    let tile_max_x = (tile_x + 1) as f64 * tile_size;
    let tile_min_y = tile_y as f64 * tile_size;
    let tile_max_y = (tile_y + 1) as f64 * tile_size;
    
    // x_range, y_range로 전달된 실제 데이터 범위
    let data_min_x = x_range[0];
    let data_max_x = x_range[1];
    let data_min_y = y_range[0];
    let data_max_y = y_range[1];
    
    // 타일 범위를 데이터 범위로 변환
    let min_x = data_min_x + (data_max_x - data_min_x) * tile_min_x;
    let max_x = data_min_x + (data_max_x - data_min_x) * tile_max_x;
    let min_y = data_min_y + (data_max_y - data_min_y) * tile_min_y;
    let max_y = data_min_y + (data_max_y - data_min_y) * tile_max_y;
    
    println!("타일 범위: X({} ~ {}), Y({} ~ {})", min_x, max_x, min_y, max_y);
    
    // 타일 크기에 따른 추출 데이터 포인트 수 계산
    // 줌 레벨이 높을수록 타일 하나에 적은 데이터를 로드
    let max_points_per_tile = if zoom < 10 {
        10000 // 낮은 줌 레벨에서는 더 많은 포인트
    } else if zoom < 15 {
        5000
    } else {
        2000 // 높은 줌 레벨에서는 더 적은 포인트
    };
    
    // DataFusion 컨텍스트 초기화
    let config = SessionConfig::new()
        .with_batch_size(8192);
    let ctx = SessionContext::new_with_config(config);
    
    // Parquet 파일 읽기 준비
    let path = PathBuf::from(&parquet_file);
    if !path.is_file() {
        return Err(format!("파일을 찾을 수 없음: {}", parquet_file));
    }
    
    // 파일 유형 확인 (UFS 또는 Block)
    let file_type = if path.to_string_lossy().contains("ufs") {
        "ufs"
    } else if path.to_string_lossy().contains("block") {
        "block"
    } else {
        return Err("지원되지 않는 파일 형식".to_string());
    };
    
    // 예상 로드 시간 로깅
    let load_start = std::time::Instant::now();
    
    // 쿼리 생성 - 해당 타일 영역의 데이터만 필터링
    let read_options = ParquetReadOptions::default();
    let df = ctx
        .read_parquet(
            path.to_str().ok_or("Invalid path")?,
            read_options,
        ).await.map_err(|e| e.to_string())?;
    
    // 필터 조건 적용
    let filtered_df = df
        .filter(
            col(&x_axis_key).gt_eq(lit(min_x))
                .and(col(&x_axis_key).lt(lit(max_x)))
                .and(col(&y_axis_key).gt_eq(lit(min_y)))
                .and(col(&y_axis_key).lt(lit(max_y)))
        ).map_err(|e| e.to_string())?;
    
    // 결과 개수 확인
    let count_df = filtered_df.clone().aggregate(
        vec![],
        vec![count(lit(1)).alias("count")],
    ).map_err(|e| e.to_string())?;
    
    // 집계된 카운트 가져오기
    let batches = count_df.collect().await.map_err(|e| e.to_string())?;
    
    let total_count = if !batches.is_empty() && batches[0].num_rows() > 0 {
        let array = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .ok_or("다운캐스트 실패")?;
        array.value(0) as usize
    } else {
        0
    };
    
    println!("타일 내 총 데이터 포인트: {}", total_count);
    
    // 만약 데이터가 너무 많다면, 다운샘플링 수행
    let final_df = if total_count > max_points_per_tile {
        // 샘플링 비율 계산
        let sampling_ratio = max_points_per_tile as f64 / total_count as f64;
        println!("다운샘플링 적용: {:.2}% ({}/{})", sampling_ratio * 100.0, max_points_per_tile, total_count);
        
        // DataFrame에서 sample 메서드 대신 TABLESAMPLE 쿼리 사용
        let sql = format!(
            "SELECT {} FROM filtered_table TABLESAMPLE ({:.6}%)",
            format!("{}, {}, {}", x_axis_key, y_axis_key, legend_key),
            sampling_ratio * 100.0
        );
        
        // 임시 테이블로 등록
        ctx.register_table("filtered_table", filtered_df.into_view())
            .map_err(|e| e.to_string())?;
        
        // SQL 실행
        ctx.sql(&sql).await.map_err(|e| e.to_string())?
    } else {
        // 필요한 컬럼만 선택
        filtered_df
            .select(vec![
                col(&x_axis_key),
                col(&y_axis_key),
                col(&legend_key),
                // 추가 필드는 필요에 따라 확장
            ])
            .map_err(|e| e.to_string())?
    };
    
    // 결과 실행 및 배치 가져오기
    let batches = final_df.collect().await.map_err(|e| e.to_string())?;
    let loaded_count = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();
    println!("로드된 데이터 포인트: {}", loaded_count);
    // 배치를 JSON으로 변환
    let mut result = Vec::with_capacity(loaded_count);
    
    for batch in batches {
        let x_idx = batch.schema().index_of(&x_axis_key).map_err(|e| e.to_string())?;
        let y_idx = batch.schema().index_of(&y_axis_key).map_err(|e| e.to_string())?;
        let legend_idx = batch.schema().index_of(&legend_key).map_err(|e| e.to_string())?;
        
        // 각 컬럼의 배열 타입에 따라 다운캐스팅
        let x_array = batch.column(x_idx);
        let y_array = batch.column(y_idx);
        let legend_array = batch.column(legend_idx);
        
        for row in 0..batch.num_rows() {
            let mut point = serde_json::Map::new();
            
            // X 값 추출 (타입에 따라 다운캐스팅)
            match x_array.data_type() {
                DataType::Float64 => {
                    let arr = x_array.as_any().downcast_ref::<Float64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(x_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::Float32 => {
                    let arr = x_array.as_any().downcast_ref::<Float32Array>().ok_or("다운캐스트 실패")?;
                    point.insert(x_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::Int64 => {
                    let arr = x_array.as_any().downcast_ref::<Int64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(x_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::UInt64 => {
                    let arr = x_array.as_any().downcast_ref::<UInt64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(x_axis_key.clone(), json!(arr.value(row)));
                },
                _ => return Err(format!("지원되지 않는 X 데이터 타입: {:?}", x_array.data_type())),
            }
            
            // Y 값 추출
            match y_array.data_type() {
                DataType::Float64 => {
                    let arr = y_array.as_any().downcast_ref::<Float64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(y_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::Float32 => {
                    let arr = y_array.as_any().downcast_ref::<Float32Array>().ok_or("다운캐스트 실패")?;
                    point.insert(y_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::Int64 => {
                    let arr = y_array.as_any().downcast_ref::<Int64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(y_axis_key.clone(), json!(arr.value(row)));
                },
                DataType::UInt64 => {
                    let arr = y_array.as_any().downcast_ref::<UInt64Array>().ok_or("다운캐스트 실패")?;
                    point.insert(y_axis_key.clone(), json!(arr.value(row)));
                },
                _ => return Err(format!("지원되지 않는 Y 데이터 타입: {:?}", y_array.data_type())),
            }
            
            // 범례 값 추출
            match legend_array.data_type() {
                DataType::Utf8 => {
                    let arr = legend_array.as_any().downcast_ref::<StringViewArray>().ok_or("다운캐스트 실패")?;
                    point.insert(legend_key.clone(), json!(arr.value(row).to_string()));
                },
                DataType::UInt32 => {
                    let arr = legend_array.as_any().downcast_ref::<UInt32Array>().ok_or("다운캐스트 실패")?;
                    point.insert(legend_key.clone(), json!(arr.value(row).to_string()));
                },
                _ => return Err(format!("지원되지 않는 범례 데이터 타입: {:?}", legend_array.data_type())),
            }
            
            result.push(serde_json::Value::Object(point));
        }
    }
    
    // 로드 시간 측정 및 로깅
    let load_time = load_start.elapsed();
    println!(
        "타일 로드 완료 - {} 포인트, 소요시간: {:.2}초",
        result.len(),
        load_time.as_secs_f64()
    );
    
// JSON으로 직렬화하여 반환
serde_json::to_string(&result).map_err(|e| e.to_string())
}