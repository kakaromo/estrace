use std::fs::File;
use std::path::PathBuf;

use arrow::array::{RecordBatchWriter, Float64Array, Array};
use arrow::datatypes::DataType;
use datafusion::arrow::csv::{Writer, WriterBuilder};
use datafusion::prelude::*; // RecordBatchWriter 트레이트 추가

// Excel의 최대 행 수 (헤더 제외)
const EXCEL_MAX_ROWS: usize = 1_048_575;

// CSV 내보내기 공통 함수
pub async fn export_to_csv(
    parquet_path: String,
    output_dir: Option<String>,
) -> Result<Vec<String>, String> {
    // DataFusion 세션 초기화
    let ctx = SessionContext::new();

    // Parquet 파일 읽기
    let df = ctx
        .read_parquet(parquet_path.as_str(), ParquetReadOptions::default())
        .await
        .map_err(|e| e.to_string())?;

    // 데이터프레임에서 레코드 배치 가져오기
    let batches = df.collect().await.map_err(|e| e.to_string())?;

    // 총 행 수 계산 (로깅용)
    let _total_rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();

    // 스키마에서 시간 컬럼 이름 결정 (start_time 또는 time)
    let time_column = if batches.is_empty() {
        "time"
    } else {
        let schema = batches[0].schema();
        if schema.column_with_name("start_time").is_some() {
            "start_time"
        } else {
            "time"
        }
    };

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
    let mut current_row_count = 0;
    let mut file_index = 0;
    let mut current_writer: Option<Writer<File>> = None;
    let mut chunk_start_time: Option<f64> = None;
    let mut chunk_end_time: Option<f64> = None;

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

    // 임시 파일 경로를 저장할 벡터
    let mut temp_files: Vec<(PathBuf, f64, f64)> = Vec::new();

    // 각 배치를 처리하면서 파일 분할
    for batch in batches {
        let batch_rows = batch.num_rows();
        let mut batch_offset = 0;

        while batch_offset < batch_rows {
            // 새 파일이 필요한 경우
            if current_writer.is_none() || current_row_count >= EXCEL_MAX_ROWS {
                // 기존 파일 닫고 최종 파일명으로 변경
                if let Some(writer) = current_writer.take() {
                    writer.close().map_err(|e| e.to_string())?;
                    
                    // 이전 청크의 정보를 저장
                    if let (Some(start), Some(end)) = (chunk_start_time, chunk_end_time) {
                        if let Some(last_temp_path) = temp_files.last_mut() {
                            last_temp_path.1 = start;
                            last_temp_path.2 = end;
                        }
                    }
                }

                // 임시 파일 경로 생성
                let temp_filename = format!("{}_temp_{}.csv", base_filename, file_index);
                let mut temp_path = base_dir.clone();
                temp_path.push(&temp_filename);
                
                // 새 파일 생성
                let file = File::create(&temp_path).map_err(|e| e.to_string())?;
                let writer = WriterBuilder::new().with_header(true).build(file);
                
                current_writer = Some(writer);
                temp_files.push((temp_path, 0.0, 0.0)); // 시간은 나중에 설정
                current_row_count = 0;
                chunk_start_time = None;
                chunk_end_time = None;
                file_index += 1;
            }

            // 현재 파일에 쓸 수 있는 최대 행 수 계산
            let remaining_capacity = EXCEL_MAX_ROWS - current_row_count;
            let rows_to_write = std::cmp::min(remaining_capacity, batch_rows - batch_offset);

            // 배치에서 필요한 부분만 슬라이스
            let slice_batch = if rows_to_write == batch_rows && batch_offset == 0 {
                batch.clone()
            } else {
                batch.slice(batch_offset, rows_to_write)
            };

            // 청크의 시작 시간 설정
            if chunk_start_time.is_none() {
                chunk_start_time = get_time_value(&slice_batch, 0);
            }
            
            // 청크의 끝 시간 갱신
            if slice_batch.num_rows() > 0 {
                chunk_end_time = get_time_value(&slice_batch, slice_batch.num_rows() - 1);
            }

            // CSV에 쓰기
            if let Some(ref mut writer) = current_writer {
                writer.write(&slice_batch).map_err(|e| e.to_string())?;
            }

            current_row_count += rows_to_write;
            batch_offset += rows_to_write;
        }
    }

    // 마지막 파일 닫기
    if let Some(writer) = current_writer.take() {
        writer.close().map_err(|e| e.to_string())?;
        
        // 마지막 청크의 정보를 저장
        if let (Some(start), Some(end)) = (chunk_start_time, chunk_end_time) {
            if let Some(last_temp_path) = temp_files.last_mut() {
                last_temp_path.1 = start;
                last_temp_path.2 = end;
            }
        }
    }

    // 임시 파일들을 최종 이름으로 변경
    for (temp_path, start_time, end_time) in temp_files {
        let final_filename = format!("{}_{:.3}_{:.3}.csv", base_filename, start_time, end_time);
        let mut final_path = base_dir.clone();
        final_path.push(&final_filename);
        
        std::fs::rename(&temp_path, &final_path).map_err(|e| e.to_string())?;
        output_paths.push(final_path.to_string_lossy().to_string());
    }

    Ok(output_paths)
}
