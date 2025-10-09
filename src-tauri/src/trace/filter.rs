use crate::trace::{Block, BLOCK_CACHE, UFS, UFS_CACHE};
use rayon::prelude::*;

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
    println!("🎯 [DEBUG] filter_ufs_data 호출: logname='{}'", logname);
    
    // 캐시에서 데이터 불러오기 (원본 데이터 우선)
    let cached_ufs_list = {
        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
        
        // 디버깅: 캐시에 있는 모든 키 출력
        let available_keys: Vec<String> = cache.keys().cloned().collect();
        println!("🔍 [DEBUG] 캐시에 있는 UFS 키들: {:?}", available_keys);
        
        // 1. 먼저 정확한 키로 시도
        if let Some(data) = cache.get(logname) {
            println!("🎯 [DEBUG] 정확한 키 '{}' 매치: {} 개 레코드", logname, data.len());
            data.clone()
        }
        // 2. 개별 파일 키가 없다면, 복합 키에서 찾기
        else {
            let mut found_data: Option<Vec<UFS>> = None;
            
            // 모든 캐시 키를 확인하여 복합 키 찾기
            for (cache_key, data) in cache.iter() {
                // 콤마로 구분된 복합 키인지 확인
                if cache_key.contains(',') {
                    let files: Vec<&str> = cache_key.split(',').map(|s| s.trim()).collect();
                    // logname이 복합 키의 일부인지 확인
                    if files.iter().any(|&file| file == logname) {
                        println!("🎯 [DEBUG] 복합 키 '{}' 에서 '{}' 찾음: {} 개 레코드", cache_key, logname, data.len());
                        found_data = Some(data.clone());
                        break;
                    }
                }
            }
            
            // 3. 캐시에 없으면 자동으로 readtrace 호출하여 데이터 로드
            if found_data.is_none() {
                drop(cache); // 락 해제
                println!("⚡ [DEBUG] UFS 캐시 없음, 자동 로드 시도: '{}'", logname);
                
                // readtrace 호출로 데이터 로드 및 캐시 저장
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                match rt.block_on(crate::trace::utils::readtrace(logname.to_string(), 1000000)) {
                    Ok(_) => {
                        println!("✅ [DEBUG] 자동 readtrace 완료");
                        // 다시 캐시에서 시도
                        let cache = UFS_CACHE.lock().map_err(|e| e.to_string())?;
                        if let Some(data) = cache.get(logname) {
                            println!("✅ [DEBUG] 자동 로드 성공: '{}' -> {} 개 레코드", logname, data.len());
                            data.clone()
                        } else {
                            return Err(format!("자동 로드 후에도 UFS Cache not found for key '{}'", logname));
                        }
                    }
                    Err(e) => {
                        return Err(format!("UFS 파일 자동 로드 실패: {}", e));
                    }
                }
            } else {
                found_data.unwrap()
            }
        }
    };

    // ⚡ 병렬 시간 필터링 (데이터 크기에 따라 병렬/순차 선택)
    let data_size = cached_ufs_list.len();
    let use_parallel = data_size > 10000; // 10K 이상일 때만 병렬 처리
    
    let time_filtered: Vec<UFS> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        if t_from == 0.0 && t_to == 0.0 {
            cached_ufs_list
        } else {
            if use_parallel {
                println!("⚡ [Performance] UFS 병렬 시간 필터링: {} 레코드", data_size);
                cached_ufs_list
                    .into_par_iter()
                    .filter(|ufs| ufs.time >= t_from && ufs.time <= t_to)
                    .collect()
            } else {
                cached_ufs_list
                    .into_iter()
                    .filter(|ufs| ufs.time >= t_from && ufs.time <= t_to)
                    .collect()
            }
        }
    } else {
        cached_ufs_list
    };

    // ⚡ 병렬 필드 필터링
    let filtered = if let (Some(v_from), Some(v_to)) = (col_from, col_to) {
        if v_from == 0.0 && v_to == 0.0 {
            time_filtered
        } else {
            let filtered_size = time_filtered.len();
            if use_parallel && filtered_size > 10000 {
                println!("⚡ [Performance] UFS 병렬 필드 필터링 ({}): {} 레코드", zoom_column, filtered_size);
                time_filtered
                    .into_par_iter()
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
        }
    } else {
        time_filtered
    };

    println!("✅ [Performance] UFS 필터링 완료: {} -> {} 레코드", data_size, filtered.len());
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
    println!("🎯 [DEBUG] filter_block_data 호출: logname='{}'", logname);
    
    // 캐시에서 데이터 불러오기 (원본 데이터 우선)
    let cached_block_list = {
        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
        
        // 디버깅: 캐시에 있는 모든 키 출력
        let available_keys: Vec<String> = cache.keys().cloned().collect();
        println!("🔍 [DEBUG] 캐시에 있는 Block 키들: {:?}", available_keys);
        
        // 1. 먼저 정확한 키로 시도
        if let Some(data) = cache.get(logname) {
            println!("🎯 [DEBUG] 정확한 키 '{}' 매치: {} 개 레코드", logname, data.len());
            data.clone()
        }
        // 2. 개별 파일 키가 없다면, 복합 키에서 찾기
        else {
            let mut found_data: Option<Vec<Block>> = None;
            
            // 모든 캐시 키를 확인하여 복합 키 찾기
            for (cache_key, data) in cache.iter() {
                // 콤마로 구분된 복합 키인지 확인
                if cache_key.contains(',') {
                    let files: Vec<&str> = cache_key.split(',').map(|s| s.trim()).collect();
                    // logname이 복합 키의 일부인지 확인
                    if files.iter().any(|&file| file == logname) {
                        println!("🎯 [DEBUG] 복합 키 '{}' 에서 '{}' 찾음: {} 개 레코드", cache_key, logname, data.len());
                        found_data = Some(data.clone());
                        break;
                    }
                }
            }
            
            // 3. 캐시에 없으면 자동으로 readtrace 호출하여 데이터 로드  
            if found_data.is_none() {
                drop(cache); // 락 해제
                println!("⚡ [DEBUG] Block 캐시 없음, 자동 로드 시도: '{}'", logname);
                
                // readtrace 호출로 데이터 로드 및 캐시 저장
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                match rt.block_on(crate::trace::utils::readtrace(logname.to_string(), 1000000)) {
                    Ok(_) => {
                        println!("✅ [DEBUG] 자동 readtrace 완료");
                        // 다시 캐시에서 시도
                        let cache = BLOCK_CACHE.lock().map_err(|e| e.to_string())?;
                        if let Some(data) = cache.get(logname) {
                            println!("✅ [DEBUG] 자동 로드 성공: '{}' -> {} 개 레코드", logname, data.len());
                            data.clone()
                        } else {
                            return Err(format!("자동 로드 후에도 Block Cache not found for key '{}'", logname));
                        }
                    }
                    Err(e) => {
                        return Err(format!("Block 파일 자동 로드 실패: {}", e));
                    }
                }
            } else {
                found_data.unwrap()
            }
        }
    };

    // ⚡ 병렬 시간 필터링 (데이터 크기에 따라 병렬/순차 선택)
    let data_size = cached_block_list.len();
    let use_parallel = data_size > 10000; // 10K 이상일 때만 병렬 처리
    
    let time_filtered: Vec<Block> = if let (Some(t_from), Some(t_to)) = (time_from, time_to) {
        if t_from == 0.0 && t_to == 0.0 {
            cached_block_list
        } else {
            if use_parallel {
                println!("⚡ [Performance] Block 병렬 시간 필터링: {} 레코드", data_size);
                cached_block_list
                    .into_par_iter()
                    .filter(|block| block.time >= t_from && block.time <= t_to)
                    .collect()
            } else {
                cached_block_list
                    .into_iter()
                    .filter(|block| block.time >= t_from && block.time <= t_to)
                    .collect()
            }
        }
    } else {
        cached_block_list
    };

    // ⚡ 병렬 필드 필터링
    let filtered = if let (Some(v_from), Some(v_to)) = (col_from, col_to) {
        if v_from == 0.0 && v_to == 0.0 {
            time_filtered
        } else {
            let filtered_size = time_filtered.len();
            if use_parallel && filtered_size > 10000 {
                println!("⚡ [Performance] Block 병렬 필드 필터링 ({}): {} 레코드", zoom_column, filtered_size);
                time_filtered
                    .into_par_iter()
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
        }
    } else {
        time_filtered
    };

    println!("✅ [Performance] Block 필터링 완료: {} -> {} 레코드", data_size, filtered.len());
    Ok(filtered)
}
