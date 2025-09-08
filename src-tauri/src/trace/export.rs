use std::fs::File;
use std::path::PathBuf;

use arrow::array::RecordBatchWriter;
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

    // 각 배치를 처리하면서 파일 분할
    for batch in batches {
        let batch_rows = batch.num_rows();
        let mut batch_offset = 0;

        while batch_offset < batch_rows {
            // 새 파일이 필요한 경우
            if current_writer.is_none() || current_row_count >= EXCEL_MAX_ROWS {
                // 기존 파일 닫기
                if let Some(writer) = current_writer.take() {
                    writer.close().map_err(|e| e.to_string())?;
                }

                // 새 파일 경로 생성
                let output_filename = if file_index == 0 {
                    format!("{}.csv", base_filename)
                } else {
                    format!("{}_{}.csv", base_filename, file_index + 1)
                };
                
                let mut output_path = base_dir.clone();
                output_path.push(&output_filename);
                
                // 새 파일 생성
                let file = File::create(&output_path).map_err(|e| e.to_string())?;
                let writer = WriterBuilder::new().with_header(true).build(file);
                
                current_writer = Some(writer);
                output_paths.push(output_path.to_string_lossy().to_string());
                current_row_count = 0;
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
    }

    Ok(output_paths)
}
