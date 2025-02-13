use std::fs::File;
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
pub struct TraceParseResult {
    pub missing_lines: Vec<usize>,
    pub filename: String,
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

#[tauri::command]
pub async fn readtrace(logfolder: String, logname: String) -> Result<Vec<UFS>, String> {
    let overall_start = Instant::now();
    let parquet_start = Instant::now();
    let path: String = format!("{}/{}", logfolder, logname);

    let ctx = SessionContext::new();
    // parquet 파일을 DataFrame으로 읽기
    let df: DataFrame = ctx
        .read_parquet(&path, ParquetReadOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    // DataFrame을 RecordBatch 단위로 수집
    let batches = df
        .collect()
        .await
        .map_err(|e| e.to_string())?;
    let parquet_time = parquet_start.elapsed();
    println!("Parquet read time: {:?}", parquet_time);

    let ufs_start = Instant::now();
    let mut ufs_list = Vec::new();

    for batch in batches {
        let num_rows = batch.num_rows();
        // 각 컬럼의 인덱스 미리 캐싱
        let time_idx = batch.schema().index_of("time").map_err(|e| e.to_string())?;
        let process_idx = batch.schema().index_of("process").map_err(|e| e.to_string())?;
        let cpu_idx = batch.schema().index_of("cpu").map_err(|e| e.to_string())?;
        let action_idx = batch.schema().index_of("action").map_err(|e| e.to_string())?;
        let tag_idx = batch.schema().index_of("tag").map_err(|e| e.to_string())?;
        let opcode_idx = batch.schema().index_of("opcode").map_err(|e| e.to_string())?;
        let lba_idx = batch.schema().index_of("lba").map_err(|e| e.to_string())?;
        let size_idx = batch.schema().index_of("size").map_err(|e| e.to_string())?;
        let groupid_idx = batch.schema().index_of("groupid").map_err(|e| e.to_string())?;
        let hwqid_idx = batch.schema().index_of("hwqid").map_err(|e| e.to_string())?;
        let qd_idx = batch.schema().index_of("qd").map_err(|e| e.to_string())?;
        let dtoc_idx = batch.schema().index_of("dtoc").map_err(|e| e.to_string())?;
        let ctoc_idx = batch.schema().index_of("ctoc").map_err(|e| e.to_string())?;
        let ctod_idx = batch.schema().index_of("ctod").map_err(|e| e.to_string())?;
        let continus_idx = batch.schema().index_of("continuous").map_err(|e| e.to_string())?;
        
        // 다운캐스팅은 한 번만 실행
        let time_array = batch.column(time_idx)
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("Failed to downcast 'time'".to_string())?;
        let process_array = batch.column(process_idx)
            .as_any()
            .downcast_ref::<arrow::array::StringViewArray>()
            .ok_or("Failed to downcast 'process'".to_string())?;
        let cpu_array = batch.column(cpu_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'cpu'".to_string())?;
        let action_array = batch.column(action_idx)
            .as_any()
            .downcast_ref::<arrow::array::StringViewArray>()
            .ok_or("Failed to downcast 'action'".to_string())?;
        let tag_array = batch.column(tag_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'tag'".to_string())?;
        let opcode_array = batch.column(opcode_idx)
            .as_any()
            .downcast_ref::<arrow::array::StringViewArray>()
            .ok_or("Failed to downcast 'opcode'".to_string())?;
        let lba_array = batch.column(lba_idx)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .ok_or("Failed to downcast 'lba'".to_string())?;
        let size_array = batch.column(size_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'size'".to_string())?;
        let groupid_array = batch.column(groupid_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'groupid'".to_string())?;
        let hwqid_array = batch.column(hwqid_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'hwqid'".to_string())?;
        let qd_array = batch.column(qd_idx)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or("Failed to downcast 'qd'".to_string())?;
        let dtoc_array = batch.column(dtoc_idx)
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("Failed to downcast 'dtoc'".to_string())?;
        let ctoc_array = batch.column(ctoc_idx)
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("Failed to downcast 'ctoc'".to_string())?;
        let ctod_array = batch.column(ctod_idx)
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("Failed to downcast 'ctod'".to_string())?;
        let continus_array = batch.column(continus_idx)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .ok_or("Failed to downcast 'continus'".to_string())?;
        
        // 재할당 비용 최소화를 위해 capacity 예약
        ufs_list.reserve(num_rows);
        
        for row in 0..num_rows {
            ufs_list.push(UFS {
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
                continuous: continus_array.value(row),
            });
        }
    }
    let ufs_time = ufs_start.elapsed();
    println!("UFS conversion time: {:?}", ufs_time);

    // 400만 레코드를 초과하는 경우, 랜덤 샘플링하여 preview용 레코드를 반환
    let preview_ufs_list = sample_ufs(&ufs_list);
    let overall_time = overall_start.elapsed();
    println!("Overall time: {:?}", overall_time);
    Ok(preview_ufs_list)
}

fn parse_caps(caps: &regex::Captures) -> Result<UFS, String> {
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

    let time: f64 = time_str.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let cpu: u32 = cpu_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let tag: u32 = tag_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let size: i32 = size_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let size: u32 = size.abs() as u32;
    let lba: u64 = lba_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let groupid: u32 = u32::from_str_radix(groupid_str, 16).map_err(|e| e.to_string())?;
    let hwqid: u32 = hwqid_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;

    Ok(UFS {
        time,
        process: process.to_string(),
        cpu,
        action: action.to_string(),
        tag,
        opcode: opcode.to_string(),
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

fn bottom_half_latency_process(mut ufs_list: Vec<UFS>) -> Vec<UFS> {
    use std::collections::HashMap;
    let mut req_times: HashMap<(u32, String), f64> = HashMap::new();
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    
    // 연속성 체크를 위한 변수: 이전 send_req의 (lba+size)와 opcode 저장
    let mut prev_end_addr: Option<u64> = None;
    let mut prev_opcode: Option<String> = None;

    // ufs_list는 시간순이라고 가정(필요하면 정렬 가능)
    for ufs in ufs_list.iter_mut() {
        match ufs.action.as_str() {
            "send_req" => {
                // 이전 send_req의 끝 주소와 opcode와 현재 요청 비교
                if let (Some(end_addr), Some(ref op)) = (prev_end_addr, prev_opcode.as_ref()) {
                    if ufs.lba == end_addr && ufs.opcode == **op {
                        ufs.continuous = true;
                    } else {
                        ufs.continuous = false;
                    }
                } else {
                    ufs.continuous = false;
                }
                // 이번 send_req의 끝 주소 및 opcode 저장
                prev_end_addr = Some(ufs.lba + ufs.size as u64);
                prev_opcode = Some(ufs.opcode.clone());

                req_times.insert((ufs.tag, ufs.opcode.clone()), ufs.time);
                current_qd += 1;
                if current_qd == 1 {
                    if let Some(t) = last_complete_qd0_time {
                        ufs.ctod = (ufs.time - t)*MILLISECONDS as f64;
                    }
                }
            },
            "complete_rsp" => {
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
            _ => {}
        }
        ufs.qd = current_qd;
    }
    ufs_list
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

fn generate_parquet_filename() -> String {
    let now = Local::now();
    format!("{}_ufs.parquet", now.format("%Y%m%d_%H%M%S"))
}

fn save_to_parquet(ufs_list: &[UFS], logfolder: String) -> Result<String, String> {
    let filename = generate_parquet_filename();
    let mut path = PathBuf::from(logfolder);
    path.push(&filename);  // 참조로 변경

    let batch = ufs_to_record_batch(ufs_list)?;
    let schema = batch.schema();
    let file = File::create(path).map_err(|e| e.to_string())?;
    
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None)
        .map_err(|e| e.to_string())?;
    
    writer.write(&batch).map_err(|e| e.to_string())?;
    writer.close().map_err(|e| e.to_string())?;

    Ok(filename)  // 생성된 파일명 반환
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
        let pattern = r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)";
        let re = Regex::new(pattern).map_err(|e| e.to_string())?;

        let mut ufs_list: Vec<UFS> = Vec::new();
        let mut missing_lines: Vec<usize> = Vec::new();

        let parsing_start = Instant::now();
        // for (i, line) in content.lines().enumerate() {
        //     let line_number = i + 1;
        //     if line.trim().is_empty() {
        //         continue;
        //     }
        //     if let Some(caps) = re.captures(line) {
        //         if let Ok(ufs) = parse_caps(&caps) {
        //             ufs_list.push(ufs);
        //         } else {
        //             missing_lines.push(line_number);
        //         }
        //     } else {
        //         missing_lines.push(line_number);
        //     }
        // }

        // 기존 반복문 대신 아래와 같이 파싱을 병렬 처리합니다.
        let lines: Vec<&str> = content.lines().collect();
        ufs_list.reserve(lines.len());
        let (ufs_vec, missing_vec): (Vec<UFS>, Vec<usize>) = lines
            .par_iter()
            .enumerate()
            .map(|(i, line)| {
                let line_number = i + 1;
                if line.trim().is_empty() {
                    (Vec::new(), vec![line_number])
                } else if let Some(caps) = re.captures(line) {
                    match parse_caps(&caps) {
                        Ok(ufs) => (vec![ufs], Vec::new()),
                        Err(_) => (Vec::new(), vec![line_number]),
                    }
                } else {
                    (Vec::new(), vec![line_number])
                }
            })
            .reduce(
                || (Vec::new(), Vec::new()),
                |(mut acc_ufs, mut acc_missing), (ufs_vec, missing_vec)| {
                    acc_ufs.extend(ufs_vec);
                    acc_missing.extend(missing_vec);
                    (acc_ufs, acc_missing)
                },
            );

        // ufs_vec에 파싱된 결과가, missing_vec에 파싱 실패한 줄 번호가 들어갑니다.
        ufs_list = ufs_vec;
        missing_lines = missing_vec;

        let parsing_time = parsing_start.elapsed();
        println!("Parsing time: {:?}", parsing_time);
        let bottom_half_start = Instant::now();
        // Bottom half: latency 계산 처리
        let processed_ufs_list = bottom_half_latency_process(ufs_list);
        let bottom_half_time = bottom_half_start.elapsed();
        println!("Bottom half time: {:?}", bottom_half_time);

        let save_parquet_start = Instant::now();
        // 파싱된 결과를 parquet 파일로 저장
        let filename = save_to_parquet(&processed_ufs_list, logfolder)?;
        let save_parquet_time = save_parquet_start.elapsed();
        println!("Save parquet time: {:?}", save_parquet_time);

        let overall_time = overall_start.elapsed();
        println!("Overall time: {:?}", overall_time);
        Ok(TraceParseResult {
            missing_lines,
            filename
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    result
}

// #[tauri::command]
// pub async fn starttrace(fname: String, logfolder: String) -> Result<TraceParseResult, String> {
//     let result = spawn_blocking(move || {
//         let file = File::open(&fname).map_err(|e| e.to_string())?;
//         let mut reader = BufReader::new(file);

//         // 수정된 정규식 패턴 - [cpu] 앞의 모든 문자열을 process로 캡처
//         let pattern = r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)";
//         let re = Regex::new(pattern).map_err(|e| e.to_string())?;

//         let mut ufs_list: Vec<UFS> = Vec::new();
//         let mut missing_lines: Vec<usize> = Vec::new();
//         let mut line_number: usize = 0;

//         const CHUNK_SIZE: usize = 1024 * 1024; // 1MB
//         let mut buf = vec![0u8; CHUNK_SIZE];
//         let mut remainder = String::new();
        
//         let overall_start = Instant::now();
//         let parsing_start = Instant::now();
//         loop {
//             let bytes_read = reader.read(&mut buf).map_err(|e| e.to_string())?;
//             if bytes_read == 0 {
//                 break;
//             }

//             let chunk_str = String::from_utf8_lossy(&buf[..bytes_read]);
//             let combined = format!("{}{}", remainder, chunk_str);
//             let mut lines: Vec<String> = combined.split('\n').map(|s| s.to_string()).collect();

//             if !combined.ends_with('\n') {
//                 remainder = lines.pop().unwrap_or_default();
//             } else {
//                 remainder.clear();
//             }

//             for line in lines {
//                 line_number += 1;
//                 if line.trim().is_empty() {
//                     continue;
//                 }
//                 if let Some(caps) = re.captures(&line) {
//                     if let Ok(ufs) = parse_caps(&caps) {
//                         ufs_list.push(ufs);
//                     } else {
//                         missing_lines.push(line_number);
//                     }
//                 } else {
//                     missing_lines.push(line_number);
//                 }
//             }
//         }

//         // 마지막 남은 불완전한 라인 처리
//         if !remainder.is_empty() {
//             line_number += 1;
//             if let Some(caps) = re.captures(&remainder) {
//                 if let Ok(ufs) = parse_caps(&caps) {
//                     ufs_list.push(ufs);
//                 } else {
//                     missing_lines.push(line_number);
//                 }
//             } else {
//                 missing_lines.push(line_number);
//             }
//         }
//         let parsing_time = parsing_start.elapsed();
//         println!("Parsing time: {:?}", parsing_time);

//         let bottom_half_start = Instant::now();
//         // Bottom half: latency 계산 처리
//         let processed_ufs_list = bottom_half_latency_process(ufs_list);
//         let bottom_half_time = bottom_half_start.elapsed();
//         println!("Bottom half time: {:?}", bottom_half_time);

//         let save_parquet_start = Instant::now();
//         // 파싱된 결과를 parquet 파일로 저장
//         let filename = save_to_parquet(&processed_ufs_list, logfolder)?;
//         let save_parquet_time = save_parquet_start.elapsed();
//         println!("Save parquet time: {:?}", save_parquet_time);

//         let overall_time = overall_start.elapsed();
//         println!("Overall time: {:?}", overall_time);
//         Ok(TraceParseResult {
//             missing_lines,
//             filename
//         })
//     })
//     .await
//     .map_err(|e| e.to_string())?;

//     result
// }