use crate::trace::{Block, BLOCK_CACHE, UFS, UFS_CACHE};

// 공통 필터링 로직 구현
// UFS 데이터 필터링 함수
pub fn filter_ufs_data(
    logname: &str,
    time_from: Option<f64>,
    time_to: Option<f64>,
    zoom_column: &str,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<UFS>, String> {
    // 캐시에서 데이터 불러오기
    let cached_ufs_list = {
        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(logname).ok_or("UFS Cache not found")?.clone()
    };

    // 시간 필터링
    let time_filtered: Vec<UFS> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        if t_from == 0.0 && t_to == 0.0 {
            cached_ufs_list
        } else {
            cached_ufs_list
                .into_iter()
                .filter(|ufs| ufs.time >= t_from && ufs.time <= t_to)
                .collect()
        }
    } else {
        cached_ufs_list
    };

    // 추가 필드 기반 필터링
    let filtered = if let (Some(v_from), Some(v_to)) = (col_from, col_to) {
        if v_from == 0.0 && v_to == 0.0 {
            time_filtered
        } else {
            time_filtered
                .into_iter()
                .filter(|ufs| {
                    let value = match zoom_column {
                        "lba" => ufs.lba as f64,
                        "dtoc" => ufs.dtoc,
                        "ctoc" => ufs.ctoc,
                        "ctod" => ufs.ctod,
                        "cpu" => ufs.cpu.into(),
                        _ => return false, // 지원하지 않는 컬럼
                    };
                    value >= v_from && value <= v_to
                })
                .collect()
        }
    } else {
        time_filtered
    };

    Ok(filtered)
}

// Block 데이터 필터링 함수
pub fn filter_block_data(
    logname: &str,
    time_from: Option<f64>,
    time_to: Option<f64>,
    zoom_column: &str,
    col_from: Option<f64>,
    col_to: Option<f64>,
) -> Result<Vec<Block>, String> {
    // 캐시에서 데이터 불러오기
    let cached_block_list = {
        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        cache.get(logname).ok_or("Block Cache not found")?.clone()
    };

    // 시간 필터링
    let time_filtered: Vec<Block> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        if t_from == 0.0 && t_to == 0.0 {
            cached_block_list
        } else {
            cached_block_list
                .into_iter()
                .filter(|block| block.time >= t_from && block.time <= t_to)
                .collect()
        }
    } else {
        cached_block_list
    };

    // 추가 필드 기반 필터링
    let filtered = if let (Some(v_from), Some(v_to)) = (col_from, col_to) {
        if v_from == 0.0 && v_to == 0.0 {
            time_filtered
        } else {
            time_filtered
                .into_iter()
                .filter(|block| {
                    let value: f64 = match zoom_column {
                        "lba" => block.sector as f64,
                        "dtoc" => block.dtoc,
                        "ctoc" => block.ctoc,
                        "ctod" => block.ctod,
                        "cpu" => block.cpu.into(),
                        _ => return false, // 지원하지 않는 컬럼
                    };
                    value >= v_from && value <= v_to
                })
                .collect()
        }
    } else {
        time_filtered
    };

    Ok(filtered)
}
