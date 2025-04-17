use std::collections::{BTreeMap, HashMap};
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use arrow::temporal_conversions::MILLISECONDS;
use parquet::arrow::ArrowWriter;

use crate::trace::filter::filter_ufs_data;
use crate::trace::utils::{
    calculate_statistics, create_range_key, initialize_ranges, parse_time_to_ms,
};
use crate::trace::{
    ContinuityCount, ContinuityStats, LatencyStat, LatencyStats, LatencyValue, SizeStats,
    TotalContinuity, UFS,
};

// UFS 레이턴시 후처리 함수
pub fn ufs_bottom_half_latency_process(mut ufs_list: Vec<UFS>) -> Vec<UFS> {
    // 이벤트가 없으면 빈 벡터 반환
    if ufs_list.is_empty() {
        return ufs_list;
    }

    // 시작 시간 기록
    let start_time = std::time::Instant::now();
    println!("UFS 지연 시간 처리 시작 (이벤트 수: {})", ufs_list.len());
    
    // time 기준으로 오름차순 정렬
    println!("  UFS 데이터 시간순 정렬 중...");
    ufs_list.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    // 메모리 효율성을 위한 용량 최적화
    let estimated_capacity = ufs_list.len() / 10;
    let mut req_times: HashMap<(u32, String), f64> = HashMap::with_capacity(estimated_capacity);
    
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    let mut first_c: bool = false;
    let mut first_complete_time: f64 = 0.0;

    // 이전 send_req의 정보를 저장할 변수들
    let mut prev_send_req: Option<(u64, u32, String)> = None; // (lba, size, opcode)

    // 프로그레스 카운터
    let total_events = ufs_list.len();
    let report_interval = (total_events / 10).max(1); // 10% 간격으로 진행 상황 보고
    let mut last_reported = 0;
    
    println!("  UFS 지연 시간 및 연속성 계산 중...");

    for (idx, ufs) in ufs_list.iter_mut().enumerate() {
        // 진행 상황 보고 (10% 간격)
        if idx >= last_reported + report_interval {
            let progress = (idx * 100) / total_events;
            println!("  UFS 처리 진행률: {}% ({}/{})", progress, idx, total_events);
            last_reported = idx;
        }

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
                        ufs.ctod = (ufs.time - t) * MILLISECONDS as f64;
                    }
                    first_c = true;
                    first_complete_time = ufs.time;
                }
            }
            "complete_rsp" => {
                // complete_rsp는 continuous 체크하지 않음
                ufs.continuous = false;

                current_qd = current_qd.saturating_sub(1);
                if let Some(send_time) = req_times.remove(&(ufs.tag, ufs.opcode.clone())) {
                    ufs.dtoc = (ufs.time - send_time) * MILLISECONDS as f64;
                }
                match first_c {
                    true => {
                        ufs.ctoc = (ufs.time - first_complete_time) * MILLISECONDS as f64;
                        first_c = false;
                    }
                    false => {
                        if let Some(t) = last_complete_time {
                            ufs.ctoc = (ufs.time - t) * MILLISECONDS as f64;
                        }
                    }
                }
                if current_qd == 0 {
                    last_complete_qd0_time = Some(ufs.time);
                }
                last_complete_time = Some(ufs.time);
            }
            _ => {
                ufs.continuous = false;
            }
        }
        ufs.qd = current_qd;
    }

    // 메모리 최적화를 위해 벡터 크기 조정
    ufs_list.shrink_to_fit();

    let elapsed = start_time.elapsed();
    println!("UFS 지연 시간 처리 완료: {:.2}초", elapsed.as_secs_f64());
    
    ufs_list
}

// Vec<UFS>를 Arrow RecordBatch로 변환하는 함수
pub fn ufs_to_record_batch(ufs_list: &[UFS]) -> Result<RecordBatch, String> {
    // 각 필드별로 Arrow 배열 생성
    let time_array = Float64Array::from(ufs_list.iter().map(|u| u.time).collect::<Vec<f64>>());
    let process_array = StringArray::from(
        ufs_list
            .iter()
            .map(|u| u.process.clone())
            .collect::<Vec<String>>(),
    );
    let cpu_array = UInt32Array::from(ufs_list.iter().map(|u| u.cpu).collect::<Vec<u32>>());
    let action_array = StringArray::from(
        ufs_list
            .iter()
            .map(|u| u.action.clone())
            .collect::<Vec<String>>(),
    );
    let tag_array = UInt32Array::from(ufs_list.iter().map(|u| u.tag).collect::<Vec<u32>>());
    let opcode_array = StringArray::from(
        ufs_list
            .iter()
            .map(|u| u.opcode.clone())
            .collect::<Vec<String>>(),
    );
    let lba_array = UInt64Array::from(ufs_list.iter().map(|u| u.lba).collect::<Vec<u64>>());
    let size_array = UInt32Array::from(ufs_list.iter().map(|u| u.size).collect::<Vec<u32>>());
    let groupid_array = UInt32Array::from(ufs_list.iter().map(|u| u.groupid).collect::<Vec<u32>>());
    let hwqid_array = UInt32Array::from(ufs_list.iter().map(|u| u.hwqid).collect::<Vec<u32>>());
    let qd_array = UInt32Array::from(ufs_list.iter().map(|u| u.qd).collect::<Vec<u32>>());
    let dtoc_array = Float64Array::from(ufs_list.iter().map(|u| u.dtoc).collect::<Vec<f64>>());
    let ctoc_array = Float64Array::from(ufs_list.iter().map(|u| u.ctoc).collect::<Vec<f64>>());
    let ctod_array = Float64Array::from(ufs_list.iter().map(|u| u.ctod).collect::<Vec<f64>>());
    let continues_array =
        BooleanArray::from(ufs_list.iter().map(|u| u.continuous).collect::<Vec<bool>>());

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
        Field::new("continuous", DataType::Boolean, false),
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
            Arc::new(continues_array) as ArrayRef,
        ],
    )
    .map_err(|e| e.to_string())
}

