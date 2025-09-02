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
    // 디버깅을 위한 로그 추가
    println!("filter_ufs_data called with logname: '{}'", logname);
    
    // 캐시에서 데이터 불러오기
    let cached_ufs_list = {
        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        
        // 캐시에 있는 모든 키 출력
        println!("Available UFS cache keys: {:?}", cache.keys().collect::<Vec<_>>());
        
        // logname이 빈 문자열이거나 캐시에서 직접 찾을 수 없는 경우
        let effective_logname = if logname.is_empty() || !cache.contains_key(logname) {
            let cache_keys: Vec<&String> = cache.keys().collect();
            
            if logname.is_empty() {
                // 빈 문자열인 경우, UFS 파일을 찾아서 사용
                if let Some(key) = cache_keys.iter().find(|k| k.contains("_ufs.parquet")) {
                    println!("Using UFS cache key for empty logname: {}", key);
                    key.as_str()
                } else {
                    println!("No UFS cache found for empty logname");
                    return Err("UFS Cache not found".to_string());
                }
            } else {
                // logname이 있지만 정확히 매칭되지 않는 경우, 부분 매칭 시도
                if let Some(key) = cache_keys.iter().find(|k| k.ends_with(logname) || k.contains(logname)) {
                    println!("Using UFS cache key for partial match '{}': {}", logname, key);
                    key.as_str()
                } else if let Some(key) = cache_keys.iter().find(|k| k.contains("_ufs.parquet")) {
                    println!("Using fallback UFS cache key for '{}': {}", logname, key);
                    key.as_str()
                } else {
                    println!("No matching UFS cache found for logname: {}", logname);
                    return Err("UFS Cache not found".to_string());
                }
            }
        } else {
            logname
        };
        
        println!("Using effective UFS logname: '{}'", effective_logname);
        
        cache.get(effective_logname).ok_or("UFS Cache not found")?.clone()
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
                        "qd" => ufs.qd as f64,
                        "cpu" => ufs.cpu as f64,
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
    // 디버깅을 위한 로그 추가
    println!("filter_block_data called with logname: '{}'", logname);
    
    // 캐시에서 데이터 불러오기
    let cached_block_list = {
        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        
        // 캐시에 있는 모든 키 출력
        println!("Available cache keys: {:?}", cache.keys().collect::<Vec<_>>());
        
        // logname이 빈 문자열이거나 캐시에서 직접 찾을 수 없는 경우
        let effective_logname = if logname.is_empty() || !cache.contains_key(logname) {
            let cache_keys: Vec<&String> = cache.keys().collect();
            
            if logname.is_empty() {
                // 빈 문자열인 경우, block 파일을 찾아서 사용
                if let Some(key) = cache_keys.iter().find(|k| k.contains("_block.parquet")) {
                    println!("Using cache key for empty logname: {}", key);
                    key.as_str()
                } else {
                    println!("No block cache found for empty logname");
                    return Err("Block Cache not found".to_string());
                }
            } else {
                // logname이 있지만 정확히 매칭되지 않는 경우, 부분 매칭 시도
                if let Some(key) = cache_keys.iter().find(|k| k.ends_with(logname) || k.contains(logname)) {
                    println!("Using cache key for partial match '{}': {}", logname, key);
                    key.as_str()
                } else if let Some(key) = cache_keys.iter().find(|k| k.contains("_block.parquet")) {
                    println!("Using fallback block cache key for '{}': {}", logname, key);
                    key.as_str()
                } else {
                    println!("No matching block cache found for logname: {}", logname);
                    return Err("Block Cache not found".to_string());
                }
            }
        } else {
            logname
        };
        
        println!("Using effective logname: '{}'", effective_logname);
        
        cache.get(effective_logname).ok_or("Block Cache not found")?.clone()
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
                        "sector" => block.sector as f64,
                        "dtoc" => block.dtoc,
                        "ctoc" => block.ctoc,
                        "ctod" => block.ctod,
                        "qd" => block.qd as f64,
                        "cpu" => block.cpu as f64,
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
