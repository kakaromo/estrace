use std::fs::File;
use std::path::PathBuf;

use arrow::array::{RecordBatchWriter, Float64Array, Array};
use arrow::datatypes::DataType;
use datafusion::arrow::csv::WriterBuilder;
use datafusion::prelude::*; // RecordBatchWriter 트레이트 추가
use serde::Deserialize;
use tauri::Emitter;

use crate::trace::ProgressEvent;

// Excel의 최대 행 수 (헤더 제외)
const EXCEL_MAX_ROWS: usize = 1_048_575;

// 필터 파라미터 구조체
#[derive(Debug, Deserialize)]
pub struct FilterParams {
    pub time_from: Option<f64>,
    pub time_to: Option<f64>,
    pub zoom_column: Option<String>,  // "lba" or "sector"
    pub col_from: Option<f64>,
    pub col_to: Option<f64>,
}

// CSV 내보내기 공통 함수 (필터 지원)
pub async fn export_to_csv(
    parquet_path: String,
    output_dir: Option<String>,
    filter: Option<FilterParams>,
    window: Option<tauri::Window>,
) -> Result<Vec<String>, String> {
    let start_time = std::time::Instant::now();
    
    // 진행 상태 업데이트
    if let Some(w) = &window {
        let _ = w.emit("export-progress", ProgressEvent {
            stage: "reading".to_string(),
            progress: 0.0,
            current: 0,
            total: 100,
            message: "Parquet 파일 읽는 중...".to_string(),
            eta_seconds: 0.0,
            processing_speed: 0.0,
        });
    }

    // DataFusion 세션 초기화
    let ctx = SessionContext::new();

    // Parquet 파일 읽기
    let mut df = ctx
        .read_parquet(parquet_path.as_str(), ParquetReadOptions::default())
        .await
        .map_err(|e| e.to_string())?;

    // 필터 적용
    if let Some(filter_params) = filter {
        println!("📊 [Export] 필터 적용 중...");
        
        if let Some(w) = &window {
            let _ = w.emit("export-progress", ProgressEvent {
                stage: "filtering".to_string(),
                progress: 10.0,
                current: 10,
                total: 100,
                message: "필터 적용 중...".to_string(),
                eta_seconds: 0.0,
                processing_speed: 0.0,
            });
        }
        
        // 시간 필터 적용
        if let (Some(t_from), Some(t_to)) = (filter_params.time_from, filter_params.time_to) {
            if t_from > 0.0 || t_to > 0.0 {
                let schema = df.schema();
                let time_column = if schema.fields().iter().any(|f| f.name() == "start_time") {
                    "start_time"
                } else {
                    "time"
                };
                
                df = df
                    .filter(col(time_column).gt_eq(lit(t_from)).and(col(time_column).lt_eq(lit(t_to))))
                    .map_err(|e| e.to_string())?;
                
                println!("⏱️  [Export] 시간 필터: {} ~ {}", t_from, t_to);
            }
        }
        
        // LBA/Sector 필터 적용
        if let (Some(zoom_col), Some(c_from), Some(c_to)) = 
            (filter_params.zoom_column.as_ref(), filter_params.col_from, filter_params.col_to) {
            if c_from > 0.0 || c_to > 0.0 {
                df = df
                    .filter(col(zoom_col.as_str()).gt_eq(lit(c_from as i64)).and(col(zoom_col.as_str()).lt_eq(lit(c_to as i64))))
                    .map_err(|e| e.to_string())?;
                
                println!("📍 [Export] {} 필터: {} ~ {}", zoom_col, c_from, c_to);
            }
        }
    }

    // 스키마에서 시간 컬럼 이름 결정 (start_time 또는 time)
    let schema = df.schema();
    let time_column = if schema.fields().iter().any(|f| f.name() == "start_time") {
        "start_time"
    } else {
        "time"
    };

    // Parquet 파일이 이미 시간순으로 정렬되어 있으므로 정렬 불필요
    println!("✅ [Export] Parquet 파일이 이미 정렬되어 있음 (정렬 스킵)");

    // 데이터프레임에서 레코드 배치 가져오기
    let batches = df.collect().await.map_err(|e| e.to_string())?;

    // 총 행 수 계산
    let total_rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();
    println!("📊 [Export] 총 {} 행을 처리합니다", total_rows);
    
    if let Some(w) = &window {
        let _ = w.emit("export-progress", ProgressEvent {
            stage: "exporting".to_string(),
            progress: 20.0,
            current: 20,
            total: 100,
            message: format!("CSV 내보내기 시작... (all {} raw)", total_rows),
            eta_seconds: 0.0,
            processing_speed: 0.0,
        });
    }

    // 출력 파일 기본 경로 설정
    let (base_dir, base_filename) = if let Some(dir) = output_dir {
        let input_path = PathBuf::from(&parquet_path);
        let filename = input_path
            .file_stem()
            .ok_or("Invalid parquet path")?
            .to_string_lossy();
        (PathBuf::from(dir), filename.to_string())
    } else {
        let input_path = PathBuf::from(&parquet_path);
        let parent = input_path.parent().ok_or("Invalid parquet path")?;
        let filename = input_path
            .file_stem()
            .ok_or("Invalid parquet path")?
            .to_string_lossy();
        (PathBuf::from(parent), filename.to_string())
    };

    let mut output_paths = Vec::new();
    let mut processed_rows = 0;
    let export_start = std::time::Instant::now();
    
    // 시간 값 추출 헬퍼 함수
    let get_time_value = |batch: &arrow::record_batch::RecordBatch, row_index: usize| -> Option<f64> {
        let schema = batch.schema();
        let time_col_index = schema.column_with_name(time_column)?.0;
        let time_array = batch.column(time_col_index);
        
        if let DataType::Float64 = time_array.data_type() {
            let float_array = time_array.as_any().downcast_ref::<Float64Array>()?;
            if row_index < float_array.len() && !float_array.is_null(row_index) {
                return Some(float_array.value(row_index));
            }
        }
        None
    };

    // 청크별로 배치를 메모리에 모아둘 벡터
    let mut current_chunk_batches: Vec<arrow::record_batch::RecordBatch> = Vec::new();
    let mut current_row_count = 0;
    let mut chunk_start_time: Option<f64> = None;
    let mut chunk_end_time: Option<f64> = None;
    let mut current_chunk_number = 1;
    let total_chunks = (total_rows + EXCEL_MAX_ROWS - 1) / EXCEL_MAX_ROWS;

    // 각 배치를 처리하면서 청크 단위로 분할
    for batch in batches {
        let batch_rows = batch.num_rows();
        let mut batch_offset = 0;

        while batch_offset < batch_rows {
            // 현재 청크에 추가 가능한 행 수 계산
            let remaining_capacity = EXCEL_MAX_ROWS - current_row_count;
            let rows_to_write = std::cmp::min(remaining_capacity, batch_rows - batch_offset);

            // 배치에서 필요한 부분만 슬라이스
            let slice_batch = if rows_to_write == batch_rows && batch_offset == 0 {
                batch.clone()
            } else {
                batch.slice(batch_offset, rows_to_write)
            };

            // 청크의 시작 시간 설정 (첫 번째 배치의 첫 번째 행)
            if chunk_start_time.is_none() && slice_batch.num_rows() > 0 {
                chunk_start_time = get_time_value(&slice_batch, 0);
            }
            
            // 청크의 끝 시간 갱신 (마지막 배치의 마지막 행)
            if slice_batch.num_rows() > 0 {
                chunk_end_time = get_time_value(&slice_batch, slice_batch.num_rows() - 1);
            }

            // 메모리에 배치 추가
            current_chunk_batches.push(slice_batch);
            current_row_count += rows_to_write;
            batch_offset += rows_to_write;
            
            // 진행 중인 파일의 row 진행 상황 업데이트 (100,000 row마다)
            if current_row_count % 100_000 == 0 || current_row_count >= EXCEL_MAX_ROWS {
                let temp_processed = processed_rows + current_row_count;
                let progress = 20.0 + ((temp_processed as f64 / total_rows as f64) * 70.0) as f32;
                let elapsed = export_start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { temp_processed as f64 / elapsed } else { 0.0 };
                
                if let Some(w) = &window {
                    let _ = w.emit("export-progress", ProgressEvent {
                        stage: "writing".to_string(),
                        progress,
                        current: temp_processed as u64,
                        total: total_rows as u64,
                        message: format!("파일 {}/{} 작성 중... ({}/{} rows)", current_chunk_number, total_chunks, current_row_count, EXCEL_MAX_ROWS.min(total_rows - processed_rows)),
                        eta_seconds: if speed > 0.0 { ((total_rows - temp_processed) as f64 / speed) as f32 } else { 0.0 },
                        processing_speed: speed as f32,
                    });
                }
            }

            // 청크가 가득 찼거나 마지막 배치인 경우 파일로 저장
            if current_row_count >= EXCEL_MAX_ROWS {
                // 파일명 생성 (시작 시간이 끝 시간보다 작도록 보장)
                let (start, end) = match (chunk_start_time, chunk_end_time) {
                    (Some(s), Some(e)) if s <= e => (s, e),
                    (Some(s), Some(e)) => (e, s),
                    _ => (0.0, 0.0),
                };
                
                let final_filename = format!("{}_{:.3}_{:.3}.csv", base_filename, start, end);
                let mut final_path = base_dir.clone();
                final_path.push(&final_filename);
                
                // 파일 생성 및 한 번에 쓰기
                let file = File::create(&final_path).map_err(|e| e.to_string())?;
                let mut writer = WriterBuilder::new().with_header(true).build(file);
                
                let mut written_rows = 0;
                for (batch_idx, chunk_batch) in current_chunk_batches.iter().enumerate() {
                    writer.write(chunk_batch).map_err(|e| e.to_string())?;
                    written_rows += chunk_batch.num_rows();
                    
                    // 배치 단위로 진행 상황 업데이트 (너무 자주 업데이트하지 않도록)
                    if batch_idx % 10 == 0 || written_rows >= current_row_count {
                        let temp_processed = processed_rows + written_rows;
                        let progress = 20.0 + ((temp_processed as f64 / total_rows as f64) * 70.0) as f32;
                        let elapsed = export_start.elapsed().as_secs_f64();
                        let speed = if elapsed > 0.0 { temp_processed as f64 / elapsed } else { 0.0 };
                        
                        if let Some(w) = &window {
                            let _ = w.emit("export-progress", ProgressEvent {
                                stage: "writing".to_string(),
                                progress,
                                current: temp_processed as u64,
                                total: total_rows as u64,
                                message: format!("파일 {}/{} 작성 중... ({}/{} rows)", current_chunk_number, total_chunks, written_rows, current_row_count),
                                eta_seconds: if speed > 0.0 { ((total_rows - temp_processed) as f64 / speed) as f32 } else { 0.0 },
                                processing_speed: speed as f32,
                            });
                        }
                    }
                }
                
                writer.close().map_err(|e| e.to_string())?;
                output_paths.push(final_path.to_string_lossy().to_string());
                
                println!("✅ [Export] 파일 생성: {} ({} 행)", final_filename, current_row_count);
                
                // 진행 상태 업데이트 (파일 완료)
                processed_rows += current_row_count;
                let progress = 20.0 + ((processed_rows as f64 / total_rows as f64) * 70.0) as f32;
                let elapsed = export_start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { processed_rows as f64 / elapsed } else { 0.0 };
                let remaining_rows = total_rows - processed_rows;
                let eta = if speed > 0.0 { remaining_rows as f64 / speed } else { 0.0 };
                
                if let Some(w) = &window {
                    let _ = w.emit("export-progress", ProgressEvent {
                        stage: "completed_file".to_string(),
                        progress,
                        current: processed_rows as u64,
                        total: total_rows as u64,
                        message: format!("파일 {}/{} 완료: {} ({} rows)", current_chunk_number, total_chunks, final_filename, current_row_count),
                        eta_seconds: eta as f32,
                        processing_speed: speed as f32,
                    });
                }
                
                // 다음 청크를 위해 초기화
                current_chunk_batches.clear();
                current_row_count = 0;
                chunk_start_time = None;
                chunk_end_time = None;
                current_chunk_number += 1;
            }
        }
    }

    // 마지막 청크 처리
    if !current_chunk_batches.is_empty() {
        // 마지막 청크의 실제 시작/끝 시간을 배치들로부터 다시 계산
        let last_chunk_start = current_chunk_batches.first()
            .and_then(|batch| if batch.num_rows() > 0 { get_time_value(batch, 0) } else { None });

        let last_chunk_end = current_chunk_batches.last()
            .and_then(|batch| {
                let num_rows = batch.num_rows();
                if num_rows > 0 {
                    get_time_value(batch, num_rows - 1)
                } else {
                    None
                }
            });

        // 파일명 생성 (시작 시간이 끝 시간보다 작도록 보장)
        let (start, end) = match (last_chunk_start, last_chunk_end) {
            (Some(s), Some(e)) if s <= e => (s, e),
            (Some(s), Some(e)) => (e, s),
            _ => (0.0, 0.0),
        };

        let final_filename = format!("{}_{:.3}_{:.3}.csv", base_filename, start, end);
        let mut final_path = base_dir.clone();
        final_path.push(&final_filename);

        // 파일 생성 및 한 번에 쓰기
        let file = File::create(&final_path).map_err(|e| e.to_string())?;
        let mut writer = WriterBuilder::new().with_header(true).build(file);

        let mut written_rows = 0;
        let last_chunk_rows: usize = current_chunk_batches.iter().map(|b| b.num_rows()).sum();
        
        for (batch_idx, chunk_batch) in current_chunk_batches.iter().enumerate() {
            writer.write(chunk_batch).map_err(|e| e.to_string())?;
            written_rows += chunk_batch.num_rows();
            
            // 배치 단위로 진행 상황 업데이트
            if batch_idx % 10 == 0 || written_rows >= last_chunk_rows {
                let temp_processed = processed_rows + written_rows;
                let progress = 20.0 + ((temp_processed as f64 / total_rows as f64) * 70.0) as f32;
                let elapsed = export_start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { temp_processed as f64 / elapsed } else { 0.0 };
                
                if let Some(w) = &window {
                    let _ = w.emit("export-progress", ProgressEvent {
                        stage: "writing".to_string(),
                        progress,
                        current: temp_processed as u64,
                        total: total_rows as u64,
                        message: format!("파일 {}/{} 작성 중... ({}/{} rows)", current_chunk_number, total_chunks, written_rows, last_chunk_rows),
                        eta_seconds: if speed > 0.0 { ((total_rows - temp_processed) as f64 / speed) as f32 } else { 0.0 },
                        processing_speed: speed as f32,
                    });
                }
            }
        }

        writer.close().map_err(|e| e.to_string())?;
        output_paths.push(final_path.to_string_lossy().to_string());
        
        println!("✅ [Export] 마지막 파일 생성: {} ({} 행)", final_filename, current_chunk_batches.iter().map(|b| b.num_rows()).sum::<usize>());
    }

    let total_time = start_time.elapsed().as_secs_f64();
    println!("🎉 [Export] CSV 내보내기 완료: {} 파일 생성 ({:.2}초)", output_paths.len(), total_time);
    
    // 완료 상태 업데이트
    if let Some(w) = &window {
        let _ = w.emit("export-progress", ProgressEvent {
            stage: "completed".to_string(),
            progress: 100.0,
            current: total_rows as u64,
            total: total_rows as u64,
            message: format!("완료! {} 파일 생성됨 ({:.2}초)", output_paths.len(), total_time),
            eta_seconds: 0.0,
            processing_speed: (total_rows as f64 / total_time) as f32,
        });
    }

    Ok(output_paths)
}