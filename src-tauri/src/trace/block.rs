use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use arrow::temporal_conversions::MILLISECONDS;
use parquet::arrow::ArrowWriter;

use crate::trace::filter::filter_block_data;
use crate::trace::utils::{
    calculate_statistics, create_range_key, initialize_ranges, normalize_io_type, parse_time_to_ms,
};
use crate::trace::{
    Block, ContinuityCount, ContinuityStats, LatencyStat, LatencyStats, LatencyValue, SizeStats,
    TotalContinuity, TraceStats,
};

// 레이턴시 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct LatencyStatsParams {
    pub logname: String,
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub thresholds: Vec<String>,
    pub group: bool,
}

// 크기 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct SizeStatsParams {
    pub logname: String,
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub group: bool,
}

// 종합 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct AllStatsParams {
    pub logname: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub thresholds: Vec<String>,
    pub group: bool,
}

// Block 레이턴시 후처리 함수
pub fn block_bottom_half_latency_process(block_list: Vec<Block>) -> Vec<Block> {
    // 이벤트가 없으면 빈 벡터 반환
    if block_list.is_empty() {
        return block_list;
    }
    
    // 시작 시간 기록
    let start_time = std::time::Instant::now();
    println!("Block Latency 처리 시작 (이벤트 수: {})", block_list.len());
    
    // 1. 시간순 정렬
    println!("  Block 데이터 시간순 정렬 중...");
    let mut sorted_blocks = block_list;
    sorted_blocks.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    // 2. 중복 block_rq_issue 제거 (사전 작업)
    println!("  중복 이벤트 필터링 중...");
    // 키를 (sector, io_type, size)로 확장하여 동일 크기의 요청만 중복으로 처리
    let mut processed_issues = HashSet::with_capacity(sorted_blocks.len() / 5);
    let mut deduplicated_blocks = Vec::with_capacity(sorted_blocks.len());

    // 프로그레스 카운터 - 중복 제거 단계
    let total_blocks = sorted_blocks.len();
    let report_interval = (total_blocks / 10).max(1); // 10% 간격으로 진행 상황 보고
    let mut last_reported = 0;
    
    for (idx, block) in sorted_blocks.into_iter().enumerate() {
        // 진행 상황 보고 (10% 간격)
        if idx >= last_reported + report_interval {
            let progress = (idx * 100) / total_blocks;
            println!("  중복 제거 진행률: {}% ({}/{})", progress, idx, total_blocks);
            last_reported = idx;
        }
        
        if block.action == "block_rq_issue" {
            let io_operation = if block.io_type.starts_with('R') {
                "read"
            } else if block.io_type.starts_with('W') {
                "write"
            } else if block.io_type.starts_with('D') {
                "discard"
            } else {
                "other"
            };

            // 키를 (sector, io_operation, size)로 확장
            let key = (block.sector, io_operation.to_string(), block.size);

            if processed_issues.contains(&key) {
                continue;
            }

            processed_issues.insert(key);
        } else if block.action == "block_rq_complete" {
            // complete일 경우 중복 체크 목록에서 제거
            let io_operation = if block.io_type.starts_with('R') {
                "read"
            } else if block.io_type.starts_with('W') {
                "write"
            } else if block.io_type.starts_with('D') {
                "discard"
            } else {
                "other"
            };

            // write 이고 size가 0인 경우에 Flush 표시가 2번 발생 (중복 제거) FF->WS 이런식으로 들어올 수 있음
            if block.io_type.starts_with('W') && block.size == 0 {
                continue;
            }

            let key = (block.sector, io_operation.to_string(), block.size);
            processed_issues.remove(&key);
        }

        deduplicated_blocks.push(block);
    }

    println!("  중복 제거 후 이벤트 수: {}", deduplicated_blocks.len());
    
    // 메모리 최적화를 위한 용량 조절
    processed_issues.clear();
    processed_issues.shrink_to_fit();
    
    // 3. 중복이 제거된 데이터에 대해 후처리 진행
    // (연속성, Latency 등 처리)
    println!("  Block Latency 및 연속성 계산 중...");
    let mut filtered_blocks = Vec::with_capacity(deduplicated_blocks.len());
    let mut req_times: HashMap<(u64, String), f64> = HashMap::with_capacity(deduplicated_blocks.len() / 5);
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    let mut prev_end_sector: Option<u64> = None;
    let mut prev_io_type: Option<String> = None;
    let mut first_c: bool = false;
    let mut first_complete_time: f64 = 0.0;

    // 프로그레스 카운터 - Latency 계산 단계
    let total_dedup = deduplicated_blocks.len();
    let report_interval_2 = (total_dedup / 10).max(1); 
    let mut last_reported_2 = 0;
    
    for (idx, mut block) in deduplicated_blocks.into_iter().enumerate() {
        // 진행 상황 보고 (10% 간격)
        if idx >= last_reported_2 + report_interval_2 {
            let progress = (idx * 100) / total_dedup;
            println!("  Latency 계산 진행률: {}% ({}/{})", progress, idx, total_dedup);
            last_reported_2 = idx;
        }
        
        // 기본적으로 continuous를 false로 설정
        block.continuous = false;

        let io_operation = if block.io_type.starts_with('R') {
            "read"
        } else if block.io_type.starts_with('W') {
            "write"
        } else if block.io_type.starts_with('D') {
            "discard"
        } else {
            "other"
        };

        let key = (block.sector, io_operation.to_string());

        match block.action.as_str() {
            "block_rq_issue" => {
                // 연속성 체크
                if io_operation != "other" {
                    if let (Some(end_sector), Some(prev_type)) =
                        (prev_end_sector, prev_io_type.as_ref())
                    {
                        if block.sector == end_sector && io_operation == prev_type {
                            block.continuous = true;
                        }
                    }

                    // 현재 요청의 끝 sector 및 io_type 업데이트
                    prev_end_sector = Some(block.sector + block.size as u64);
                    prev_io_type = Some(io_operation.to_string());
                }

                // 요청 시간 기록 및 QD 업데이트
                req_times.insert(key, block.time);
                current_qd += 1;

                if current_qd == 1 {
                    if let Some(t) = last_complete_qd0_time {
                        block.ctod = (block.time - t) * MILLISECONDS as f64;
                    }
                    first_c = true;
                    first_complete_time = block.time;
                }
            }
            "block_rq_complete" => {
                // complete는 항상 continuous = false
                if let Some(first_issue_time) = req_times.remove(&key) {
                    block.dtoc = (block.time - first_issue_time) * MILLISECONDS as f64;
                }

                match first_c {
                    true => {
                        block.ctoc = (block.time - first_complete_time) * MILLISECONDS as f64;
                        first_c = false;
                    }
                    false => {
                        if let Some(t) = last_complete_time {
                            block.ctoc = (block.time - t) * MILLISECONDS as f64;
                        }
                    }
                }

                current_qd = current_qd.saturating_sub(1);
                if current_qd == 0 {
                    last_complete_qd0_time = Some(block.time);
                }
                last_complete_time = Some(block.time);
            }
            _ => {}
        }

        block.qd = current_qd;
        filtered_blocks.push(block);
    }

    // 메모리 최적화를 위해 벡터 크기 조정
    filtered_blocks.shrink_to_fit();
    
    let elapsed = start_time.elapsed();
    println!("Block Latency 처리 완료: {:.2}초", elapsed.as_secs_f64());
    
    filtered_blocks
}

