use std::collections::{BTreeMap, HashMap};
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use tauri::Emitter;


use crate::trace::filter::{filter_ufscustom_data};
use crate::trace::utils::{
    calculate_statistics, create_range_key, initialize_ranges, parse_time_to_ms,
};
use crate::trace::{
    ContinuityCount, ContinuityStats, LatencyStat, LatencyStats, LatencyValue, SizeStats,
    TotalContinuity, TraceStats, UFSCUSTOM,
};

const MILLISECONDS_CONST: u32 = 1000;

// UFSCUSTOM 레이턴시 후처리 함수
pub fn ufscustom_bottom_half_latency_process(mut ufscustom_list: Vec<UFSCUSTOM>) -> Vec<UFSCUSTOM> {
    // 이벤트가 없으면 빈 벡터 반환
    if ufscustom_list.is_empty() {
        return ufscustom_list;
    }

    // 시작 시간 기록
    let start_time = std::time::Instant::now();
    println!("\n🔄 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 UFSCUSTOM Latency 후처리 시작");
    println!("   총 이벤트 수: {}", ufscustom_list.len());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // 정렬 여부 확인 (이미 정렬되어 있으면 정렬 스킵)
    println!("\n[1/3] ⏱️  데이터 순서 확인 중...");
    let sort_start = std::time::Instant::now();
    let mut needs_sort = false;
    for i in 1..ufscustom_list.len().min(1000) {
        if ufscustom_list[i - 1].start_time > ufscustom_list[i].start_time {
            needs_sort = true;
            break;
        }
    }
    
    let sort_elapsed = if needs_sort {
        println!("      ⚠️  정렬되지 않은 데이터 감지, 정렬 중...");
        ufscustom_list.sort_unstable_by(|a, b| {
            a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal)
        });
        let elapsed = sort_start.elapsed().as_secs_f64();
        println!("      ✅ 정렬 완료: {:.2}초", elapsed);
        elapsed
    } else {
        let elapsed = sort_start.elapsed().as_secs_f64();
        println!("      ✅ 이미 정렬됨 (정렬 스킵): {:.3}초", elapsed);
        elapsed
    };

    // 이벤트 기반 QD 계산을 위한 구조체
    #[derive(Debug, Clone, Copy)]
    struct Event {
        time: f64,
        event_type: EventType,
        request_idx: usize,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum EventType {
        Start,
        Complete,
    }

    // 모든 요청에 대한 이벤트 생성 (용량 사전 할당)
    println!("\n[2/3] 🔢 QD 계산 중...");
    let qd_calc_start = std::time::Instant::now();
    let mut events = Vec::with_capacity(ufscustom_list.len() * 2);
    for (idx, ufscustom) in ufscustom_list.iter().enumerate() {
        events.push(Event {
            time: ufscustom.start_time,
            event_type: EventType::Start,
            request_idx: idx,
        });
        events.push(Event {
            time: ufscustom.end_time,
            event_type: EventType::Complete,
            request_idx: idx,
        });
    }

    // 시간순으로 이벤트 정렬 (unstable sort)
    events.sort_unstable_by(|a, b| {
        a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal)
    });

    // 이벤트 처리하여 각 요청의 start_qd, end_qd 계산
    let mut current_qd = 0u32;
    let mut qd_values = vec![(0u32, 0u32); ufscustom_list.len()]; // (start_qd, end_qd)

    for event in &events {
        match event.event_type {
            EventType::Start => {
                current_qd += 1;
                qd_values[event.request_idx].0 = current_qd; // start_qd 설정 (1부터 시작)
            }
            EventType::Complete => {
                current_qd = current_qd.saturating_sub(1);
                qd_values[event.request_idx].1 = current_qd; // end_qd 설정
            }
        }
    }

    // 이벤트 벡터는 자동으로 스코프 종료시 해제됨
    
    let qd_calc_elapsed = qd_calc_start.elapsed().as_secs_f64();
    println!("      ✅ QD 계산 완료: {:.2}초", qd_calc_elapsed);

    // QD 값들을 실제 구조체에 설정
    for (idx, ufscustom) in ufscustom_list.iter_mut().enumerate() {
        ufscustom.start_qd = qd_values[idx].0;
        ufscustom.end_qd = qd_values[idx].1;
    }

    // CTOC, CTOD, continuous 계산
    let mut prev_request: Option<(u64, u32, String)> = None;
    let mut last_complete_time: Option<f64> = None;
    let mut last_qd_zero_complete_time: Option<f64> = None; // QD가 0이 될 때의 완료 시간
    
    let total_items = ufscustom_list.len();
    let report_threshold = total_items / 20; // 5% 간격
    
    println!("\n[3/3] ⚙️  Latency 및 연속성 계산 중...");
    let latency_start = std::time::Instant::now();

    for (i, ufscustom) in ufscustom_list.iter_mut().enumerate() {
        // 진행률 출력 (5% 간격, 모듈로 연산)
        if report_threshold > 0 && i % report_threshold == 0 && i > 0 {
            let progress = (i * 100) / total_items;
            let elapsed = latency_start.elapsed().as_secs_f64();
            let rate = i as f64 / elapsed;
            let remaining = total_items - i;
            let eta = if rate > 0.0 { remaining as f64 / rate } else { 0.0 };
            println!("      📌 진행률: {}% ({}/{}) | 속도: {:.0} events/s | 예상 남은 시간: {:.1}초", 
                     progress, i, total_items, rate, eta);
        }

        // continuous 요청 판단
        if let Some((prev_lba, prev_size, ref prev_opcode)) = prev_request {
            ufscustom.continuous = ufscustom.lba == prev_lba + prev_size as u64
                && ufscustom.opcode == *prev_opcode;
        } else {
            ufscustom.continuous = false;
        }

        // CTOC 계산 (Complete to Complete) - 이전 완료에서 현재 완료까지
        ufscustom.ctoc = if let Some(prev_complete) = last_complete_time {
            let time_diff = ufscustom.end_time - prev_complete;
            if time_diff >= 0.0 { time_diff * MILLISECONDS_CONST as f64 } else { 0.0 }
        } else {
            0.0 // 첫 번째 요청
        };

        // CTOD 계산 (Complete to Dispatch)
        // start_qd가 1인 경우: 이전 QD=0 완료에서 현재 시작까지
        // start_qd가 1이 아닌 경우: 이전 완료에서 현재 시작까지
        ufscustom.ctod = if ufscustom.start_qd == 1 {
            if let Some(prev_qd_zero_complete) = last_qd_zero_complete_time {
                let time_diff = ufscustom.start_time - prev_qd_zero_complete;
                if time_diff >= 0.0 { time_diff * MILLISECONDS_CONST as f64 } else { 0.0 }
            } else {
                0.0 // 첫 번째 idle 시작 요청
            }
        } else if let Some(prev_complete) = last_complete_time {
            let time_diff = ufscustom.start_time - prev_complete;
            if time_diff >= 0.0 { time_diff * MILLISECONDS_CONST as f64 } else { 0.0 }
        } else {
            0.0 // 첫 번째 요청
        };

        // 완료 시간 업데이트
        last_complete_time = Some(ufscustom.end_time);
        
        // QD가 0이 되는 완료 시간 업데이트
        if ufscustom.end_qd == 0 {
            last_qd_zero_complete_time = Some(ufscustom.end_time);
        }

        // 현재 요청 정보 저장
        prev_request = Some((ufscustom.lba, ufscustom.size, ufscustom.opcode.clone()));
    }

    let latency_elapsed = latency_start.elapsed().as_secs_f64();
    let latency_rate = ufscustom_list.len() as f64 / latency_elapsed;
    println!("      ✅ 계산 완료: {} 이벤트 | {:.2}초 | {:.0} events/s", 
             ufscustom_list.len(), latency_elapsed, latency_rate);
    
    // 메모리 최적화
    ufscustom_list.shrink_to_fit();

    let total_elapsed = start_time.elapsed().as_secs_f64();
    let total_rate = ufscustom_list.len() as f64 / total_elapsed;
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✨ UFSCUSTOM Latency 후처리 완료!");
    println!("   총 소요 시간: {:.2}초", total_elapsed);
    println!("   평균 처리 속도: {:.0} events/s", total_rate);
    println!("   최종 이벤트 수: {}", ufscustom_list.len());
    println!("   단계별 시간:");
    println!("     - 정렬: {:.2}초 ({:.1}%)", sort_elapsed, (sort_elapsed / total_elapsed) * 100.0);
    println!("     - QD 계산: {:.2}초 ({:.1}%)", qd_calc_elapsed, (qd_calc_elapsed / total_elapsed) * 100.0);
    println!("     - Latency 계산: {:.2}초 ({:.1}%)", latency_elapsed, (latency_elapsed / total_elapsed) * 100.0);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    ufscustom_list
}

