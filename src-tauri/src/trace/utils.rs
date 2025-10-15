use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::Write;

use chrono::Local;
use datafusion::prelude::*;
use memmap2::Mmap;
use rayon::prelude::*;
use tauri::async_runtime::spawn_blocking;
use tauri::Emitter;
use arrow::ipc::writer::StreamWriter;
use parquet::file::reader::{FileReader, SerializedFileReader};

use serde::Serialize;

use crate::trace::block::{block_bottom_half_latency_process, save_block_to_parquet};
use crate::trace::ufs::{save_ufs_to_parquet, ufs_bottom_half_latency_process};
use crate::trace::ufscustom::{save_ufscustom_to_parquet, ufscustom_bottom_half_latency_process, ufscustom_to_record_batch};
use crate::trace::{Block, LatencySummary, TraceParseResult, BLOCK_CACHE, UFS, UFS_CACHE, UFSCUSTOM, UFSCUSTOM_CACHE, ProgressEvent, CANCEL_SIGNAL};

use crate::trace::filter::{filter_block_data, filter_ufs_data, filter_ufscustom_data};
use crate::trace::block::block_to_record_batch;
use crate::trace::ufs::ufs_to_record_batch;
use crate::trace::constants::{UFS_DEBUG_LBA, MAX_VALID_UFS_LBA};

use super::{ACTIVE_BLOCK_PATTERN, ACTIVE_UFS_PATTERN, ACTIVE_UFSCUSTOM_PATTERN};

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
        // 랜덤 샘플링 수행
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        
        let mut rng = rand::rngs::StdRng::seed_from_u64(12345); // 고정 시드로 재현 가능한 결과
        let mut indices: Vec<usize> = (0..total_count).collect();
        indices.shuffle(&mut rng);
        indices.truncate(max_records);
        indices.sort(); // 시간 순서 유지를 위해 정렬
        
        let mut sampled_data = Vec::with_capacity(max_records);
        for &index in &indices {
            sampled_data.push(ufs_list[index].clone());
        }
        
        let sampled_count = sampled_data.len();
        let sampling_ratio = (sampled_count as f64 / total_count as f64) * 100.0;
        
        SamplingInfo {
            data: sampled_data,
            total_count,
            sampled_count,
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
        // 랜덤 샘플링으로 임시 변경 (테스트용)
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        
        println!("🔍 [RANDOM sampling] Block 랜덤 샘플링: {}/{} 레코드", max_records, total_count);
        
        let mut rng = rand::rngs::StdRng::seed_from_u64(12345); // 고정 시드로 재현 가능한 결과
        let mut indices: Vec<usize> = (0..total_count).collect();
        indices.shuffle(&mut rng);
        indices.truncate(max_records);
        indices.sort(); // 시간 순서 유지를 위해 정렬
        
        let mut sampled_data = Vec::with_capacity(max_records);
        for &index in &indices {
            sampled_data.push(block_list[index].clone());
        }
        
        let sampled_count = sampled_data.len();
        let sampling_ratio = (sampled_count as f64 / total_count as f64) * 100.0;
        
        println!("  Random sampled: {} records, ratio: {:.2}%", sampled_count, sampling_ratio);
        
        SamplingInfo {
            data: sampled_data,
            total_count,
            sampled_count,
            sampling_ratio,
        }
    }
}

