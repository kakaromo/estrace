use std::collections::{BTreeMap, HashMap};
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use arrow::temporal_conversions::MILLISECONDS;
use parquet::arrow::ArrowWriter;
use tauri::Emitter;

use crate::trace::filter::{filter_ufs_data};
use crate::trace::utils::{
    calculate_statistics, create_range_key, initialize_ranges, parse_time_to_ms,
};
use crate::trace::{
    ContinuityCount, ContinuityStats, LatencyStat, LatencyStats, LatencyValue, SizeStats,
    TotalContinuity, TraceStats, UFS,
};

// UFS 레이턴시 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfsLatencyStatsParams {
    pub logname: String,
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub thresholds: Vec<String>,
}

// UFS 크기 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfsSizeStatsParams {
    pub logname: String,
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
}

// UFS 종합 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfsAllStatsParams {
    pub logname: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
}

// UFS 레이턴시 후처리 함수
pub fn ufs_bottom_half_latency_process(mut ufs_list: Vec<UFS>) -> Vec<UFS> {
    // 이벤트가 없으면 빈 벡터 반환
    if ufs_list.is_empty() {
        return ufs_list;
    }

    // 시작 시간 기록
    let start_time = std::time::Instant::now();
    println!("\n🔄 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 UFS Latency 후처리 시작");
    println!("   총 이벤트 수: {}", ufs_list.len());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // 정렬 여부 확인 (이미 정렬되어 있으면 정렬 스킵)
    println!("\n[1/2] ⏱️  데이터 순서 확인 중...");
    let sort_start = std::time::Instant::now();
    let mut needs_sort = false;
    for i in 1..ufs_list.len().min(1000) {
        if ufs_list[i - 1].time > ufs_list[i].time {
            needs_sort = true;
            break;
        }
    }
    
    let sort_elapsed = if needs_sort {
        println!("      ⚠️  정렬되지 않은 데이터 감지, 정렬 중...");
        ufs_list.sort_unstable_by(|a, b| {
            a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal)
        });
        let elapsed = sort_start.elapsed().as_secs_f64();
        println!("      ✅ 정렬 완료: {:.2}초", elapsed);
        elapsed
    } else {
        let elapsed = sort_start.elapsed().as_secs_f64();
        println!("      ✅ 이미 정렬됨 (정렬 스킵): {:.3}초", elapsed);
        elapsed
    };

    // 메모리 효율성을 위한 용량 최적화 (더 정확한 추정)
    let estimated_capacity = (ufs_list.len() / 4).max(1024);
    let mut req_times: HashMap<(u32, String), f64> = HashMap::with_capacity(estimated_capacity);
    
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    let mut first_c: bool = false;
    let mut first_complete_time: f64 = 0.0;

    // 이전 send_req의 정보를 저장할 변수들
    let mut prev_send_req: Option<(u64, u32, String)> = None; // (lba, size, opcode)

    // 프로그레스 카운터 최적화
    let total_events = ufs_list.len();
    let report_threshold = total_events / 20; // 5% 간격 (더 적은 출력)
    
    println!("\n[2/2] ⚙️  Latency 및 연속성 계산 중...");
    let processing_start = std::time::Instant::now();

    for (idx, ufs) in ufs_list.iter_mut().enumerate() {
        // 진행 상황 보고 (5% 간격, 모듈로 연산 사용)
        if report_threshold > 0 && idx % report_threshold == 0 && idx > 0 {
            let progress = (idx * 100) / total_events;
            let elapsed = processing_start.elapsed().as_secs_f64();
            let rate = idx as f64 / elapsed;
            let remaining = total_events - idx;
            let eta = if rate > 0.0 { remaining as f64 / rate } else { 0.0 };
            println!("      📌 진행률: {}% ({}/{}) | 속도: {:.0} events/s | 예상 남은 시간: {:.1}초", 
                     progress, idx, total_events, rate, eta);
        }

        // 성능 최적화: 문자열 비교를 바이트 비교로 대체
        let action_bytes = ufs.action.as_bytes();
        
        if action_bytes == b"send_req" {
            // 연속성 체크: 이전 send_req가 있는 경우
            if let Some((prev_lba, prev_size, ref prev_opcode)) = prev_send_req {
                let prev_end_addr = prev_lba + prev_size as u64;
                // 현재 요청의 시작 주소가 이전 요청의 끝 주소와 같고, opcode가 같은 경우
                ufs.continuous = ufs.lba == prev_end_addr && ufs.opcode == *prev_opcode;
            } else {
                ufs.continuous = false;
            }

            // 현재 send_req 정보 저장 (clone 최소화)
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
        } else if action_bytes == b"complete_rsp" {
            // complete_rsp는 continuous 체크하지 않음
            ufs.continuous = false;

            current_qd = current_qd.saturating_sub(1);
            if let Some(send_time) = req_times.remove(&(ufs.tag, ufs.opcode.clone())) {
                ufs.dtoc = (ufs.time - send_time) * MILLISECONDS as f64;
            }
            
            // 조건 분기 최적화
            if first_c {
                ufs.ctoc = (ufs.time - first_complete_time) * MILLISECONDS as f64;
                first_c = false;
            } else if let Some(t) = last_complete_time {
                ufs.ctoc = (ufs.time - t) * MILLISECONDS as f64;
            }
            
            if current_qd == 0 {
                last_complete_qd0_time = Some(ufs.time);
            }
            last_complete_time = Some(ufs.time);
        } else {
            ufs.continuous = false;
        }
        ufs.qd = current_qd;
    }

    let processing_elapsed = processing_start.elapsed().as_secs_f64();
    let processing_rate = ufs_list.len() as f64 / processing_elapsed;
    println!("      ✅ 계산 완료: {} 이벤트 | {:.2}초 | {:.0} events/s", 
             ufs_list.len(), processing_elapsed, processing_rate);
    
    // 메모리 최적화를 위해 벡터 크기 조정
    ufs_list.shrink_to_fit();

    let total_elapsed = start_time.elapsed().as_secs_f64();
    let total_rate = ufs_list.len() as f64 / total_elapsed;
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✨ UFS Latency 후처리 완료!");
    println!("   총 소요 시간: {:.2}초", total_elapsed);
    println!("   평균 처리 속도: {:.0} events/s", total_rate);
    println!("   최종 이벤트 수: {}", ufs_list.len());
    println!("   단계별 시간:");
    println!("     - 정렬: {:.2}초 ({:.1}%)", sort_elapsed, (sort_elapsed / total_elapsed) * 100.0);
    println!("     - Latency 계산: {:.2}초 ({:.1}%)", processing_elapsed, (processing_elapsed / total_elapsed) * 100.0);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    
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