// UFSCUSTOM을 RecordBatch로 변환하는 함수
pub fn ufscustom_to_record_batch(ufscustom_list: &[UFSCUSTOM]) -> Result<RecordBatch, String> {
    // 빈 벡터인 경우 빈 RecordBatch 반환
    if ufscustom_list.is_empty() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("opcode", DataType::Utf8, false),
            Field::new("lba", DataType::UInt64, false),
            Field::new("size", DataType::UInt32, false),
            Field::new("start_time", DataType::Float64, false),
            Field::new("end_time", DataType::Float64, false),            
            Field::new("start_qd", DataType::UInt32, false),
            Field::new("end_qd", DataType::UInt32, false),
            Field::new("dtoc", DataType::Float64, false),
            Field::new("ctoc", DataType::Float64, false),
            Field::new("ctod", DataType::Float64, false),
            Field::new("continuous", DataType::Boolean, false),
        ]));
        
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(Vec::<String>::new())),  // opcode
            Arc::new(UInt64Array::from(Vec::<u64>::new())),     // lba
            Arc::new(UInt32Array::from(Vec::<u32>::new())),     // size
            Arc::new(Float64Array::from(Vec::<f64>::new())),    // start_time
            Arc::new(Float64Array::from(Vec::<f64>::new())),    // end_time
            Arc::new(UInt32Array::from(Vec::<u32>::new())),     // start_qd
            Arc::new(UInt32Array::from(Vec::<u32>::new())),     // end_qd
            Arc::new(Float64Array::from(Vec::<f64>::new())),    // dtoc
            Arc::new(Float64Array::from(Vec::<f64>::new())),    // ctoc
            Arc::new(Float64Array::from(Vec::<f64>::new())),    // ctod
            Arc::new(BooleanArray::from(Vec::<bool>::new())),   // continuous
        ];
        
        return RecordBatch::try_new(schema, arrays).map_err(|e| e.to_string());
    }

    // 벡터들을 미리 할당
    let len = ufscustom_list.len();
    let mut opcode_vec = Vec::with_capacity(len);
    let mut lba_vec = Vec::with_capacity(len);
    let mut size_vec = Vec::with_capacity(len);
    let mut start_time_vec = Vec::with_capacity(len);
    let mut end_time_vec = Vec::with_capacity(len);    
    let mut start_qd_vec = Vec::with_capacity(len);
    let mut end_qd_vec = Vec::with_capacity(len);
    let mut dtoc_vec = Vec::with_capacity(len);
    let mut ctoc_vec = Vec::with_capacity(len);
    let mut ctod_vec = Vec::with_capacity(len);
    let mut continuous_vec = Vec::with_capacity(len);

    // 데이터 복사
    for ufscustom in ufscustom_list {
        opcode_vec.push(ufscustom.opcode.as_str());
        lba_vec.push(ufscustom.lba);
        size_vec.push(ufscustom.size);
        start_time_vec.push(ufscustom.start_time);
        end_time_vec.push(ufscustom.end_time);        
        start_qd_vec.push(ufscustom.start_qd);
        end_qd_vec.push(ufscustom.end_qd);
        dtoc_vec.push(ufscustom.dtoc);
        ctoc_vec.push(ufscustom.ctoc);
        ctod_vec.push(ufscustom.ctod);
        continuous_vec.push(ufscustom.continuous);
    }

    // 스키마 정의
    let schema = Arc::new(Schema::new(vec![
        Field::new("opcode", DataType::Utf8, false),
        Field::new("lba", DataType::UInt64, false),
        Field::new("size", DataType::UInt32, false),
        Field::new("start_time", DataType::Float64, false),
        Field::new("end_time", DataType::Float64, false),        
        Field::new("start_qd", DataType::UInt32, false),
        Field::new("end_qd", DataType::UInt32, false),
        Field::new("dtoc", DataType::Float64, false),
        Field::new("ctoc", DataType::Float64, false),
        Field::new("ctod", DataType::Float64, false),
        Field::new("continuous", DataType::Boolean, false),
    ]));

    // ArrayRef 벡터 생성
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(opcode_vec)),
        Arc::new(UInt64Array::from(lba_vec)),
        Arc::new(UInt32Array::from(size_vec)),
        Arc::new(Float64Array::from(start_time_vec)),
        Arc::new(Float64Array::from(end_time_vec)),        
        Arc::new(UInt32Array::from(start_qd_vec)),
        Arc::new(UInt32Array::from(end_qd_vec)),
        Arc::new(Float64Array::from(dtoc_vec)),
        Arc::new(Float64Array::from(ctoc_vec)),
        Arc::new(Float64Array::from(ctod_vec)),
        Arc::new(BooleanArray::from(continuous_vec)),
    ];

    RecordBatch::try_new(schema, arrays).map_err(|e| e.to_string())
}

