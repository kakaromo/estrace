use datafusion::prelude::*;
use arrow::array::*;

/// WebGL용 전처리된 데이터 구조
#[derive(serde::Serialize)]
pub struct WebGLChartData {
    /// Float32 형식의 위치 데이터 [x1, y1, x2, y2, ...]
    pub positions: Vec<u8>,  // Float32Array 바이트
    /// 각 포인트의 색상 인덱스 (legend용)
    pub color_indices: Vec<u8>,  // Uint8Array
    /// Legend 정보
    pub legends: Vec<String>,
    /// 데이터 bounds
    pub bounds: DataBounds,
    /// 총 포인트 수
    pub point_count: usize,
}

#[derive(serde::Serialize)]
pub struct DataBounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

/// Parquet에서 WebGL 최적화 데이터 추출
pub async fn prepare_webgl_data(
    parquet_path: String,
    x_column: String,
    y_column: String,
    legend_column: String,
    x_filter: Option<(f64, f64)>,  // (min, max)
    y_filter: Option<(f64, f64)>,
) -> Result<WebGLChartData, String> {
    let ctx = SessionContext::new();
    
    // Parquet 파일 읽기
    let mut df = ctx
        .read_parquet(&parquet_path, ParquetReadOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    
    // 필터 적용
    if let Some((x_min, x_max)) = x_filter {
        df = df
            .filter(col(&x_column).gt_eq(lit(x_min)).and(col(&x_column).lt_eq(lit(x_max))))
            .map_err(|e| e.to_string())?;
    }
    
    if let Some((y_min, y_max)) = y_filter {
        df = df
            .filter(col(&y_column).gt_eq(lit(y_min)).and(col(&y_column).lt_eq(lit(y_max))))
            .map_err(|e| e.to_string())?;
    }
    
    // 필요한 컬럼만 선택
    df = df
        .select_columns(&[&x_column, &y_column, &legend_column])
        .map_err(|e| e.to_string())?;
    
    // 데이터 수집
    let batches = df.collect().await.map_err(|e| e.to_string())?;
    
    let mut positions_f32 = Vec::new();
    let mut color_indices_u8 = Vec::new();
    let mut legend_map = std::collections::HashMap::new();
    let mut legend_list = Vec::new();
    
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    
    // 각 배치 처리
    for batch in batches {
        let x_array = batch
            .column_by_name(&x_column)
            .ok_or("X column not found")?;
        let y_array = batch
            .column_by_name(&y_column)
            .ok_or("Y column not found")?;
        let legend_array = batch
            .column_by_name(&legend_column)
            .ok_or("Legend column not found")?;
        
        let row_count = batch.num_rows();
        
        for i in 0..row_count {
            // X 값 추출 (다양한 타입 지원)
            let x_val = match x_array.data_type() {
                arrow::datatypes::DataType::Float64 => {
                    x_array.as_any().downcast_ref::<Float64Array>()
                        .unwrap().value(i)
                }
                arrow::datatypes::DataType::Float32 => {
                    x_array.as_any().downcast_ref::<Float32Array>()
                        .unwrap().value(i) as f64
                }
                arrow::datatypes::DataType::Int64 => {
                    x_array.as_any().downcast_ref::<Int64Array>()
                        .unwrap().value(i) as f64
                }
                _ => return Err(format!("Unsupported X column type: {:?}", x_array.data_type())),
            };
            
            // Y 값 추출
            let y_val = match y_array.data_type() {
                arrow::datatypes::DataType::Float64 => {
                    y_array.as_any().downcast_ref::<Float64Array>()
                        .unwrap().value(i)
                }
                arrow::datatypes::DataType::Float32 => {
                    y_array.as_any().downcast_ref::<Float32Array>()
                        .unwrap().value(i) as f64
                }
                arrow::datatypes::DataType::Int64 => {
                    y_array.as_any().downcast_ref::<Int64Array>()
                        .unwrap().value(i) as f64
                }
                arrow::datatypes::DataType::UInt64 => {
                    y_array.as_any().downcast_ref::<UInt64Array>()
                        .unwrap().value(i) as f64
                }
                _ => return Err(format!("Unsupported Y column type: {:?}", y_array.data_type())),
            };
            
            // Legend 값 추출
            let legend_val = match legend_array.data_type() {
                arrow::datatypes::DataType::Utf8 => {
                    legend_array.as_any().downcast_ref::<StringArray>()
                        .unwrap().value(i).to_string()
                }
                arrow::datatypes::DataType::Int32 => {
                    legend_array.as_any().downcast_ref::<Int32Array>()
                        .unwrap().value(i).to_string()
                }
                arrow::datatypes::DataType::Int64 => {
                    legend_array.as_any().downcast_ref::<Int64Array>()
                        .unwrap().value(i).to_string()
                }
                _ => return Err(format!("Unsupported legend column type: {:?}", legend_array.data_type())),
            };
            
            // Bounds 업데이트
            x_min = x_min.min(x_val);
            x_max = x_max.max(x_val);
            y_min = y_min.min(y_val);
            y_max = y_max.max(y_val);
            
            // Position 데이터 추가 (Float32)
            positions_f32.push(x_val as f32);
            positions_f32.push(y_val as f32);
            
            // Legend 인덱스 추가
            let legend_idx = *legend_map.entry(legend_val.clone()).or_insert_with(|| {
                let idx = legend_list.len();
                legend_list.push(legend_val);
                idx
            });
            color_indices_u8.push(legend_idx as u8);
        }
    }
    
    // Float32Array를 바이트로 변환
    let positions_bytes: Vec<u8> = positions_f32
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    
    Ok(WebGLChartData {
        positions: positions_bytes,
        color_indices: color_indices_u8,
        legends: legend_list,
        bounds: DataBounds {
            x_min,
            x_max,
            y_min,
            y_max,
        },
        point_count: positions_f32.len() / 2,
    })
}
