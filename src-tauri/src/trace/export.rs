use std::fs::File;
use std::path::PathBuf;
use std::time::Instant;

use datafusion::prelude::*;
use datafusion::arrow::csv::WriterBuilder;
use arrow::array::RecordBatchWriter; // RecordBatchWriter 트레이트 추가

// CSV 내보내기 공통 함수
pub async fn export_to_csv(
    parquet_path: String, 
    file_type: String,
    output_dir: Option<String>
) -> Result<String, String> {
    // 출력 경로 설정
    let output_path = if let Some(dir) = output_dir {
        let mut path = PathBuf::from(dir);
        let input_path = PathBuf::from(&parquet_path);
        let filename = input_path.file_stem().ok_or("Invalid parquet path")?.to_string_lossy();
        path.push(format!("{}.csv", filename));
        path
    } else {
        let input_path = PathBuf::from(&parquet_path);
        let parent = input_path.parent().ok_or("Invalid parquet path")?;
        let filename = input_path.file_stem().ok_or("Invalid parquet path")?.to_string_lossy();
        let mut path = PathBuf::from(parent);
        path.push(format!("{}.csv", filename));
        path
    };

    let start_time = Instant::now();
    println!("Starting CSV export for {}: {}", file_type, parquet_path);

    // DataFusion 세션 초기화
    let ctx = SessionContext::new();
    
    // Parquet 파일 읽기
    let df = ctx
        .read_parquet(
            parquet_path.as_str(),
            ParquetReadOptions::default(),
        )
        .await
        .map_err(|e| e.to_string())?;

    let read_time = start_time.elapsed();
    println!("Parquet read time: {:?}", read_time);

    // 데이터프레임에서 레코드 배치 가져오기
    let batches = df.collect().await.map_err(|e| e.to_string())?;
    
    // CSV 파일 생성
    let file = File::create(&output_path).map_err(|e| e.to_string())?;
    let mut writer = WriterBuilder::new()
        .with_header(true)
        .build(file);
    
    // 각 배치를 CSV로 저장
    let batch_write_start = Instant::now();
    for batch in batches {
        writer.write(&batch).map_err(|e| e.to_string())?;
    }
    let batch_write_time = batch_write_start.elapsed();
    println!("Batch write time: {:?}", batch_write_time);
    
    // 파일 닫기
    writer.close().map_err(|e| e.to_string())?;
    
    let total_time = start_time.elapsed();
    println!("Total CSV export time: {:?}", total_time);
    
    Ok(output_path.to_string_lossy().to_string())
}
