use std::fs::File;
use std::io::{BufReader, Read};
use arrow::temporal_conversions::MILLISECONDS;
use regex::Regex;
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use chrono::Local;

use std::path::PathBuf;
use std::sync::Arc;
use arrow::array::{ArrayRef, Float64Array, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use datafusion::prelude::*;

#[derive(Serialize, Debug)]
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
    ctod: f64      // Complete to Device latency
}

#[derive(Serialize, Debug)]
pub struct TraceParseResult {
    pub parsed: Vec<UFS>,
    pub missing_lines: Vec<usize>,
    pub filename: String,
}

#[tauri::command]
pub async fn readtrace(logfolder: String, logname: String) -> Result<Vec<UFS>, String> {
    // 조합된 parquet 파일 경로 생성
    let path: String = format!("{}/{}", logfolder, logname);

    let ctx = SessionContext::new();
    // parquet 파일을 DataFrame으로 읽기
    let df = ctx
        .read_parquet(&path, ParquetReadOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    // DataFrame을 RecordBatch 단위로 수집
    let batches = df
        .collect()
        .await
        .map_err(|e| e.to_string())?;

    let mut ufs_list = Vec::new();
    // 각 RecordBatch와 각 row를 순회하며 UFS 구조체로 변환
    for batch in batches {
        let num_rows = batch.num_rows();
        for row in 0..num_rows {
            let time = extract_f64(&batch, "time", row)?;
            let process = extract_string(&batch, "process", row)?;
            let cpu = extract_u32(&batch, "cpu", row)?;
            let action = extract_string(&batch, "action", row)?;
            let tag = extract_u32(&batch, "tag", row)?;
            let opcode = extract_string(&batch, "opcode", row)?;
            let lba = extract_u64(&batch, "lba", row)?;
            let size = extract_u32(&batch, "size", row)?;
            let groupid = extract_u32(&batch, "groupid", row)?;
            let hwqid = extract_u32(&batch, "hwqid", row)?;
            let qd = extract_u32(&batch, "qd", row)?;
            let dtoc = extract_f64(&batch, "dtoc", row)?;
            let ctoc = extract_f64(&batch, "ctoc", row)?;
            let ctod = extract_f64(&batch, "ctod", row)?;

            ufs_list.push(UFS {
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
                qd,
                dtoc,
                ctoc,
                ctod,
            });
        }
    }
    Ok(ufs_list)
}

fn extract_f64(batch: &RecordBatch, col: &str, row: usize) -> Result<f64, String> {
    let index = batch.schema().index_of(col).map_err(|e| e.to_string())?;
    let array = batch.column(index)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or("Failed to downcast to Float64Array".to_string())?;
    Ok(array.value(row))
}

fn extract_string(batch: &RecordBatch, col: &str, row: usize) -> Result<String, String> {
    let index = batch.schema().index_of(col).map_err(|e| e.to_string())?;
    
    // DataType::Utf8에 대응하는 타입은 기본적으로 arrow::array::StringArray (GenericStringArray<i32>)입니다.
    if let Some(array) = batch.column(index)
        .as_any()
        .downcast_ref::<arrow::array::StringViewArray>() {
        return Ok(array.value(row).to_string());
    }
    // 만약 필요한 경우, LargeStringArray도 시도할 수 있습니다.
    if let Some(array) = batch.column(index)
        .as_any()
        .downcast_ref::<arrow::array::LargeStringArray>() {
        return Ok(array.value(row).to_string());
    }
    Err(format!(
        "Failed to downcast column '{}' (with data type: {:?}) to a known string array type",
        col,
        batch.schema().field(index).data_type()
    ))
}

fn extract_u32(batch: &RecordBatch, col: &str, row: usize) -> Result<u32, String> {
    let index = batch.schema().index_of(col).map_err(|e| e.to_string())?;
    let array = batch.column(index)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or("Failed to downcast to UInt32Array".to_string())?;
    Ok(array.value(row))
}

fn extract_u64(batch: &RecordBatch, col: &str, row: usize) -> Result<u64, String> {
    let index = batch.schema().index_of(col).map_err(|e| e.to_string())?;
    let array = batch.column(index)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or("Failed to downcast to UInt64Array".to_string())?;
    Ok(array.value(row))
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
        
    })
}

fn bottom_half_latency_process(mut ufs_list: Vec<UFS>) -> Vec<UFS> {
    use std::collections::HashMap;
    let mut req_times: HashMap<(u32, String), f64> = HashMap::new();
    let mut current_qd: u32 = 0;
    let mut last_complete_time: Option<f64> = None;
    let mut last_complete_qd0_time: Option<f64> = None;
    

    // ufs_list는 시간순이라고 가정(필요하면 정렬 가능)
    for ufs in ufs_list.iter_mut() {
        match ufs.action.as_str() {
            "send_req" => {
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
        Field::new("ctod", DataType::Float64, false)
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
            Arc::new(ctod_array) as ArrayRef
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
        let file = File::open(&fname).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(file);

        // 수정된 정규식 패턴 - [cpu] 앞의 모든 문자열을 process로 캡처
        let pattern = r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)";
        let re = Regex::new(pattern).map_err(|e| e.to_string())?;

        let mut ufs_list: Vec<UFS> = Vec::new();
        let mut missing_lines: Vec<usize> = Vec::new();
        let mut line_number: usize = 0;

        const CHUNK_SIZE: usize = 1024 * 1024; // 1MB
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut remainder = String::new();

        loop {
            let bytes_read = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if bytes_read == 0 {
                break;
            }

            let chunk_str = String::from_utf8_lossy(&buf[..bytes_read]);
            let combined = format!("{}{}", remainder, chunk_str);
            let mut lines: Vec<String> = combined.split('\n').map(|s| s.to_string()).collect();

            if !combined.ends_with('\n') {
                remainder = lines.pop().unwrap_or_default();
            } else {
                remainder.clear();
            }

            for line in lines {
                line_number += 1;
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(caps) = re.captures(&line) {
                    if let Ok(ufs) = parse_caps(&caps) {
                        ufs_list.push(ufs);
                    } else {
                        missing_lines.push(line_number);
                    }
                } else {
                    missing_lines.push(line_number);
                }
            }
        }

        // 마지막 남은 불완전한 라인 처리
        if !remainder.is_empty() {
            line_number += 1;
            if let Some(caps) = re.captures(&remainder) {
                if let Ok(ufs) = parse_caps(&caps) {
                    ufs_list.push(ufs);
                } else {
                    missing_lines.push(line_number);
                }
            } else {
                missing_lines.push(line_number);
            }
        }

        // Bottom half: latency 계산 처리
        let processed_ufs_list = bottom_half_latency_process(ufs_list);

        // 파싱된 결과를 parquet 파일로 저장
        let filename = save_to_parquet(&processed_ufs_list, logfolder)?;

        Ok(TraceParseResult {
            parsed: processed_ufs_list,
            missing_lines,
            filename
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    result
}