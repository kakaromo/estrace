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

    // 스키마에서 시간 컬럼 이름 결정 (start_time 또는 time)
    let schema = df.schema();
    let time_column = if schema.fields().iter().any(|f| f.name() == "start_time") {
        "start_time"
    } else {
        "time"
    };

    // 시간 컬럼으로 정렬 (ufscustom은 start_time, 나머지는 time으로 정렬)
    let sorted_df = df
        .sort(vec![col(time_column).sort(true, true)])
        .map_err(|e| e.to_string())?;

    // 데이터프레임에서 레코드 배치 가져오기
    let batches = sorted_df.collect().await.map_err(|e| e.to_string())?;

    // 총 행 수 계산 (로깅용)
    let _total_rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();

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
                
                for chunk_batch in &current_chunk_batches {
                    writer.write(chunk_batch).map_err(|e| e.to_string())?;
                }
                
                writer.close().map_err(|e| e.to_string())?;
                output_paths.push(final_path.to_string_lossy().to_string());
                
                // 다음 청크를 위해 초기화
                current_chunk_batches.clear();
                current_row_count = 0;
                chunk_start_time = None;
                chunk_end_time = None;
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

        for chunk_batch in &current_chunk_batches {
            writer.write(chunk_batch).map_err(|e| e.to_string())?;
        }

        writer.close().map_err(|e| e.to_string())?;
        output_paths.push(final_path.to_string_lossy().to_string());
    }

    Ok(output_paths)
}