// Parquet 파일 저장 함수
pub fn save_ufs_to_parquet(
    ufs_list: &[UFS],
    logfolder: String,
    fname: String,
    timestamp: &str,
) -> Result<String, String> {
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
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).map_err(|e| e.to_string())?;
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

// UFS 레이턴시 통계 함수
pub async fn latencystats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
    thresholds: Vec<String>,
) -> Result<String, String> {
    // 문자열 thresholds를 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

    // LatencyStat 생성 - column에 따라 데이터 매핑
    let mut latency_stats = match column.as_str() {
        "dtoc" | "ctoc" => filtered_ufs
            .iter()
            .filter(|ufs| ufs.action == "complete_rsp")
            .map(|ufs| LatencyStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: if column == "dtoc" {
                    LatencyValue::F64(ufs.dtoc)
                } else {
                    LatencyValue::F64(ufs.ctoc)
                },
            })
            .collect::<Vec<_>>(),
        "ctod" => filtered_ufs
            .iter()
            .filter(|ufs| ufs.action == "send_req")
            .map(|ufs| LatencyStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: LatencyValue::F64(ufs.ctod),
            })
            .collect::<Vec<_>>(),
        "lba" => filtered_ufs
            .iter()
            .map(|ufs| LatencyStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: LatencyValue::F64(ufs.lba as f64),
            })
            .collect::<Vec<_>>(),
        _ => return Err("Invalid column".to_string()),
    };

    // 시간순 정렬
    latency_stats.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());

    // opcode별 latency count 초기화
    let mut latency_counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();

    // 모든 opcode에 대해 threshold 구간 초기화
    let unique_opcodes: Vec<String> = latency_stats
        .iter()
        .map(|stat| stat.opcode.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for opcode in &unique_opcodes {
        latency_counts.insert(opcode.clone(), initialize_ranges(&thresholds));
    }

    // 각 데이터에 대해 해당하는 opcode의 구간 카운트 증가
    for stat in &latency_stats {
        let latency = stat.value.as_f64();
        let range_key = create_range_key(latency, &threshold_values, &thresholds);

        if let Some(opcode_ranges) = latency_counts.get_mut(&stat.opcode) {
            if let Some(count) = opcode_ranges.get_mut(&range_key) {
                *count += 1;
            }
        }
    }

    // opcode별 그룹핑
    let mut opcode_groups = BTreeMap::new();
    for stat in &latency_stats {
        opcode_groups
            .entry(stat.opcode.clone())
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
        summary: Some(summary_map),
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// UFS 크기 통계 함수
pub async fn sizestats(
    logname: String,
    column: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

    // 관심있는 opcode들
    let target_opcodes = ["0x2a", "0x28", "0x42"];

    // column에 따른 추가 필터링 (action 체크)
    let filtered_ufs: Vec<&UFS> = filtered_ufs
        .iter()
        .filter(|ufs| match column.as_str() {
            "dtoc" | "ctoc" => ufs.action == "complete_rsp",
            "ctod" | "lba" => ufs.action == "send_req",
            _ => false,
        })
        .filter(|ufs| target_opcodes.contains(&ufs.opcode.as_str()))
        .collect();

    // opcode별 size 통계 계산
    let mut opcode_stats: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut total_counts: BTreeMap<String, usize> = BTreeMap::new();

    // 각 opcode에 대해 빈 통계 맵 초기화
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

// UFS 연속성 통계 함수
pub async fn continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<String, String> {
    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&logname, time_from, time_to, &zoom_column, col_from, col_to)?;

    // send_req 동작만 필터링 (연속성은 send_req에서만 의미 있음)
    // 주로 관심 있는 opcode만 필터링: 0x28(read), 0x2a(write)
    let send_reqs: Vec<&UFS> = filtered_ufs
        .iter()
        .filter(|ufs| {
            ufs.action == "send_req"
                && (ufs.opcode == "0x28" || ufs.opcode == "0x2a" || ufs.opcode == "0x42")
        })
        .collect();

    // opcode별 연속성 통계 수집
    let mut op_stats: BTreeMap<String, ContinuityCount> = BTreeMap::new();
    let mut total_requests = 0;
    let mut total_continuous = 0;
    let mut total_bytes: u64 = 0;
    let mut continuous_bytes: u64 = 0;

    for ufs in &send_reqs {
        // opcode별 통계 업데이트
        let stats = op_stats
            .entry(ufs.opcode.clone())
            .or_insert(ContinuityCount {
                continuous: 0,
                non_continuous: 0,
                ratio: 0.0,
                total_bytes: 0,
                continuous_bytes: 0,
                bytes_ratio: 0.0,
            });

        // UFS의 size 필드는 이미 4KB 단위로 저장되어 있음
        let bytes = ufs.size as u64 * 4096; // 4KB = 4096 bytes
        stats.total_bytes += bytes;
        total_bytes += bytes;

        if ufs.continuous {
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

    serde_json::to_string(&result).map_err(|e| e.to_string())
}