// UFSCUSTOM을 Parquet 파일로 저장하는 함수 - chunk 단위로 분할하여 OOM 방지
pub fn save_ufscustom_to_parquet(
    ufscustom_list: &[UFSCUSTOM],
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

    let ufscustom_filename = format!("{}_ufscustom.parquet", timestamp);
    let mut path = folder_path;
    path.push(&ufscustom_filename);

    // chunk 크기 설정 (400,000 레코드씩 처리)
    const CHUNK_SIZE: usize = 400_000;
    let total_records = ufscustom_list.len();
    
    if total_records == 0 {
        return Err("저장할 데이터가 없습니다.".to_string());
    }
    
    println!("UFSCUSTOM 데이터 저장 시작: {} 레코드를 {} 레코드씩 Chunk로 처리", total_records, CHUNK_SIZE);
    
    let total_chunks = (total_records + CHUNK_SIZE - 1) / CHUNK_SIZE;
    
    // 첫 번째 Chunk로 스키마 생성
    let first_chunk = if total_records > CHUNK_SIZE {
        &ufscustom_list[0..CHUNK_SIZE]
    } else {
        ufscustom_list
    };
    
    let first_batch = ufscustom_to_record_batch(first_chunk)?;
    let schema = first_batch.schema();
    let file = File::create(&path).map_err(|e| e.to_string())?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).map_err(|e| e.to_string())?;
    
    // 첫 번째 Chunk 쓰기
    writer.write(&first_batch).map_err(|e| e.to_string())?;
    println!("UFSCUSTOM Chunk 1/{} 저장 완료", total_chunks);
    
    // 진행률 업데이트 (첫 번째 Chunk)
    if let Some(w) = window {
        let progress = 85.0 + (1.0 / total_chunks as f64) * 10.0; // 85%에서 95% 사이
        let _ = w.emit("trace-progress", crate::trace::ProgressEvent {
            stage: "saving".to_string(),
            progress: progress as f32,
            current: (85 + ((1 * 10) / total_chunks)) as u64,
            total: 100,
            message: format!("UFSCUSTOM Parquet 저장 중: {}/{} Chunk", 1, total_chunks),
            eta_seconds: (total_chunks - 1) as f32 * 0.5,
            processing_speed: 0.0,
        });
    }
    
    // 나머지 Chunk들 처리
    let mut chunk_num = 2;
    for chunk_start in (CHUNK_SIZE..total_records).step_by(CHUNK_SIZE) {
        let chunk_end = std::cmp::min(chunk_start + CHUNK_SIZE, total_records);
        let chunk = &ufscustom_list[chunk_start..chunk_end];
        
        let batch = ufscustom_to_record_batch(chunk)?;
        writer.write(&batch).map_err(|e| e.to_string())?;
        
        println!("UFSCUSTOM Chunk {}/{} 저장 완료", chunk_num, total_chunks);
        
        // 진행률 업데이트
        if let Some(w) = window {
            let progress = 85.0 + (chunk_num as f64 / total_chunks as f64) * 10.0;
            let _ = w.emit("trace-progress", crate::trace::ProgressEvent {
                stage: "saving".to_string(),
                progress: progress as f32,
                current: (85 + ((chunk_num * 10) / total_chunks)) as u64,
                total: 100,
                message: format!("UFSCUSTOM Parquet 저장 중: {}/{} Chunk", chunk_num, total_chunks),
                eta_seconds: (total_chunks - chunk_num) as f32 * 0.5,
                processing_speed: 0.0,
            });
        }
        
        chunk_num += 1;
    }
    
    writer.close().map_err(|e| e.to_string())?;
    println!("UFSCUSTOM Parquet 파일 저장 완료: {}", path.to_string_lossy());

    Ok(path.to_string_lossy().to_string())
}