// Vec<Block>을 Arrow RecordBatch로 변환하는 함수
pub fn block_to_record_batch(block_list: &[Block]) -> Result<RecordBatch, String> {
    let time_array = Float64Array::from(block_list.iter().map(|b| b.time).collect::<Vec<_>>());
    let process_array = StringArray::from(
        block_list
            .iter()
            .map(|b| b.process.clone())
            .collect::<Vec<_>>(),
    );
    let cpu_array = UInt32Array::from(block_list.iter().map(|b| b.cpu).collect::<Vec<_>>());
    let flags_array = StringArray::from(
        block_list
            .iter()
            .map(|b| b.flags.clone())
            .collect::<Vec<_>>(),
    );
    let action_array = StringArray::from(
        block_list
            .iter()
            .map(|b| b.action.clone())
            .collect::<Vec<_>>(),
    );
    let devmajor_array =
        UInt32Array::from(block_list.iter().map(|b| b.devmajor).collect::<Vec<_>>());
    let devminor_array =
        UInt32Array::from(block_list.iter().map(|b| b.devminor).collect::<Vec<_>>());
    let io_type_array = StringArray::from(
        block_list
            .iter()
            .map(|b| b.io_type.clone())
            .collect::<Vec<_>>(),
    );
    let extra_array = UInt32Array::from(block_list.iter().map(|b| b.extra).collect::<Vec<_>>());
    let sector_array = UInt64Array::from(block_list.iter().map(|b| b.sector).collect::<Vec<_>>());
    let size_array = UInt32Array::from(block_list.iter().map(|b| b.size).collect::<Vec<_>>());
    let comm_array = StringArray::from(
        block_list
            .iter()
            .map(|b| b.comm.clone())
            .collect::<Vec<_>>(),
    );
    let qd_array = UInt32Array::from(block_list.iter().map(|b| b.qd).collect::<Vec<u32>>());
    let dtoc_array = Float64Array::from(block_list.iter().map(|b| b.dtoc).collect::<Vec<f64>>());
    let ctoc_array = Float64Array::from(block_list.iter().map(|b| b.ctoc).collect::<Vec<f64>>());
    let ctod_array = Float64Array::from(block_list.iter().map(|b| b.ctod).collect::<Vec<f64>>());
    let continuous_array = BooleanArray::from(
        block_list
            .iter()
            .map(|b| b.continuous)
            .collect::<Vec<bool>>(),
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

// Parquet 파일 저장 함수
pub fn save_block_to_parquet(
    block_traces: &[Block],
    logfolder: String,
    fname: String,
    timestamp: &str,
) -> Result<String, String> {
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
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).map_err(|e| e.to_string())?;
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

// Block 레이턴시 통계 함수
pub async fn latencystats(params: LatencyStatsParams) -> Result<Vec<u8>, String> {
    // threshold 문자열을 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &params.thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    // 필터링 적용
    let filtered_blocks =
        filter_block_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to)?;

    // LatencyStat 생성 - column에 따라 데이터 매핑
    let latency_stats: Vec<LatencyStat> = match params.column.as_str() {
        "dtoc" | "ctoc" => filtered_blocks
            .iter()
            .filter(|b| b.action == "block_rq_complete")
            .map(|b| LatencyStat {
                time: b.time,
                // grouping key로 io_type 사용
                opcode: if params.group {
                    normalize_io_type(&b.io_type)
                } else {
                    b.io_type.clone()
                },
                value: if params.column == "dtoc" {
                    LatencyValue::F64(b.dtoc)
                } else {
                    LatencyValue::F64(b.ctoc)
                },
            })
            .collect(),
        "ctod" => filtered_blocks
            .iter()
            .filter(|b| b.action == "block_rq_issue")
            .map(|b| LatencyStat {
                time: b.time,
                opcode: if params.group {
                    normalize_io_type(&b.io_type)
                } else {
                    b.io_type.clone()
                },
                value: LatencyValue::F64(b.ctod),
            })
            .collect(),
        "sector" => filtered_blocks
            .iter()
            .filter(|b| b.action == "block_rq_issue")
            .map(|b| LatencyStat {
                time: b.time,
                opcode: if params.group {
                    normalize_io_type(&b.io_type)
                } else {
                    b.io_type.clone()
                },
                value: LatencyValue::F64(b.sector as f64),
            })
            .collect(),
        _ => return Err(format!("유효하지 않은 컬럼: {}", params.column)),
    };

    // 시간순 정렬
    let mut filtered_stats = latency_stats;
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
        latency_counts.insert(io.clone(), initialize_ranges(&params.thresholds));
    }

    // 각 데이터에 대해 해당 io_type의 구간 카운트 증가
    for stat in &filtered_stats {
        let latency = stat.value.as_f64();
        let range_key = create_range_key(latency, &threshold_values, &params.thresholds);

        if let Some(io_counts) = latency_counts.get_mut(&stat.opcode) {
            if let Some(count) = io_counts.get_mut(&range_key) {
                *count += 1;
            }
        }
    }

    // io_type별 그룹핑 후 통계 계산
    let mut io_groups = BTreeMap::new();
    for stat in &filtered_stats {
        io_groups
            .entry(stat.opcode.clone())
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

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// Block 크기 통계 함수
pub async fn sizestats(params: SizeStatsParams) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_blocks =
        filter_block_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to)?;

    // column 조건에 따라 유효한 데이터만 필터링
    let filtered_blocks: Vec<&Block> = filtered_blocks
        .iter()
        .filter(|b| match params.column.as_str() {
            "dtoc" | "ctoc" => b.action == "block_rq_complete",
            "ctod" | "sector" => b.action == "block_rq_issue",
            _ => false,
        })
        .collect();

    // group 옵션에 따라 io_type을 normalize (첫 글자) 하거나 원본 사용
    let target_io_types: Vec<String> = filtered_blocks
        .iter()
        .map(|b| {
            if params.group {
                normalize_io_type(&b.io_type)
            } else {
                b.io_type.clone()
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut io_stats: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut total_counts: BTreeMap<String, usize> = BTreeMap::new();

    // 각 io_type별 빈 통계 맵 초기화
    for io in &target_io_types {
        io_stats.insert(io.clone(), BTreeMap::new());
        total_counts.insert(io.clone(), 0);
    }

    // size 기준 count 계산
    for block in &filtered_blocks {
        let io_key = if params.group {
            normalize_io_type(&block.io_type)
        } else {
            block.io_type.clone()
        };

        if let Some(size_counts) = io_stats.get_mut(&io_key) {
            *size_counts.entry(block.size).or_insert(0) += 1;
            *total_counts.get_mut(&io_key).unwrap() += 1;
        }
    }

    let result = SizeStats {
        opcode_stats: io_stats,
        total_counts,
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// Block 연속성 통계 함수
pub async fn continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_blocks =
        filter_block_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

    // block_rq_issue 동작만 필터링
    // R*(read) 또는 W*(write) D*(discard)로 시작하는 IO 타입만 포함
    let issues: Vec<&Block> = filtered_blocks
        .iter()
        .filter(|b| {
            b.action == "block_rq_issue"
                && (b.io_type.starts_with('R')
                    || b.io_type.starts_with('W')
                    || b.io_type.starts_with('D'))
        })
        .collect();

    // io_type 첫 글자(R/W/D)로 그룹화
    let mut op_stats: BTreeMap<String, ContinuityCount> = BTreeMap::new();
    let mut total_requests = 0;
    let mut total_continuous = 0;
    let mut total_bytes: u64 = 0;
    let mut continuous_bytes: u64 = 0;

    for block in &issues {
        let io_type = normalize_io_type(&block.io_type);

        // io_type별 통계 업데이트
        let stats = op_stats.entry(io_type).or_insert(ContinuityCount {
            continuous: 0,
            non_continuous: 0,
            ratio: 0.0,
            total_bytes: 0,
            continuous_bytes: 0,
            bytes_ratio: 0.0,
        });

        // Block의 size는 sector 단위(512 bytes)로 저장되어 있음
        let bytes = block.size as u64 * 512; // 1 sector = 512 bytes
        stats.total_bytes += bytes;
        total_bytes += bytes;

        if block.continuous {
            stats.continuous += 1;
            stats.continuous_bytes += bytes;
            total_continuous += 1;
            continuous_bytes += bytes;
        } else {
            stats.non_continuous += 1;
        }
        total_requests += 1;
    }

    // 비율 계산
    for (_, stats) in op_stats.iter_mut() {
        let total = stats.continuous + stats.non_continuous;
        if total > 0 {
            stats.ratio = (stats.continuous as f64) / (total as f64) * 100.0;
            stats.bytes_ratio =
                (stats.continuous_bytes as f64) / (stats.total_bytes as f64) * 100.0;
        }
    }

    // 전체 통계 계산
    let overall_ratio = if total_requests > 0 {
        (total_continuous as f64) / (total_requests as f64) * 100.0
    } else {
        0.0
    };

    let bytes_ratio = if total_bytes > 0 {
        (continuous_bytes as f64) / (total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let result = ContinuityStats {
        op_stats,
        total: TotalContinuity {
            total_requests,
            continuous_requests: total_continuous,
            overall_ratio,
            total_bytes,
            continuous_bytes,
            bytes_ratio,
        },
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// Block 전체 통계 계산 함수 - 단일 필터링으로 모든 통계 계산
pub async fn allstats(params: AllStatsParams) -> Result<Vec<u8>, String> {
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &params.thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    let filtered_blocks =
        filter_block_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to)?;

    let unique_io_types: std::collections::HashSet<String> = filtered_blocks
        .iter()
        .map(|b| if params.group { normalize_io_type(&b.io_type) } else { b.io_type.clone() })
        .collect();

    let mut dtoc_counts = std::collections::BTreeMap::new();
    let mut ctod_counts = std::collections::BTreeMap::new();
    let mut ctoc_counts = std::collections::BTreeMap::new();
    let mut dtoc_groups = std::collections::BTreeMap::new();
    let mut ctod_groups = std::collections::BTreeMap::new();
    let mut ctoc_groups = std::collections::BTreeMap::new();

    for io in &unique_io_types {
        dtoc_counts.insert(io.clone(), initialize_ranges(&params.thresholds));
        ctod_counts.insert(io.clone(), initialize_ranges(&params.thresholds));
        ctoc_counts.insert(io.clone(), initialize_ranges(&params.thresholds));
        dtoc_groups.insert(io.clone(), Vec::new());
        ctod_groups.insert(io.clone(), Vec::new());
        ctoc_groups.insert(io.clone(), Vec::new());
    }

    let mut io_stats = std::collections::BTreeMap::new();
    let mut total_counts = std::collections::BTreeMap::new();
    for io in &unique_io_types {
        io_stats.insert(io.clone(), std::collections::BTreeMap::new());
        total_counts.insert(io.clone(), 0usize);
    }

    let mut op_stats: std::collections::BTreeMap<String, ContinuityCount> =
        std::collections::BTreeMap::new();
    let mut total_requests = 0usize;
    let mut total_continuous = 0usize;
    let mut total_bytes: u64 = 0;
    let mut continuous_bytes: u64 = 0;

    for block in &filtered_blocks {
        let io_key = if params.group {
            normalize_io_type(&block.io_type)
        } else {
            block.io_type.clone()
        };

        if block.action == "block_rq_complete" {
            let range_key = create_range_key(block.dtoc, &threshold_values, &params.thresholds);
            if let Some(map) = dtoc_counts.get_mut(&io_key) {
                if let Some(v) = map.get_mut(&range_key) {
                    *v += 1;
                }
            }
            dtoc_groups.entry(io_key.clone()).or_default().push(block.dtoc);

            let range_key = create_range_key(block.ctoc, &threshold_values, &params.thresholds);
            if let Some(map) = ctoc_counts.get_mut(&io_key) {
                if let Some(v) = map.get_mut(&range_key) {
                    *v += 1;
                }
            }
            ctoc_groups.entry(io_key.clone()).or_default().push(block.ctoc);

            if let Some(size_map) = io_stats.get_mut(&io_key) {
                *size_map.entry(block.size).or_insert(0) += 1;
                *total_counts.get_mut(&io_key).unwrap() += 1;
            }
        }

        if block.action == "block_rq_issue" {
            let range_key = create_range_key(block.ctod, &threshold_values, &params.thresholds);
            if let Some(map) = ctod_counts.get_mut(&io_key) {
                if let Some(v) = map.get_mut(&range_key) {
                    *v += 1;
                }
            }
            ctod_groups.entry(io_key.clone()).or_default().push(block.ctod);

            if block.io_type.starts_with('R') || block.io_type.starts_with('W') || block.io_type.starts_with('D') {
                let stats = op_stats.entry(normalize_io_type(&block.io_type)).or_insert(ContinuityCount {
                    continuous: 0,
                    non_continuous: 0,
                    ratio: 0.0,
                    total_bytes: 0,
                    continuous_bytes: 0,
                    bytes_ratio: 0.0,
                });

                let bytes = block.size as u64 * 512;
                stats.total_bytes += bytes;
                total_bytes += bytes;

                if block.continuous {
                    stats.continuous += 1;
                    stats.continuous_bytes += bytes;
                    total_continuous += 1;
                    continuous_bytes += bytes;
                } else {
                    stats.non_continuous += 1;
                }
                total_requests += 1;
            }
        }
    }

    let mut dtoc_summary = std::collections::BTreeMap::new();
    for (io, mut values) in dtoc_groups {
        let summary = calculate_statistics(&mut values);
        dtoc_summary.insert(io, summary);
    }
    let mut ctod_summary = std::collections::BTreeMap::new();
    for (io, mut values) in ctod_groups {
        let summary = calculate_statistics(&mut values);
        ctod_summary.insert(io, summary);
    }
    let mut ctoc_summary = std::collections::BTreeMap::new();
    for (io, mut values) in ctoc_groups {
        let summary = calculate_statistics(&mut values);
        ctoc_summary.insert(io, summary);
    }

    let dtoc_stat = LatencyStats {
        latency_counts: dtoc_counts,
        summary: Some(dtoc_summary),
    };
    let ctod_stat = LatencyStats {
        latency_counts: ctod_counts,
        summary: Some(ctod_summary),
    };
    let ctoc_stat = LatencyStats {
        latency_counts: ctoc_counts,
        summary: Some(ctoc_summary),
    };

    let size_counts = SizeStats {
        opcode_stats: io_stats,
        total_counts,
    };

    for (_, stats) in op_stats.iter_mut() {
        let total = stats.continuous + stats.non_continuous;
        if total > 0 {
            stats.ratio = (stats.continuous as f64) / (total as f64) * 100.0;
            stats.bytes_ratio = (stats.continuous_bytes as f64) / (stats.total_bytes as f64) * 100.0;
        }
    }

    let overall_ratio = if total_requests > 0 {
        (total_continuous as f64) / (total_requests as f64) * 100.0
    } else {
        0.0
    };
    let bytes_ratio = if total_bytes > 0 {
        (continuous_bytes as f64) / (total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let continuity = ContinuityStats {
        op_stats,
        total: TotalContinuity {
            total_requests,
            continuous_requests: total_continuous,
            overall_ratio,
            total_bytes,
            continuous_bytes,
            bytes_ratio,
        },
    };

    let result = TraceStats {
        dtoc_stat,
        ctod_stat,
        ctoc_stat,
        size_counts,
        continuity,
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}