// Parquet 파일 저장 함수 - chunk 단위로 분할하여 OOM 방지
pub fn save_ufs_to_parquet(
    ufs_list: &[UFS],
    logfolder: String,
    fname: String,
    timestamp: &str,
    window: Option<&tauri::Window>,
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

    // chunk 크기 설정 (100,000 레코드씩 처리)
    const CHUNK_SIZE: usize = 400_000;
    let total_records = ufs_list.len();
    
    if total_records == 0 {
        return Err("저장할 데이터가 없습니다.".to_string());
    }
    
    println!("UFS 데이터 저장 시작: {} 레코드를 {} 레코드씩 Chunk로 처리", total_records, CHUNK_SIZE);
    
    let total_chunks = (total_records + CHUNK_SIZE - 1) / CHUNK_SIZE;
    
    // 첫 번째 Chunk로 스키마 생성
    let first_chunk = if total_records > CHUNK_SIZE {
        &ufs_list[0..CHUNK_SIZE]
    } else {
        ufs_list
    };
    
    let first_batch = ufs_to_record_batch(first_chunk)?;
    let schema = first_batch.schema();
    let file = File::create(&path).map_err(|e| e.to_string())?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).map_err(|e| e.to_string())?;
    
        // 첫 번째 Chunk 쓰기
    writer.write(&first_batch).map_err(|e| e.to_string())?;
    println!("UFS Chunk 1/{} 저장 완료", total_chunks);
    
    // 진행률 업데이트 (첫 번째 Chunk)
    if let Some(w) = window {
        let progress = 85.0 + (1.0 / total_chunks as f64) * 10.0; // 85%에서 95% 사이
        let _ = w.emit("trace-progress", crate::trace::ProgressEvent {
            stage: "saving".to_string(),
            progress: progress as f32,
            current: (85 + ((1 * 10) / total_chunks)) as u64,
            total: 100,
            message: format!("UFS Parquet 저장 중: {}/{} Chunk", 1, total_chunks),
            eta_seconds: (total_chunks - 1) as f32 * 0.5,
            processing_speed: 0.0,
        });
    }
    
    // 나머지 Chunk들 처리
    let mut chunk_num = 2;
    for chunk_start in (CHUNK_SIZE..total_records).step_by(CHUNK_SIZE) {
        let chunk_end = std::cmp::min(chunk_start + CHUNK_SIZE, total_records);
        let chunk = &ufs_list[chunk_start..chunk_end];
        
        let batch = ufs_to_record_batch(chunk)?;
        writer.write(&batch).map_err(|e| e.to_string())?;
        
        println!("UFS Chunk {}/{} 저장 완료", chunk_num, total_chunks);
        
        // 진행률 업데이트
        if let Some(w) = window {
            let progress = 85.0 + (chunk_num as f64 / total_chunks as f64) * 10.0;
            let _ = w.emit("trace-progress", crate::trace::ProgressEvent {
                stage: "saving".to_string(),
                progress: progress as f32,
                current: (85 + ((chunk_num * 10) / total_chunks)) as u64,
                total: 100,
                message: format!("UFS Parquet 저장 중: {}/{} Chunk", chunk_num, total_chunks),
                eta_seconds: (total_chunks - chunk_num) as f32 * 0.5,
                processing_speed: 0.0,
            });
        }
        
        chunk_num += 1;
    }
    
    writer.close().map_err(|e| e.to_string())?;
    println!("UFS Parquet 파일 저장 완료: {}", path.to_string_lossy());

    Ok(path.to_string_lossy().to_string())
}