// UFSCUSTOM 레이턴시 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfscustomLatencyStatsParams {
    pub logname: String,
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
    pub thresholds: Vec<String>,
}

// UFSCUSTOM 크기 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfscustomSizeStatsParams {
    pub logname: String,
    #[allow(dead_code)]
    pub column: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
}

// UFSCUSTOM 종합 통계 분석을 위한 매개변수 구조체
#[derive(Debug, Clone)]
pub struct UfscustomAllStatsParams {
    pub logname: String,
    pub zoom_column: String,
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
}

// UFSCUSTOM 레이턴시 통계 함수
pub async fn latencystats(params: UfscustomLatencyStatsParams) -> Result<Vec<u8>, String> {
    // 문자열 thresholds를 밀리초 값으로 변환
    let mut threshold_values: Vec<f64> = Vec::new();
    for t in &params.thresholds {
        let ms = parse_time_to_ms(t)?;
        threshold_values.push(ms);
    }

    // 필터링 적용
    let filtered_ufscustom =
        filter_ufscustom_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // LatencyStat 생성 - column에 따라 데이터 매핑
    let latency_stats = match params.column.as_str() {
        "dtoc" => filtered_ufscustom
            .iter()
            .map(|ufscustom| LatencyStat {
                time: ufscustom.start_time,
                opcode: ufscustom.opcode.clone(),
                value: LatencyValue::F64(ufscustom.dtoc),
            })
            .collect::<Vec<_>>(),
        "ctoc" => filtered_ufscustom
            .iter()
            .map(|ufscustom| LatencyStat {
                time: ufscustom.start_time,
                opcode: ufscustom.opcode.clone(),
                value: LatencyValue::F64(ufscustom.ctoc),
            })
            .collect::<Vec<_>>(),
        "ctod" => filtered_ufscustom
            .iter()
            .map(|ufscustom| LatencyStat {
                time: ufscustom.start_time,
                opcode: ufscustom.opcode.clone(),
                value: LatencyValue::F64(ufscustom.ctod),
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

        if let Some(opcode_map) = latency_counts.get_mut(&stat.opcode) {
            *opcode_map.entry(range_key).or_insert(0) += 1;
        }
    }

    // 백분위수 및 통계 계산 (opcode별로 수행)
    let mut summary = BTreeMap::new();
    let mut opcode_values: HashMap<String, Vec<f64>> = HashMap::new();

    for stat in &latency_stats {
        let latency = stat.value.as_f64();
        opcode_values
            .entry(stat.opcode.clone())
            .or_insert_with(Vec::new)
            .push(latency);
    }

    for (opcode, mut values) in opcode_values {
        let stats = calculate_statistics(&mut values);
        summary.insert(opcode, stats);
    }

    let result = LatencyStats {
        latency_counts,
        summary: Some(summary),
    };

    // JSON으로 직렬화 후 바이트로 변환
    serde_json::to_vec(&result).map_err(|e| format!("Failed to serialize latency stats: {}", e))
}

// UFSCUSTOM 크기 통계 함수
pub async fn sizestats(params: UfscustomSizeStatsParams) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_ufscustom =
        filter_ufscustom_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // opcode별로 size 분포 계산
    let mut opcode_stats: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut total_counts: BTreeMap<String, usize> = BTreeMap::new();

    for ufscustom in &filtered_ufscustom {
        *opcode_stats
            .entry(ufscustom.opcode.clone())
            .or_insert_with(BTreeMap::new)
            .entry(ufscustom.size)
            .or_insert(0) += 1;

        *total_counts
            .entry(ufscustom.opcode.clone())
            .or_insert(0) += 1;
    }

    let result = SizeStats {
        opcode_stats,
        total_counts,
    };

    // JSON으로 직렬화 후 바이트로 변환
    serde_json::to_vec(&result).map_err(|e| format!("Failed to serialize size stats: {}", e))
}

// UFSCUSTOM 연속성 통계 함수
pub async fn continuity_stats(
    logname: String,
    zoom_column: String,
    time_from: Option<f64>,
    time_to: Option<f64>,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<u8>, String> {
    // 필터링 적용
    let filtered_ufscustom = 
        filter_ufscustom_data(&logname, time_from, time_to, &zoom_column, col_from, col_to, None)?;

    // opcode별 연속성 통계 수집
    let mut op_stats: BTreeMap<String, ContinuityCount> = BTreeMap::new();
    let mut total_requests = 0;
    let mut total_continuous = 0;
    let mut total_bytes: u64 = 0;
    let mut continuous_bytes: u64 = 0;

    for ufscustom in &filtered_ufscustom {
        // opcode별 통계 업데이트
        let stats = op_stats
            .entry(ufscustom.opcode.clone())
            .or_insert(ContinuityCount {
                continuous: 0,
                non_continuous: 0,
                ratio: 0.0,
                total_bytes: 0,
                continuous_bytes: 0,
                bytes_ratio: 0.0,
            });

        // UFSCUSTOM의 size는 섹터 수 (512 bytes 단위)
        let bytes = ufscustom.size as u64 * 512; // 512 bytes per sector
        stats.total_bytes += bytes;
        total_bytes += bytes;

        if ufscustom.continuous {
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
            if stats.total_bytes > 0 {
                stats.bytes_ratio =
                    (stats.continuous_bytes as f64) / (stats.total_bytes as f64) * 100.0;
            }
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

    // JSON으로 직렬화 후 바이트로 변환
    serde_json::to_vec(&result).map_err(|e| format!("Failed to serialize continuity stats: {}", e))
}

// UFSCUSTOM 종합 통계 함수
pub async fn allstats(params: UfscustomAllStatsParams, thresholds: Vec<String>) -> Result<Vec<u8>, String> {
    // 필터링 적용 (전체 통계에서는 개별 함수에서 필터링하므로 여기서는 불필요)
    let _filtered_ufscustom =
        filter_ufscustom_data(&params.logname, params.time_from, params.time_to, &params.zoom_column, params.col_from, params.col_to, None)?;

    // dtoc 통계 계산
    let dtoc_params = UfscustomLatencyStatsParams {
        logname: params.logname.clone(),
        column: "dtoc".to_string(),
        zoom_column: params.zoom_column.clone(),
        time_from: params.time_from,
        time_to: params.time_to,
        col_from: params.col_from,
        col_to: params.col_to,
        thresholds: thresholds.clone(),
    };
    let dtoc_bytes = latencystats(dtoc_params).await?;
    let dtoc_stat: LatencyStats = serde_json::from_slice(&dtoc_bytes)
        .map_err(|e| format!("Failed to deserialize dtoc stats: {}", e))?;

    // ctod 통계 계산
    let ctod_params = UfscustomLatencyStatsParams {
        logname: params.logname.clone(),
        column: "ctod".to_string(),
        zoom_column: params.zoom_column.clone(),
        time_from: params.time_from,
        time_to: params.time_to,
        col_from: params.col_from,
        col_to: params.col_to,
        thresholds: thresholds.clone(),
    };
    let ctod_bytes = latencystats(ctod_params).await?;
    let ctod_stat: LatencyStats = serde_json::from_slice(&ctod_bytes)
        .map_err(|e| format!("Failed to deserialize ctod stats: {}", e))?;

    // ctoc 통계 계산
    let ctoc_params = UfscustomLatencyStatsParams {
        logname: params.logname.clone(),
        column: "ctoc".to_string(),
        zoom_column: params.zoom_column.clone(),
        time_from: params.time_from,
        time_to: params.time_to,
        col_from: params.col_from,
        col_to: params.col_to,
        thresholds: thresholds.clone(),
    };
    let ctoc_bytes = latencystats(ctoc_params).await?;
    let ctoc_stat: LatencyStats = serde_json::from_slice(&ctoc_bytes)
        .map_err(|e| format!("Failed to deserialize ctoc stats: {}", e))?;

    // 크기 통계 계산
    let size_params = UfscustomSizeStatsParams {
        logname: params.logname.clone(),
        column: "size".to_string(),
        zoom_column: params.zoom_column.clone(),
        time_from: params.time_from,
        time_to: params.time_to,
        col_from: params.col_from,
        col_to: params.col_to,
    };
    let size_bytes = sizestats(size_params).await?;
    let size_counts: SizeStats = serde_json::from_slice(&size_bytes)
        .map_err(|e| format!("Failed to deserialize size stats: {}", e))?;

    // 연속성 통계 계산
    let continuity_bytes = continuity_stats(
        params.logname.clone(),
        params.zoom_column.clone(),
        params.time_from,
        params.time_to,
        params.col_from,
        params.col_to,
    )
    .await?;
    let continuity: ContinuityStats = serde_json::from_slice(&continuity_bytes)
        .map_err(|e| format!("Failed to deserialize continuity stats: {}", e))?;

    // 종합 통계 생성
    let all_stats = TraceStats {
        dtoc_stat,
        ctod_stat,
        ctoc_stat,
        size_counts,
        continuity,
    };

    // JSON으로 직렬화 후 바이트로 변환
    serde_json::to_vec(&all_stats).map_err(|e| format!("Failed to serialize all stats: {}", e))
}