pub fn sample_ufscustom(ufscustom_list: &[UFSCUSTOM], max_records: usize) -> SamplingInfo<UFSCUSTOM> {
    let total_count = ufscustom_list.len();

    if total_count <= max_records {
        // 샘플링이 필요 없는 경우
        SamplingInfo {
            data: ufscustom_list.to_vec(),
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    } else {
        // 랜덤 샘플링
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        
        println!("🔍 [RANDOM sampling] UFSCUSTOM 랜덤 샘플링: {}/{} 레코드", max_records, total_count);
        
        let mut rng = rand::rngs::StdRng::seed_from_u64(12345); // 고정 시드로 재현 가능한 결과
        let mut indices: Vec<usize> = (0..total_count).collect();
        indices.shuffle(&mut rng);
        indices.truncate(max_records);
        indices.sort(); // 시간 순서 유지를 위해 정렬
        
        let mut sampled_data = Vec::with_capacity(max_records);
        for &index in &indices {
            sampled_data.push(ufscustom_list[index].clone());
        }
        
        let sampled_count = sampled_data.len();
        let sampling_ratio = (sampled_count as f64 / total_count as f64) * 100.0;
        
        println!("  Random sampled: {} records, ratio: {:.2}%", sampled_count, sampling_ratio);
        
        SamplingInfo {
            data: sampled_data,
            total_count,
            sampled_count,
            sampling_ratio,
        }
    }
}

// Arrow IPC 바이트와 샘플링 메타데이터를 함께 보낼 구조체들
#[derive(Serialize, Debug, Clone)]
pub struct ArrowBytes {
    #[serde(with = "serde_bytes")]  // ⚡ Base64 인코딩 건너뛰기 - 바이너리 직접 전송으로 40% 성능 개선
    pub bytes: Vec<u8>,
    pub total_count: usize,
    pub sampled_count: usize,
    pub sampling_ratio: f64,
}

#[derive(Serialize, Debug, Clone)]
pub struct TraceDataBytes {
    pub ufs: ArrowBytes,
    pub block: ArrowBytes,
    pub ufscustom: ArrowBytes,
}

#[derive(Serialize, Debug, Clone)]
pub struct TraceLengths {
    pub ufs: usize,
    pub block: usize,
    pub ufscustom: usize,
}

// 파일 기반 전송을 위한 구조체
#[derive(Serialize, Debug, Clone)]
pub struct TraceFilePaths {
    pub ufs_path: String,
    pub block_path: String,
    pub ufscustom_path: String,
    pub ufs_total_count: usize,
    pub ufs_sampled_count: usize,
    pub ufs_sampling_ratio: f64,
    pub block_total_count: usize,
    pub block_sampled_count: usize,
    pub block_sampling_ratio: f64,
    pub ufscustom_total_count: usize,
    pub ufscustom_sampled_count: usize,
    pub ufscustom_sampling_ratio: f64,
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
pub fn calculate_statistics(values: &mut [f64]) -> LatencySummary {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

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
        if c.is_ascii_digit() || c == '.' {
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

// RecordBatch를 Arrow IPC 바이트로 변환하는 헬퍼
fn batch_to_ipc_bytes(batch: &arrow::record_batch::RecordBatch) -> Result<Vec<u8>, String> {
    let ipc_start = std::time::Instant::now();
    
    let mut buf = Vec::new();
    let mut writer = StreamWriter::try_new(&mut buf, batch.schema().as_ref()).map_err(|e| e.to_string())?;
    writer.write(batch).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    
    let ipc_time = ipc_start.elapsed();
    println!("📊 [Performance] IPC 변환: {}KB, {}ms", 
             buf.len() / 1024,
             ipc_time.as_millis());
    
    Ok(buf)
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
pub async fn readtrace(logname: String, max_records: usize) -> Result<TraceDataBytes, String> {
    let starttime = std::time::Instant::now();
    
    println!("🔍 readtrace 호출: logname='{}', max_records={}", logname, max_records);
    
    // 캐시 키 생성 (원본 파일명 사용)
    let cache_key = format!("{}", logname);
    println!("🔑 캐시 키: '{}'", cache_key);
    
    // 캐시 확인: 원본 데이터가 있는지 확인
    {
        let ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        let block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        let ufscustom_cache = UFSCUSTOM_CACHE.lock().map_err(|e| e.to_string())?;

        if ufs_cache.contains_key(&cache_key) || block_cache.contains_key(&cache_key) || ufscustom_cache.contains_key(&cache_key) {
            let empty_ufs_vec: Vec<UFS> = Vec::new();
            let empty_block_vec: Vec<Block> = Vec::new();
            let empty_ufscustom_vec: Vec<UFSCUSTOM> = Vec::new();

            let ufs_data = ufs_cache.get(&cache_key).unwrap_or(&empty_ufs_vec);
            let block_data = block_cache.get(&cache_key).unwrap_or(&empty_block_vec);
            let ufscustom_data = ufscustom_cache.get(&cache_key).unwrap_or(&empty_ufscustom_vec);

            println!("🎯 [DEBUG] 캐시된 원본 데이터 사용: UFS={}, Block={}, UFSCUSTOM={}", 
                ufs_data.len(), block_data.len(), ufscustom_data.len());
            
            // 캐시된 원본 데이터를 샘플링해서 반환
            let ufs_sample_info = sample_ufs(&ufs_data, max_records);
            let block_sample_info = sample_block(&block_data, max_records);
            let ufscustom_sample_info = sample_ufscustom(&ufscustom_data, max_records);
            
            let ufs_batch = crate::trace::ufs::ufs_to_record_batch(&ufs_sample_info.data)?;
            let block_batch = crate::trace::block::block_to_record_batch(&block_sample_info.data)?;
            let ufscustom_batch = crate::trace::ufscustom::ufscustom_to_record_batch(&ufscustom_sample_info.data)?;

            let ufs_bytes = batch_to_ipc_bytes(&ufs_batch)?;
            let block_bytes = batch_to_ipc_bytes(&block_batch)?;
            let ufscustom_bytes = batch_to_ipc_bytes(&ufscustom_batch)?;

            return Ok(TraceDataBytes {
                ufs: ArrowBytes {
                    bytes: ufs_bytes,
                    total_count: ufs_sample_info.total_count,
                    sampled_count: ufs_sample_info.sampled_count,
                    sampling_ratio: ufs_sample_info.sampling_ratio,
                },
                block: ArrowBytes {
                    bytes: block_bytes,
                    total_count: block_sample_info.total_count,
                    sampled_count: block_sample_info.sampled_count,
                    sampling_ratio: block_sample_info.sampling_ratio,
                },
                ufscustom: ArrowBytes {
                    bytes: ufscustom_bytes,
                    total_count: ufscustom_sample_info.total_count,
                    sampled_count: ufscustom_sample_info.sampled_count,
                    sampling_ratio: ufscustom_sample_info.sampling_ratio,
                },
            });
        }
    }

    let mut ufs_vec: Vec<UFS> = Vec::new();
    let mut block_vec: Vec<Block> = Vec::new();
    let mut ufscustom_vec: Vec<UFSCUSTOM> = Vec::new();

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
        println!("📁 파일 확인: '{}'", file);
        
        if !path.is_file() {
            println!("⚠️  파일이 존재하지 않음: '{}'", file);
            continue; // 파일이 아니면 건너뜁니다.
        }
        
        println!("✅ 파일 존재 확인: '{}'", file);

        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            println!("🔍 파일명 분석: '{}'", fname);
            
            // ⚠️ 중요: ufscustom을 먼저 체크해야 함 (ufs 체크가 먼저 오면 ufscustom도 매칭됨)
            if fname.contains("ufscustom") && fname.ends_with(".parquet") {
                println!("📊 UFSCUSTOM parquet 파일 처리 시작: '{}'", file);
                // UFSCUSTOM parquet 파일 읽기
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

                    // 컬럼 인덱스 추출
                    let opcode_idx = schema.index_of("opcode").map_err(|e| e.to_string())?;
                    let lba_idx = schema.index_of("lba").map_err(|e| e.to_string())?;
                    let size_idx = schema.index_of("size").map_err(|e| e.to_string())?;
                    let start_time_idx = schema.index_of("start_time").map_err(|e| e.to_string())?;
                    let end_time_idx = schema.index_of("end_time").map_err(|e| e.to_string())?;
                    let dtoc_idx = schema.index_of("dtoc").map_err(|e| e.to_string())?;
                    let start_qd_idx = schema.index_of("start_qd").map_err(|e| e.to_string())?;
                    let end_qd_idx = schema.index_of("end_qd").map_err(|e| e.to_string())?;
                    let ctoc_idx = schema.index_of("ctoc").map_err(|e| e.to_string())?;
                    let ctod_idx = schema.index_of("ctod").map_err(|e| e.to_string())?;
                    let cont_idx = schema.index_of("continuous").map_err(|e| e.to_string())?;

                    // 각 컬럼 배열 다운캐스팅
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
                    let start_time_array = batch
                        .column(start_time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'start_time'")?;
                    let end_time_array = batch
                        .column(end_time_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'end_time'")?;
                    let dtoc_array = batch
                        .column(dtoc_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::Float64Array>()
                        .ok_or("Failed to downcast 'dtoc'")?;
                    let start_qd_array = batch
                        .column(start_qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'start_qd'")?;
                    let end_qd_array = batch
                        .column(end_qd_idx)
                        .as_any()
                        .downcast_ref::<arrow::array::UInt32Array>()
                        .ok_or("Failed to downcast 'end_qd'")?;
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

                    // 배열에서 값을 추출하여 UFSCUSTOM 객체 생성
                    for row in 0..num_rows {
                        ufscustom_vec.push(UFSCUSTOM {
                            opcode: opcode_array.value(row).to_string(),
                            lba: lba_array.value(row),
                            size: size_array.value(row),
                            start_time: start_time_array.value(row),
                            end_time: end_time_array.value(row),
                            dtoc: dtoc_array.value(row),
                            start_qd: start_qd_array.value(row),
                            end_qd: end_qd_array.value(row),
                            ctoc: ctoc_array.value(row),
                            ctod: ctod_array.value(row),
                            continuous: cont_array.value(row),
                        });
                    }
                }
            } else if fname.contains("ufs") && fname.ends_with(".parquet") {
                println!("📊 UFS parquet 파일 처리 시작: '{}'", file);
                // UFS parquet 파일 읽기
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
            } else if fname.contains("block") && fname.ends_with(".parquet") {
                println!("📊 Block parquet 파일 처리 시작: '{}'", file);
                // Block parquet 파일 읽기
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
            }
        }
    }

    println!("📋 데이터 로딩 완료: UFS={} 개, Block={} 개, UFSCUSTOM={} 개 레코드", ufs_vec.len(), block_vec.len(), ufscustom_vec.len());

    // 원본 데이터를 캐시에 저장
    {
        let mut ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        let mut block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        let mut ufscustom_cache = UFSCUSTOM_CACHE.lock().map_err(|e| e.to_string())?;
        
        // 1. 복합 키로 저장 (기존)
        ufs_cache.insert(cache_key.clone(), ufs_vec.clone());
        block_cache.insert(cache_key.clone(), block_vec.clone());
        ufscustom_cache.insert(cache_key.clone(), ufscustom_vec.clone());
        
        // 2. 개별 파일 키로도 저장 (통계 요청 시 사용)
        if cache_key.contains(',') {
            // 복합 키인 경우: 각 파일별로 분리해서 저장
            let files: Vec<&str> = cache_key.split(',').map(|s| s.trim()).collect();
            for file in files {
                if file.contains("_ufs.parquet") && !ufs_vec.is_empty() {
                    ufs_cache.insert(file.to_string(), ufs_vec.clone());
                    println!("💾 개별 UFS 키로도 저장: '{}' -> {} 개 레코드", file, ufs_vec.len());
                }
                if file.contains("_block.parquet") && !block_vec.is_empty() {
                    block_cache.insert(file.to_string(), block_vec.clone());
                    println!("💾 개별 Block 키로도 저장: '{}' -> {} 개 레코드", file, block_vec.len());
                }
                if file.contains("_ufscustom.parquet") && !ufscustom_vec.is_empty() {
                    ufscustom_cache.insert(file.to_string(), ufscustom_vec.clone());
                    println!("💾 개별 UFSCUSTOM 키로도 저장: '{}' -> {} 개 레코드", file, ufscustom_vec.len());
                }
            }
        } else {
            // 단일 키인 경우: 파일 타입에 따라 해당 캐시에만 저장
            if cache_key.contains("_ufs.parquet") && !ufs_vec.is_empty() {
                // UFS 파일인 경우 UFS 캐시에만 저장 (이미 위에서 저장했으므로 로그만)
                println!("💾 단일 UFS 파일 캐시 저장: '{}' -> {} 개 레코드", cache_key, ufs_vec.len());
            }
            if cache_key.contains("_block.parquet") && !block_vec.is_empty() {
                // Block 파일인 경우 Block 캐시에만 저장 (이미 위에서 저장했으므로 로그만)
                println!("💾 단일 Block 파일 캐시 저장: '{}' -> {} 개 레코드", cache_key, block_vec.len());
            }
            if cache_key.contains("_ufscustom.parquet") && !ufscustom_vec.is_empty() {
                // UFSCUSTOM 파일인 경우 UFSCUSTOM 캐시에만 저장 (이미 위에서 저장했으므로 로그만)
                println!("💾 단일 UFSCUSTOM 파일 캐시 저장: '{}' -> {} 개 레코드", cache_key, ufscustom_vec.len());
            }
        }
        
        println!("💾 원본 데이터를 캐시에 저장: UFS={}, Block={}, UFSCUSTOM={}", ufs_vec.len(), block_vec.len(), ufscustom_vec.len());
    }

    // 샘플링을 수행
    let ufs_sample_info = sample_ufs(&ufs_vec, max_records);
    let block_sample_info = sample_block(&block_vec, max_records);
    let ufscustom_sample_info = sample_ufscustom(&ufscustom_vec, max_records);

    // Arrow IPC 형식으로 직렬화하여 반환
    let ufs_batch = crate::trace::ufs::ufs_to_record_batch(&ufs_sample_info.data)?;
    let block_batch = crate::trace::block::block_to_record_batch(&block_sample_info.data)?;
    let ufscustom_batch = crate::trace::ufscustom::ufscustom_to_record_batch(&ufscustom_sample_info.data)?;

    let ufs_bytes = batch_to_ipc_bytes(&ufs_batch)?;
    let block_bytes = batch_to_ipc_bytes(&block_batch)?;
    let ufscustom_bytes = batch_to_ipc_bytes(&ufscustom_batch)?;

    println!("readtrace elapsed time: {:?}", starttime.elapsed());
    Ok(TraceDataBytes {
        ufs: ArrowBytes {
            bytes: ufs_bytes,
            total_count: ufs_sample_info.total_count,
            sampled_count: ufs_sample_info.sampled_count,
            sampling_ratio: ufs_sample_info.sampling_ratio,
        },
        block: ArrowBytes {
            bytes: block_bytes,
            total_count: block_sample_info.total_count,
            sampled_count: block_sample_info.sampled_count,
            sampling_ratio: block_sample_info.sampling_ratio,
        },
        ufscustom: ArrowBytes {
            bytes: ufscustom_bytes,
            total_count: ufscustom_sample_info.total_count,
            sampled_count: ufscustom_sample_info.sampled_count,
            sampling_ratio: ufscustom_sample_info.sampling_ratio,
        },
    })
}

/// readtrace_to_files - Arrow IPC 데이터를 임시 파일로 저장하고 파일 경로 반환
/// 
/// IPC를 통한 대용량 바이너리 전송 대신 파일 시스템을 사용하여 성능 최적화
/// - 예상 성능: 53s → 15s (73% 개선)
/// - 자동 cleanup으로 멀티 인스턴스 안전
pub async fn readtrace_to_files(logname: String, max_records: usize) -> Result<TraceFilePaths, String> {
    let starttime = std::time::Instant::now();
    
    println!("📁 readtrace_to_files 호출: logname='{}', max_records={}", logname, max_records);
    
    // 먼저 기존 readtrace 함수를 호출하여 Arrow IPC 바이트 가져오기
    let trace_data = readtrace(logname.clone(), max_records).await?;

    // 로그 파일이 위치한 디렉토리 경로 추출
    let first_file = logname.split(',').next().ok_or("Invalid logname")?;
    let log_dir = PathBuf::from(first_file)
        .parent()
        .ok_or("Failed to get parent directory")?
        .to_path_buf();
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    
    // 로그 디렉토리에 임시 파일 저장
    let ufs_path = log_dir.join(format!("estrace_temp_ufs_{}.arrow", timestamp));
    let block_path = log_dir.join(format!("estrace_temp_block_{}.arrow", timestamp));
    let ufscustom_path = log_dir.join(format!("estrace_temp_ufscustom_{}.arrow", timestamp));

    // UFS 파일 저장
    let mut ufs_file = File::create(&ufs_path)
        .map_err(|e| format!("Failed to create UFS temp file: {}", e))?;
    ufs_file.write_all(&trace_data.ufs.bytes)
        .map_err(|e| format!("Failed to write UFS data: {}", e))?;
    
    // Block 파일 저장
    let mut block_file = File::create(&block_path)
        .map_err(|e| format!("Failed to create Block temp file: {}", e))?;
    block_file.write_all(&trace_data.block.bytes)
        .map_err(|e| format!("Failed to write Block data: {}", e))?;
    
    // UFSCUSTOM 파일 저장
    let mut ufscustom_file = File::create(&ufscustom_path)
        .map_err(|e| format!("Failed to create UFSCUSTOM temp file: {}", e))?;
    ufscustom_file.write_all(&trace_data.ufscustom.bytes)
        .map_err(|e| format!("Failed to write UFSCUSTOM data: {}", e))?;

    println!("readtrace_to_files elapsed time: {:?}", starttime.elapsed());
    println!("📁 임시 파일 생성: UFS={:?}, Block={:?}, UFSCUSTOM={:?}", ufs_path, block_path, ufscustom_path);
    
    Ok(TraceFilePaths {
        ufs_path: ufs_path.to_string_lossy().to_string(),
        block_path: block_path.to_string_lossy().to_string(),
        ufscustom_path: ufscustom_path.to_string_lossy().to_string(),
        ufs_total_count: trace_data.ufs.total_count,
        ufs_sampled_count: trace_data.ufs.sampled_count,
        ufs_sampling_ratio: trace_data.ufs.sampling_ratio,
        block_total_count: trace_data.block.total_count,
        block_sampled_count: trace_data.block.sampled_count,
        block_sampling_ratio: trace_data.block.sampling_ratio,
        ufscustom_total_count: trace_data.ufscustom.total_count,
        ufscustom_sampled_count: trace_data.ufscustom.sampled_count,
        ufscustom_sampling_ratio: trace_data.ufscustom.sampling_ratio,
    })
}

fn parquet_num_rows(path: &str) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
    let metadata = reader.metadata().file_metadata();
    Ok(metadata.num_rows() as usize)
}

pub async fn trace_lengths(logname: String) -> Result<TraceLengths, String> {
    let files: Vec<String> = if logname.contains(',') {
        logname.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![logname.clone()]
    };

    let mut ufs_len = 0;
    let mut block_len = 0;
    let mut ufscustom_len = 0;

    // 각 파일의 타입을 파일명으로 감지
    for file in files {
        if file.contains("_ufs.parquet") {
            ufs_len = parquet_num_rows(&file)?;
        } else if file.contains("_block.parquet") {
            block_len = parquet_num_rows(&file)?;
        } else if file.contains("_ufscustom.parquet") {
            ufscustom_len = parquet_num_rows(&file)?;
        }
    }

    Ok(TraceLengths { ufs: ufs_len, block: block_len, ufscustom: ufscustom_len })
}

// 로그 파일 파싱 및 parquet 저장 함수
pub async fn starttrace(fname: String, logfolder: String, window: tauri::Window) -> Result<TraceParseResult, String> {
    spawn_blocking(move || {
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
        let content = if file_size > 5_368_709_120 {  // 5GB 이상은 스트리밍 방식으로 처리
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
            std::fs::read_to_string(&fname).map_err(|e| e.to_string())?
        } else {
            // 1GB 미만은 메모리 맵 사용
            let file = File::open(&fname).map_err(|e| e.to_string())?;
            let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };
            
            // 파일 내용 UTF-8로 변환
            match std::str::from_utf8(&mmap) {
                Ok(c) => c.to_string(),
                Err(e) => return Err(format!("File is not valid UTF-8: {}", e)),
            }
        };

        // 청크 크기 최적화: 파일 크기에 따라 조정
        let chunk_size = if file_size > 10_000_000_000 {  // 10GB 이상
            450_000  // 더 큰 청크
        } else if file_size > 1_000_000_000 {  // 1GB 이상
            350_000  // 중간 크기 청크
        } else {
            200_000  // 기본 청크 크기
        };
        
        println!("Chunk Size: {} 라인씩 처리", chunk_size);

        let mut ufs_list: Vec<UFS> = Vec::new();
        let mut block_list: Vec<Block> = Vec::new();
        let mut ufscustom_list: Vec<UFSCUSTOM> = Vec::new();
        let mut missing_lines: Vec<usize> = Vec::new();

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

        let active_ufscustom_pattern = match ACTIVE_UFSCUSTOM_PATTERN.read() {
            Ok(pattern) => pattern,
            Err(e) => return Err(format!("UFSCUSTOM 패턴 로드 실패: {}", e)),
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
            let chunk_results: (Vec<UFS>, Vec<Block>, Vec<UFSCUSTOM>, Vec<usize>) = chunk_slice
                .par_iter()
                .enumerate()
                .map(|(i, &line)| {
                    let line_number = chunk_start + i + 1; // 실제 라인 번호 계산
                    if line.trim().is_empty() {
                        return (Vec::new(), Vec::new(), Vec::new(), vec![line_number]);
                    }

                    // UFSCUSTOM 패턴으로 먼저 파싱 시도
                    let ufscustom_caps = active_ufscustom_pattern.1.captures(line);
                    if let Some(caps) = ufscustom_caps {
                        if let Ok(ufscustom) = parse_ufscustom_trace_with_caps(&caps) {
                            return (Vec::new(), Vec::new(), vec![ufscustom], Vec::new());
                        }
                    }

                    // UFS 패턴으로 파싱 시도
                    let ufs_caps = active_ufs_pattern.1.captures(line);
                    if let Some(caps) = ufs_caps {
                        if let Ok(ufs) = parse_ufs_trace_with_caps(&caps) {
                            return (vec![ufs], Vec::new(), Vec::new(), Vec::new());
                        }
                    }

                    // Block 패턴으로 파싱 시도
                    let block_caps = active_block_pattern.1.captures(line);
                    if let Some(caps) = block_caps {
                        if let Ok(block) = parse_block_trace_with_caps(&caps) {
                            return (Vec::new(), vec![block], Vec::new(), Vec::new());
                        }
                    }

                    // 어떤 패턴과도 일치하지 않음
                    (Vec::new(), Vec::new(), Vec::new(), vec![line_number])
                })
                .reduce(
                    || {
                        (
                            Vec::with_capacity(chunk_size / 4),  // 메모리 사용 최적화
                            Vec::with_capacity(chunk_size / 4),
                            Vec::with_capacity(chunk_size / 4),
                            Vec::new(),
                        )
                    },
                    |(mut acc_ufs, mut acc_block, mut acc_ufscustom, mut acc_missing),
                     (ufs_vec, block_vec, ufscustom_vec, missing_vec)| {
                        acc_ufs.extend(ufs_vec);
                        acc_block.extend(block_vec);
                        acc_ufscustom.extend(ufscustom_vec);
                        acc_missing.extend(missing_vec);
                        (acc_ufs, acc_block, acc_ufscustom, acc_missing)
                    },
                );

            // 결과를 메인 벡터에 추가
            ufs_list.extend(chunk_results.0);
            block_list.extend(chunk_results.1);
            ufscustom_list.extend(chunk_results.2);
            
            // missing_lines가 너무 많으면 처음 1000개만 저장 (메모리 절약)
            if missing_lines.len() < 1000 {
                missing_lines.extend(chunk_results.3);
            } else if missing_lines.len() == 1000 && !chunk_results.3.is_empty() {
                missing_lines.push(0); // 표시용 센티널 값
            }
            
            // 메모리 사용량 정보 (10청크 단위로만 표시)
            if chunk_index % 10 == 0 {
                let ufs_mem = (std::mem::size_of::<UFS>() * ufs_list.capacity()) as f64 / 1_048_576.0;
                let block_mem = (std::mem::size_of::<Block>() * block_list.capacity()) as f64 / 1_048_576.0;
                let ufscustom_mem = (std::mem::size_of::<UFSCUSTOM>() * ufscustom_list.capacity()) as f64 / 1_048_576.0;
                println!("메모리 사용량 - UFS: {:.1} MB, Block: {:.1} MB, UFSCUSTOM: {:.1} MB", ufs_mem, block_mem, ufscustom_mem);
            }
        }

        println!("파싱 완료: UFS 이벤트 {}, Block 이벤트 {}, UFSCUSTOM 이벤트 {}, 미인식 라인 {}",
                 ufs_list.len(), block_list.len(), ufscustom_list.len(),
                 if missing_lines.len() > 1000 { 
                     "1000+".to_string() 
                 } else { 
                     missing_lines.len().to_string()
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
            progress: 60.0,
            current: 60,
            total: 100,
            message: format!("Block latency 처리 완료 (소요시간: {:.1}초)", block_elapsed),
            eta_seconds: 10.0,
            processing_speed: if block_elapsed > 0.0 { processed_block_list.len() as f32 / block_elapsed } else { 0.0 },
        });

        // 작업 취소 확인
        {
            let cancel = CANCEL_SIGNAL.lock().map_err(|e| e.to_string())?;
            if *cancel {
                return Err("사용자에 의해 작업이 취소되었습니다.".to_string());
            }
        }
        
        // UFSCUSTOM latency 처리
        println!("UFSCUSTOM latency 처리 시작...");
        let ufscustom_start = std::time::Instant::now();
        let processed_ufscustom_list = ufscustom_bottom_half_latency_process(ufscustom_list);
        let ufscustom_elapsed = ufscustom_start.elapsed().as_secs_f32();
        
        // 진행 상태 업데이트: UFSCUSTOM 처리 완료
        let _ = window.emit("trace-progress", ProgressEvent {
            stage: "latency".to_string(),
            progress: 80.0,
            current: 80,
            total: 100,
            message: format!("UFSCUSTOM latency 처리 완료 (소요시간: {:.1}초)", ufscustom_elapsed),
            eta_seconds: 10.0, // 파일 저장에 약 10초 소요 예상
            processing_speed: if ufscustom_elapsed > 0.0 { processed_ufscustom_list.len() as f32 / ufscustom_elapsed } else { 0.0 },
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
                Some(&window),
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
                progress: 90.0,
                current: 90,
                total: 100,
                message: format!("Block Parquet 저장 중 ({} 이벤트)...", processed_block_list.len()),
                eta_seconds: 3.0,
                processing_speed: 0.0,
            });
            
            save_block_to_parquet(
                &processed_block_list,
                logfolder.clone(),
                fname.clone(),
                &timestamp,
                Some(&window),
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
        
        // UFSCUSTOM trace 로그를 parquet 파일로 저장
        let ufscustom_parquet_filename = if !processed_ufscustom_list.is_empty() {
            println!("UFSCUSTOM Parquet 저장 중 ({} 이벤트)...", processed_ufscustom_list.len());
            
            // 진행 상태 업데이트: UFSCUSTOM 파일 저장 중
            let _ = window.emit("trace-progress", ProgressEvent {
                stage: "saving".to_string(),
                progress: 95.0,
                current: 95,
                total: 100,
                message: format!("UFSCUSTOM Parquet 저장 중 ({} 이벤트)...", processed_ufscustom_list.len()),
                eta_seconds: 2.0,
                processing_speed: 0.0,
            });
            
            let log_basename = PathBuf::from(&fname)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&fname)
                .to_string();
                
            save_ufscustom_to_parquet(
                &processed_ufscustom_list,
                &logfolder,
                &log_basename,
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
            ufscustom_parquet_filename,
        })
    })
    .await
    .map_err(|e| e.to_string())?
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
    let size: u32 = size.unsigned_abs() / 4096;

    // LBA 처리 - 터무니 없는 값(최대값) 체크
    let raw_lba: u64 = caps["lba"].parse().unwrap_or(0);
    // Debug 또는 비정상적으로 큰 LBA 값은 0으로 처리
    let lba = if raw_lba == UFS_DEBUG_LBA || raw_lba > MAX_VALID_UFS_LBA {
        0
    } else {
        raw_lba
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
// UFSCUSTOM 파싱 함수
pub fn parse_ufscustom_trace_with_caps(caps: &regex::Captures) -> Result<UFSCUSTOM, String> {
    let opcode = caps.name("opcode").map_or("", |m| m.as_str()).to_string();
    let lba: u64 = caps
        .name("lba")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let size: u32 = caps
        .name("size")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let start_time: f64 = caps
        .name("start_time")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0.0);
    let end_time: f64 = caps
        .name("end_time")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0.0);

    // dtoc 계산 (밀리초 단위)
    let dtoc = (end_time - start_time) * 1000.0;

    Ok(UFSCUSTOM {
        opcode,
        lba,
        size,
        start_time,
        end_time,
        dtoc,
        start_qd: 0,
        end_qd: 0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

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

// 필터 검색을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct FilterTraceParams {
    pub logname: String,
    pub tracetype: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub max_records: usize,
}

// 추가적인 필터링을 위한 함수
async fn filter_block_trace(
    logname: &str,
    zoom_column: &str,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    max_records: usize,
) -> Result<TraceDataBytes, String> {
    println!("🎄 [DEBUG] filter_block_trace 호출: logname='{}', max_records={}", logname, max_records);
    
    // filter_block_data를 사용하여 필터링
    let filtered_blocks = filter_block_data(logname, time_from, time_to, zoom_column, col_from, col_to)?;
    
    // total_count 미리 계산
    let total_count = filtered_blocks.len();
    println!("📈 [DEBUG] Block 데이터 필터링 완료: total_count={}", total_count);
    
    // max_records 제한 적용 (랜덤 샘플링)
    let sampling_info = if total_count > max_records {
        println!("⚙️ [DEBUG] Block 랜덤 샘플링 수행: {} -> {} 레코드", total_count, max_records);
        sample_block(&filtered_blocks, max_records)
    } else {
        println!("✅ [DEBUG] Block 샘플링 불필요: {} 레코드 그대로 사용", total_count);
        SamplingInfo {
            data: filtered_blocks,
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    };
    
    let limited_blocks = sampling_info.data;
    let sampled_count = sampling_info.sampled_count;
    let sampling_ratio = sampling_info.sampling_ratio;
    
    println!("📋 [DEBUG] Block 샘플링 결과: sampled_count={}, sampling_ratio={:.1}%", sampled_count, sampling_ratio);
    
    // Arrow RecordBatch 변환 및 IPC 포맷으로 직렬화
    let batch = block_to_record_batch(&limited_blocks)?;
    let bytes = batch_to_ipc_bytes(&batch)?;
    
    Ok(TraceDataBytes {
        ufs: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
        block: ArrowBytes {
            bytes,
            total_count,
            sampled_count,
            sampling_ratio,
        },
        ufscustom: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
    })
}

async fn filter_ufs_trace(
    logname: &str,
    zoom_column: &str,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    max_records: usize,
) -> Result<TraceDataBytes, String> {
    println!("🎄 [DEBUG] filter_ufs_trace 호출: logname='{}', max_records={}", logname, max_records);
    
    // filter_ufs_data를 사용하여 필터링
    let filtered_ufs = filter_ufs_data(logname, time_from, time_to, zoom_column, col_from, col_to)?;
    
    // total_count 미리 계산
    let total_count = filtered_ufs.len();
    println!("📈 [DEBUG] UFS 데이터 필터링 완료: total_count={}", total_count);
    
    // max_records 제한 적용 (랜덤 샘플링)
    let sampling_info = if total_count > max_records {
        println!("⚙️ [DEBUG] UFS 랜덤 샘플링 수행: {} -> {} 레코드", total_count, max_records);
        sample_ufs(&filtered_ufs, max_records)
    } else {
        println!("✅ [DEBUG] UFS 샘플링 불필요: {} 레코드 그대로 사용", total_count);
        SamplingInfo {
            data: filtered_ufs,
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    };
    
    let limited_ufs = sampling_info.data;
    let sampled_count = sampling_info.sampled_count;
    let sampling_ratio = sampling_info.sampling_ratio;
    
    println!("📋 [DEBUG] UFS 샘플링 결과: sampled_count={}, sampling_ratio={:.1}%", sampled_count, sampling_ratio);
    
    println!("📋 [DEBUG] UFS 샘플링 결과: sampled_count={}, sampling_ratio={:.1}%", sampled_count, sampling_ratio);
    
    // Arrow RecordBatch 변환 및 IPC 포맷으로 직렬화
    let batch = ufs_to_record_batch(&limited_ufs)?;
    let bytes = batch_to_ipc_bytes(&batch)?;
    
    Ok(TraceDataBytes {
        ufs: ArrowBytes {
            bytes,
            total_count,
            sampled_count,
            sampling_ratio,
        },
        block: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
        ufscustom: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
    })
}

async fn filter_ufscustom_trace(
    logname: &str,
    zoom_column: &str,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    max_records: usize,
) -> Result<TraceDataBytes, String> {
    println!("🎄 [DEBUG] filter_ufscustom_trace 호출: logname='{}', max_records={}", logname, max_records);
    
    // filter_ufscustom_data를 사용하여 필터링
    let filtered_ufscustom = filter_ufscustom_data(logname, time_from, time_to, zoom_column, col_from, col_to)?;
    
    let total_count = filtered_ufscustom.len();
    println!("📋 [DEBUG] UFSCUSTOM 필터링 후 총 레코드: {}", total_count);
    
    // 샘플링 수행
    let sampling_info = if total_count > max_records {
        println!("⚙️ [DEBUG] UFSCUSTOM 랜덤 샘플링 수행: {} -> {} 레코드", total_count, max_records);
        sample_ufscustom(&filtered_ufscustom, max_records)
    } else {
        println!("✅ [DEBUG] UFSCUSTOM 샘플링 불필요: {} 레코드 그대로 사용", total_count);
        SamplingInfo {
            data: filtered_ufscustom,
            total_count,
            sampled_count: total_count,
            sampling_ratio: 100.0,
        }
    };
    
    let limited_ufscustom = sampling_info.data;
    let sampled_count = sampling_info.sampled_count;
    let sampling_ratio = sampling_info.sampling_ratio;
    
    println!("📋 [DEBUG] UFSCUSTOM 샘플링 결과: sampled_count={}, sampling_ratio={:.1}%", sampled_count, sampling_ratio);
    
    // Arrow RecordBatch 변환 및 IPC 포맷으로 직렬화
    let batch = ufscustom_to_record_batch(&limited_ufscustom)?;
    let bytes = batch_to_ipc_bytes(&batch)?;
    
    Ok(TraceDataBytes {
        ufs: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
        block: ArrowBytes {
            bytes: vec![],
            total_count: 0,
            sampled_count: 0,
            sampling_ratio: 100.0,
        },
        ufscustom: ArrowBytes {
            bytes,
            total_count,
            sampled_count,
            sampling_ratio,
        },
    })
}

pub async fn filter_trace(params: FilterTraceParams) -> Result<TraceDataBytes, String> {
    if params.tracetype == "block" {
        filter_block_trace(&params.logname, &params.zoom_column, params.time_from, params.time_to, params.col_from, params.col_to, params.max_records)
            .await
    } else if params.tracetype == "ufs" {
        filter_ufs_trace(&params.logname, &params.zoom_column, params.time_from, params.time_to, params.col_from, params.col_to, params.max_records)
            .await
    } else if params.tracetype == "ufscustom" {
        filter_ufscustom_trace(&params.logname, &params.zoom_column, params.time_from, params.time_to, params.col_from, params.col_to, params.max_records)
            .await
    } else {
        Err(format!("Unknown trace type: {}", params.tracetype))
    }
}

// 캐시 초기화 함수
pub async fn clear_all_cache() -> Result<String, String> {
    println!("🧹 모든 캐시 초기화 시작");
    
    // UFS 캐시 초기화
    {
        let mut ufs_cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        let ufs_count = ufs_cache.len();
        ufs_cache.clear();
        println!("  - UFS 캐시 초기화: {} 항목 삭제", ufs_count);
    }
    
    // Block 캐시 초기화
    {
        let mut block_cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        let block_count = block_cache.len();
        block_cache.clear();
        println!("  - Block 캐시 초기화: {} 항목 삭제", block_count);
    }
    
    println!("✅ 모든 캐시 초기화 완료");
    Ok("캐시가 성공적으로 초기화되었습니다.".to_string())
}

/// DB에 등록된 로그 폴더들의 임시 Arrow 파일을 정리하는 함수
/// 
/// test.db의 folder 테이블과 testinfo 테이블에서 로그 폴더 경로를 가져와
/// 해당 폴더(및 하위 폴더)에 있는 오래된 임시 Arrow 파일들을 삭제합니다.
/// 
/// # Arguments
/// * `max_age_hours` - 삭제할 파일의 최대 나이 (시간 단위, 기본값: 24시간)
/// 
/// # Returns
/// * `Ok(usize)` - 삭제된 파일 수
/// * `Err(String)` - 에러 메시지
pub async fn cleanup_temp_arrow_files_impl(max_age_hours: u64) -> Result<usize, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::fs;
    use std::path::Path;
    
    // DB 경로 자동 찾기 (홈 디렉토리의 test.db)
    let home_dir = dirs::home_dir()
        .ok_or_else(|| "홈 디렉토리를 찾을 수 없습니다".to_string())?;
    let db_path = home_dir.join("test.db");
    let db_path_str = db_path.to_str()
        .ok_or_else(|| "DB 경로 변환 실패".to_string())?;
    
    println!("🧹 임시 파일 정리 시작 (DB: {})", db_path_str);
    
    let max_age_secs = max_age_hours * 3600;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    
    let mut deleted_count = 0;
    let mut folders_to_check = Vec::new();
    
    // SQLite 연결
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB 연결 실패: {}", e))?;
    
    // 1. folder 테이블에서 기본 로그 폴더 경로 가져오기
    {
        let mut stmt = conn.prepare("SELECT path FROM folder WHERE id = 1")
            .map_err(|e| format!("folder 테이블 쿼리 실패: {}", e))?;
        
        let paths: Result<Vec<String>, _> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("folder 데이터 읽기 실패: {}", e))?
            .collect();
        
        if let Ok(paths) = paths {
            folders_to_check.extend(paths);
        }
    }
    
    // 2. testinfo 테이블에서 모든 로그 폴더 경로 가져오기
    {
        let mut stmt = conn.prepare("SELECT DISTINCT logfolder FROM testinfo WHERE logfolder IS NOT NULL AND logfolder != ''")
            .map_err(|e| format!("testinfo 테이블 쿼리 실패: {}", e))?;
        
        let paths: Result<Vec<String>, _> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("testinfo 데이터 읽기 실패: {}", e))?
            .collect();
        
        if let Ok(paths) = paths {
            folders_to_check.extend(paths);
        }
    }
    
    println!("📂 검색할 폴더 수: {}", folders_to_check.len());
    
    // 각 폴더를 순회하며 임시 파일 검색 및 삭제
    for folder_path in folders_to_check {
        let path = Path::new(&folder_path);
        
        if !path.exists() || !path.is_dir() {
            continue;
        }
        
        // 폴더 내 파일 검색 (재귀적으로 하위 폴더도 검색)
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                
                // 하위 디렉토리면 재귀 검색
                if entry_path.is_dir() {
                    if let Ok(sub_entries) = fs::read_dir(&entry_path) {
                        for sub_entry in sub_entries.flatten() {
                            deleted_count += check_and_delete_temp_file(&sub_entry.path(), now, max_age_secs)?;
                        }
                    }
                } else {
                    // 현재 디렉토리의 파일 검사
                    deleted_count += check_and_delete_temp_file(&entry_path, now, max_age_secs)?;
                }
            }
        }
    }
    
    if deleted_count > 0 {
        println!("✅ 임시 파일 정리 완료: {}개 삭제", deleted_count);
    } else {
        println!("ℹ️  정리할 임시 파일 없음");
    }
    
    Ok(deleted_count)
}

/// 임시 파일인지 확인하고 오래된 파일이면 삭제
fn check_and_delete_temp_file(path: &Path, now: u64, max_age_secs: u64) -> Result<usize, String> {
    use std::fs;
    
    // 파일명 검사: estrace_temp_*.arrow 패턴
    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
        if filename.starts_with("estrace_temp_") && filename.ends_with(".arrow") {
            // 파일 메타데이터 확인
            if let Ok(metadata) = fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(modified_duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let file_age_secs = now.saturating_sub(modified_duration.as_secs());
                        
                        // 오래된 파일 삭제
                        if file_age_secs > max_age_secs {
                            match fs::remove_file(path) {
                                Ok(_) => {
                                    println!("🗑️  삭제: {} ({}시간 전)", 
                                        path.display(), 
                                        file_age_secs / 3600
                                    );
                                    return Ok(1);
                                }
                                Err(e) => {
                                    println!("⚠️  삭제 실패: {} - {}", path.display(), e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(0)
}