// UFS 레이턴시 통계 함수
pub async fn latencystats(params: UfsLatencyStatsParams) -> Result<Vec<u8>, String> {
    // 문자열 thresholds를 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &params.thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // LatencyStat 생성 - column에 따라 데이터 매핑
    let latency_stats = match params.column.as_str() {
        "dtoc" | "ctoc" => filtered_ufs
            .iter()
            .filter(|ufs| ufs.action == "complete_rsp")
            .map(|ufs| LatencyStat {
                time: ufs.time,
                opcode: ufs.opcode.clone(),
                value: if params.column == "dtoc" {
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
        _ => return Err(format!("Invalid column: {}", params.column)),
    };

    // 이미 parquet에서 시간순으로 정렬되어 있으므로 정렬 불필요

    // 각 opcode별 레이턴시 카운트 초기화
    let mut latency_counts = std::collections::BTreeMap::new();
    let opcodes: std::collections::HashSet<String> = latency_stats
        .iter()
        .map(|stat| stat.opcode.clone())
        .collect();

    for opcode in opcodes {
        latency_counts.insert(opcode.clone(), initialize_ranges(&params.thresholds));
    }

    // 각 데이터의 latency에 따라 구간 카운트 증가
    for stat in &latency_stats {
        let latency = stat.value.as_f64();
        let range_key = create_range_key(latency, &threshold_values, &params.thresholds);

        if let Some(opcode_counts) = latency_counts.get_mut(&stat.opcode) {
            if let Some(count) = opcode_counts.get_mut(&range_key) {
                *count += 1;
            }
        }
    }

    // opcode별 그룹핑 후 통계 계산
    let mut opcode_groups = std::collections::BTreeMap::new();
    for stat in &latency_stats {
        opcode_groups
            .entry(stat.opcode.clone())
            .or_insert_with(Vec::new)
            .push(stat.value.as_f64());
    }

    // 각 opcode별 통계 계산
    let mut summary_map = std::collections::BTreeMap::new();
    for (opcode, mut values) in opcode_groups {
        let summary = calculate_statistics(&mut values);
        summary_map.insert(opcode, summary);
    }

    let result = LatencyStats {
        latency_counts,
        summary: Some(summary_map),
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// UFS 크기 통계 함수
pub async fn sizestats(params: UfsSizeStatsParams) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // column 조건에 따라 유효한 데이터만 필터링
    let filtered_ufs: Vec<&UFS> = filtered_ufs
        .iter()
        .filter(|ufs| match params.column.as_str() {
            "dtoc" | "ctoc" => ufs.action == "complete_rsp",
            "ctod" => ufs.action == "send_req",
            _ => false,
        })
        .collect();

    // opcode별 통계 초기화
    let mut opcode_stats: std::collections::BTreeMap<String, std::collections::BTreeMap<u32, usize>> =
        std::collections::BTreeMap::new();
    let mut total_counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    // 모든 opcode 수집
    let opcodes: Vec<String> = filtered_ufs
        .iter()
        .map(|ufs| ufs.opcode.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for opcode in &opcodes {
        opcode_stats.insert(opcode.clone(), std::collections::BTreeMap::new());
        total_counts.insert(opcode.clone(), 0);
    }

    // size 기준 count 계산
    for ufs in &filtered_ufs {
        if let Some(size_counts) = opcode_stats.get_mut(&ufs.opcode) {
            let size_kb = ufs.size;

            *size_counts.entry(size_kb).or_insert(0) += 1;
            *total_counts.get_mut(&ufs.opcode).unwrap() += 1;
        }
    }

    // 응답 객체 생성
    let result = SizeStats {
        opcode_stats,
        total_counts,
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// UFS 연속성 통계 함수
pub async fn continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&logname, time_from, time_to, &zoom_column, col_from, col_to, None)?;

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

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}

// UFS 전체 통계 계산 함수 - 단일 필터링으로 모든 통계 계산
pub async fn allstats(params: UfsAllStatsParams, thresholds: Vec<String>) -> Result<Vec<u8>, String> {
    // 문자열 threshold를 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    // 필터링 적용
    let filtered_ufs =
        filter_ufs_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // 모든 opcode 수집
    let opcodes: Vec<String> = filtered_ufs
        .iter()
        .map(|ufs| ufs.opcode.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // 통계 변수 초기화
    let mut dtoc_counts = std::collections::BTreeMap::new();
    let mut ctod_counts = std::collections::BTreeMap::new();
    let mut ctoc_counts = std::collections::BTreeMap::new();
    let mut dtoc_groups = std::collections::BTreeMap::new();
    let mut ctod_groups = std::collections::BTreeMap::new();
    let mut ctoc_groups = std::collections::BTreeMap::new();

    let mut size_stats = std::collections::BTreeMap::new();
    let mut total_counts = std::collections::BTreeMap::new();

    let mut opcode_qd = std::collections::BTreeMap::new();

    // 초기화
    for opcode in &opcodes {
        dtoc_counts.insert(opcode.clone(), initialize_ranges(&thresholds));
        ctod_counts.insert(opcode.clone(), initialize_ranges(&thresholds));
        ctoc_counts.insert(opcode.clone(), initialize_ranges(&thresholds));
        dtoc_groups.insert(opcode.clone(), Vec::new());
        ctod_groups.insert(opcode.clone(), Vec::new());
        ctoc_groups.insert(opcode.clone(), Vec::new());
        size_stats.insert(opcode.clone(), std::collections::BTreeMap::new());
        total_counts.insert(opcode.clone(), 0);
        opcode_qd.insert(opcode.clone(), Vec::new());
    }

    // 연속성 통계를 위한 변수 초기화
    let mut op_stats: BTreeMap<String, ContinuityCount> = BTreeMap::new();
    let mut total_requests = 0;
    let mut total_continuous = 0;
    let mut total_bytes_continuity: u64 = 0;
    let mut continuous_bytes: u64 = 0;

    // 전체 통계 한번에 계산
    for ufs in &filtered_ufs {
        if ufs.action == "complete_rsp" {
            // DTOC 레이턴시 통계
            let range_key = create_range_key(ufs.dtoc, &threshold_values, &thresholds);
            if let Some(counts) = dtoc_counts.get_mut(&ufs.opcode) {
                if let Some(count) = counts.get_mut(&range_key) {
                    *count += 1;
                }
            }
            dtoc_groups.entry(ufs.opcode.clone()).or_default().push(ufs.dtoc);

            // CTOC 레이턴시 통계
            let range_key = create_range_key(ufs.ctoc, &threshold_values, &thresholds);
            if let Some(counts) = ctoc_counts.get_mut(&ufs.opcode) {
                if let Some(count) = counts.get_mut(&range_key) {
                    *count += 1;
                }
            }
            ctoc_groups.entry(ufs.opcode.clone()).or_default().push(ufs.ctoc);

            // QD 통계
            opcode_qd.entry(ufs.opcode.clone()).or_default().push(ufs.qd as f64);
        }

        if ufs.action == "send_req" {
            // CTOD 레이턴시 통계
            let range_key = create_range_key(ufs.ctod, &threshold_values, &thresholds);
            if let Some(counts) = ctod_counts.get_mut(&ufs.opcode) {
                if let Some(count) = counts.get_mut(&range_key) {
                    *count += 1;
                }
            }
            ctod_groups.entry(ufs.opcode.clone()).or_default().push(ufs.ctod);

            // 연속성 통계 (send_req에서만 연속성이 의미가 있음)
            if ufs.opcode == "0x28" || ufs.opcode == "0x2a" || ufs.opcode == "0x42" {
                // opcode별 연속성 통계 업데이트
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
                total_bytes_continuity += bytes;

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
        }

        // 크기 통계 (KB 단위로 변환)
        let size_kb = ufs.size * 4; // 4KB 단위이므로 4를 곱함
        if let Some(size_counts) = size_stats.get_mut(&ufs.opcode) {
            *size_counts.entry(size_kb).or_insert(0) += 1;
            *total_counts.get_mut(&ufs.opcode).unwrap() += 1;
        }
    }

    // 연속성 통계의 비율 계산
    for (_, stats) in op_stats.iter_mut() {
        let total = stats.continuous + stats.non_continuous;
        if total > 0 {
            stats.ratio = (stats.continuous as f64) / (total as f64) * 100.0;
            stats.bytes_ratio =
                (stats.continuous_bytes as f64) / (stats.total_bytes as f64) * 100.0;
        }
    }

    // 통계 요약 계산
    let mut dtoc_summary = std::collections::BTreeMap::new();
    let mut ctod_summary = std::collections::BTreeMap::new();
    let mut ctoc_summary = std::collections::BTreeMap::new();
    let mut qd_summary = std::collections::BTreeMap::new();

    for (opcode, mut values) in dtoc_groups {
        dtoc_summary.insert(opcode, calculate_statistics(&mut values));
    }

    for (opcode, mut values) in ctod_groups {
        ctod_summary.insert(opcode, calculate_statistics(&mut values));
    }

    for (opcode, mut values) in ctoc_groups {
        ctoc_summary.insert(opcode, calculate_statistics(&mut values));
    }

    for (opcode, mut values) in opcode_qd {
        qd_summary.insert(opcode, calculate_statistics(&mut values));
    }

    // 결과 객체 생성
    let dtoc_stats = LatencyStats {
        latency_counts: dtoc_counts,
        summary: Some(dtoc_summary),
    };

    let ctod_stats = LatencyStats {
        latency_counts: ctod_counts,
        summary: Some(ctod_summary),
    };

    let ctoc_stats = LatencyStats {
        latency_counts: ctoc_counts,
        summary: Some(ctoc_summary),
    };

    let size_result = SizeStats {
        opcode_stats: size_stats,
        total_counts,
    };

    // 전체 연속성 통계 계산
    let overall_ratio = if total_requests > 0 {
        (total_continuous as f64) / (total_requests as f64) * 100.0
    } else {
        0.0
    };

    let bytes_ratio = if total_bytes_continuity > 0 {
        (continuous_bytes as f64) / (total_bytes_continuity as f64) * 100.0
    } else {
        0.0
    };

    // TraceStats 구조체를 사용 (UfsTraceStats 대신)
    let result = TraceStats {
        dtoc_stat: dtoc_stats,
        ctod_stat: ctod_stats,
        ctoc_stat: ctoc_stats,
        size_counts: size_result,
        continuity: ContinuityStats {
            op_stats,
            total: TotalContinuity {
                total_requests,
                continuous_requests: total_continuous,
                overall_ratio,
                total_bytes: total_bytes_continuity,
                continuous_bytes,
                bytes_ratio,
            },
        },
    };

    serde_json::to_vec(&result).map_err(|e| e.to_string())
}
