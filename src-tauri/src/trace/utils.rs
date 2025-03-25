use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::PathBuf;

use chrono::Local;
use datafusion::prelude::*;
use memmap2::Mmap;
use rand::prelude::IndexedRandom;
use rayon::prelude::*;
use tauri::async_runtime::spawn_blocking;

use serde::Serialize;

use crate::trace::block::{
    block_bottom_half_latency_process, save_block_to_parquet,
};
use crate::trace::ufs::{save_ufs_to_parquet, ufs_bottom_half_latency_process};
use crate::trace::{
    Block, LatencySummary, TraceParseResult, BLOCK_CACHE, UFS, UFS_CACHE,
};

use crate::trace::filter::{filter_ufs_data, filter_block_data};

use super::{ACTIVE_UFS_PATTERN, ACTIVE_BLOCK_PATTERN};

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
        let mut rng = rand::rng();
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
        let mut rng = rand::rng();
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
                    "sampling_ratio": block_sample_info.sampling_ratio
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

    // DataFusion context 생성
    let ctx = SessionContext::new();

    // 각 파일 처리: 파일명에 따라 ufs 또는 block으로 구분
    for file in files {
        let path = PathBuf::from(&file);
        if !path.is_file() {
            continue; // 파일이 아니면 건너뜁니다.
        }

        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            if fname.contains("ufs") && fname.ends_with(".parquet") {
                // UFS parquet 파일 읽기
                let df = ctx
                    .read_parquet(
                        path.to_str().ok_or("Invalid path")?,
                        ParquetReadOptions::default(),
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
                // Block parquet 파일 읽기
                let df = ctx
                    .read_parquet(
                        path.to_str().ok_or("Invalid path")?,
                        ParquetReadOptions::default(),
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
            "sampling_ratio": block_sample_info.sampling_ratio
        }
    });

    Ok(result_json.to_string())
}

// 로그 파일 파싱 및 parquet 저장 함수
pub async fn starttrace(fname: String, logfolder: String) -> Result<TraceParseResult, String> {
    let result = spawn_blocking(move || {
        // 파일 열기 및 메모리 매핑
        let file = File::open(&fname).map_err(|e| e.to_string())?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };

        // 청크 단위로 처리
        let chunk_size = 100_000; // 10만 라인씩 처리

        let mut ufs_list = Vec::new();
        let mut block_list = Vec::new();
        let mut missing_lines = Vec::new();

        // 파일 내용 UTF-8로 변환
        let content = match std::str::from_utf8(&mmap) {
            Ok(c) => c,
            Err(e) => return Err(format!("File is not valid UTF-8: {}", e)),
        };

        // 라인별 병렬 처리
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        
        // 현재 활성화된 패턴 가져오기
        let active_ufs_pattern = match ACTIVE_UFS_PATTERN.read() {
            Ok(pattern) => pattern,
            Err(e) => return Err(format!("UFS 패턴 로드 실패: {}", e)),
        };
        
        let active_block_pattern = match ACTIVE_BLOCK_PATTERN.read() {
            Ok(pattern) => pattern,
            Err(e) => return Err(format!("Block 패턴 로드 실패: {}", e)),
        };

        // 청크 단위 처리 (메모리 효율성)
        for chunk_start in (0..total_lines).step_by(chunk_size) {
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
                            return (Vec::new(), vec![block], Vec::new());
                        }
                    }
                    
                    // 어떤 패턴과도 일치하지 않음
                    (Vec::new(), Vec::new(), vec![line_number])
                })
                .reduce(
                    || {
                        (
                            Vec::with_capacity(chunk_size),
                            Vec::with_capacity(chunk_size),
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
            missing_lines.extend(chunk_results.2);
        }

        // Bottom half: latency 계산 처리
        let processed_ufs_list = ufs_bottom_half_latency_process(ufs_list);
        let processed_block_list = block_bottom_half_latency_process(block_list);

        // 공통 timestamp 생성
        let now = Local::now();
        let timestamp = now.format("%Y%m%d_%H%M%S").to_string();

        // 파싱된 UFS 로그를 parquet 파일로 저장
        let ufs_parquet_filename = if !processed_ufs_list.is_empty() {
            save_ufs_to_parquet(
                &processed_ufs_list,
                logfolder.clone(),
                fname.clone(),
                &timestamp,
            )?
        } else {
            String::new()
        };

        // Block trace 로그를 parquet 파일로 저장
        let block_parquet_filename = if !processed_block_list.is_empty() {
            save_block_to_parquet(
                &processed_block_list,
                logfolder.clone(),
                fname.clone(),
                &timestamp,
            )?
        } else {
            String::new()
        };

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
    let size_str = caps
        .name("size")
        .ok_or("size field missing")?
        .as_str();
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
            let ufs_vec = filter_ufs_data(
                &logname,
                time_from,
                time_to,
                &zoom_column,
                col_from,
                col_to,
            )?;
            
            // UFS 데이터 샘플링
            let ufs_sample_info = sample_ufs(&ufs_vec, max_records);
            
            // 샘플링 정보를 JSON으로 직렬화하여 반환
            serde_json::json!({
                "data": ufs_sample_info.data,
                "total_count": ufs_sample_info.total_count,
                "sampled_count": ufs_sample_info.sampled_count,
                "sampling_ratio": ufs_sample_info.sampling_ratio,
                "type": "ufs"
            }).to_string()
        },
        "block" => {
            // Block 데이터 필터링
            let block_vec = filter_block_data(
                &logname,
                time_from,
                time_to,
                &zoom_column,
                col_from,
                col_to,
            )?;
            
            // Block 데이터 샘플링
            let block_sample_info = sample_block(&block_vec, max_records);
            
            // 샘플링 정보를 JSON으로 직렬화하여 반환
            serde_json::json!({
                "data": block_sample_info.data,
                "total_count": block_sample_info.total_count,
                "sampled_count": block_sample_info.sampled_count,
                "sampling_ratio": block_sample_info.sampling_ratio,
                "type": "block"
            }).to_string()
        },
        _ => return Err("Unsupported trace type".to_string()),
    };

    Ok(result)
}