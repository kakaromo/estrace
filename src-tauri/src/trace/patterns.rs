// src-tauri/src/trace/patterns.rs

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{ACTIVE_BLOCK_PATTERN, ACTIVE_UFS_PATTERN, BLOCK_PATTERNS, UFS_PATTERNS};

#[derive(Debug, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub pattern_type: String,
    pub pattern: String,
    pub is_active: bool,
}

// 정규식 테스트를 위한 구조체들
#[derive(Debug, Serialize, Deserialize)]
pub struct RegexTestResult {
    pub success: bool,
    pub error: Option<String>,
    pub matches: Option<Vec<RegexMatch>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegexMatch {
    pub full_match: String,
    pub captures: Vec<String>,
    pub groups: Option<HashMap<String, String>>,
}

/// Add a new pattern to the appropriate cache
pub fn add_pattern(name: String, pattern_type: String, pattern: String) -> Result<(), String> {
    // Validate pattern by trying to compile it
    let compiled_pattern = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(e) => return Err(format!("Invalid regex pattern: {}", e)),
    };

    match pattern_type.as_str() {
        "ufs" => {
            let mut patterns = UFS_PATTERNS.write().map_err(|e| e.to_string())?;
            patterns.insert(name, compiled_pattern);
        }
        "block" => {
            let mut patterns = BLOCK_PATTERNS.write().map_err(|e| e.to_string())?;
            patterns.insert(name, compiled_pattern);
        }
        _ => return Err(format!("Unsupported pattern type: {}", pattern_type)),
    }

    Ok(())
}

/// Set a pattern as active for a specific type
pub fn set_active_pattern(name: String, pattern_type: String) -> Result<(), String> {
    match pattern_type.as_str() {
        "ufs" => {
            let patterns = UFS_PATTERNS.read().map_err(|e| e.to_string())?;
            let pattern = patterns
                .get(&name)
                .ok_or_else(|| format!("Pattern '{}' not found", name))?;

            let mut active = ACTIVE_UFS_PATTERN.write().map_err(|e| e.to_string())?;
            *active = (name, pattern.clone());
        }
        "block" => {
            let patterns = BLOCK_PATTERNS.read().map_err(|e| e.to_string())?;
            let pattern = patterns
                .get(&name)
                .ok_or_else(|| format!("Pattern '{}' not found", name))?;

            let mut active = ACTIVE_BLOCK_PATTERN.write().map_err(|e| e.to_string())?;
            *active = (name, pattern.clone());
        }
        _ => return Err(format!("Unsupported pattern type: {}", pattern_type)),
    }

    Ok(())
}

/// Get all patterns or patterns of a specific type
pub fn get_patterns(pattern_type: Option<String>) -> Result<String, String> {
    let mut result = Vec::new();

    if let Some(ref pattern_type) = pattern_type {
        match pattern_type.as_str() {
            "ufs" => {
                let patterns = UFS_PATTERNS.read().map_err(|e| e.to_string())?;
                let active = ACTIVE_UFS_PATTERN.read().map_err(|e| e.to_string())?;

                for (name, _) in patterns.iter() {
                    result.push(Pattern {
                        name: name.clone(),
                        pattern_type: "ufs".to_string(),
                        pattern: patterns.get(name).unwrap().to_string(),
                        is_active: name == &active.0,
                    });
                }
            }
            "block" => {
                let patterns = BLOCK_PATTERNS.read().map_err(|e| e.to_string())?;
                let active = ACTIVE_BLOCK_PATTERN.read().map_err(|e| e.to_string())?;

                for (name, _) in patterns.iter() {
                    result.push(Pattern {
                        name: name.clone(),
                        pattern_type: "block".to_string(),
                        pattern: patterns.get(name).unwrap().to_string(),
                        is_active: name == &active.0,
                    });
                }
            }
            _ => return Err(format!("Unsupported pattern type: {}", pattern_type)),
        }
    } else {
        // Both ufs and block patterns
        {
            let patterns = UFS_PATTERNS.read().map_err(|e| e.to_string())?;
            let active = ACTIVE_UFS_PATTERN.read().map_err(|e| e.to_string())?;

            for (name, _) in patterns.iter() {
                result.push(Pattern {
                    name: name.clone(),
                    pattern_type: "ufs".to_string(),
                    pattern: patterns.get(name).unwrap().to_string(),
                    is_active: name == &active.0,
                });
            }
        }

        {
            let patterns = BLOCK_PATTERNS.read().map_err(|e| e.to_string())?;
            let active = ACTIVE_BLOCK_PATTERN.read().map_err(|e| e.to_string())?;

            for (name, _) in patterns.iter() {
                result.push(Pattern {
                    name: name.clone(),
                    pattern_type: "block".to_string(),
                    pattern: patterns.get(name).unwrap().to_string(),
                    is_active: name == &active.0,
                });
            }
        }
    }

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Get active patterns
pub fn get_active_patterns() -> Result<String, String> {
    let mut active_patterns = HashMap::new();

    {
        let active = ACTIVE_UFS_PATTERN.read().map_err(|e| e.to_string())?;
        active_patterns.insert(
            "ufs".to_string(),
            Pattern {
                name: active.0.clone(),
                pattern_type: "ufs".to_string(),
                pattern: active.1.to_string(),
                is_active: true,
            },
        );
    }

    {
        let active = ACTIVE_BLOCK_PATTERN.read().map_err(|e| e.to_string())?;
        active_patterns.insert(
            "block".to_string(),
            Pattern {
                name: active.0.clone(),
                pattern_type: "block".to_string(),
                pattern: active.1.to_string(),
                is_active: true,
            },
        );
    }

    serde_json::to_string(&active_patterns).map_err(|e| e.to_string())
}

/// Delete a pattern
pub fn delete_pattern(name: String, pattern_type: String) -> Result<(), String> {
    match pattern_type.as_str() {
        "ufs" => {
            let mut patterns = UFS_PATTERNS.write().map_err(|e| e.to_string())?;
            let active = ACTIVE_UFS_PATTERN.read().map_err(|e| e.to_string())?;

            // Check if the pattern is active
            if active.0 == name {
                return Err("Cannot delete an active pattern".to_string());
            }

            if patterns.remove(&name).is_none() {
                return Err(format!("Pattern '{}' not found", name));
            }
        }
        "block" => {
            let mut patterns = BLOCK_PATTERNS.write().map_err(|e| e.to_string())?;
            let active = ACTIVE_BLOCK_PATTERN.read().map_err(|e| e.to_string())?;

            // Check if the pattern is active
            if active.0 == name {
                return Err("Cannot delete an active pattern".to_string());
            }

            if patterns.remove(&name).is_none() {
                return Err(format!("Pattern '{}' not found", name));
            }
        }
        _ => return Err(format!("Unsupported pattern type: {}", pattern_type)),
    }

    Ok(())
}

// Initialize the default patterns
pub fn initialize_patterns() {
    // Add default UFS pattern
    let default_ufs_pattern = Regex::new(
        r"^\s*(.*?)\s+\[([0-9]+)\].*?([0-9]+\.[0-9]+):\s+ufshcd_command:\s+(send_req|complete_rsp):.*?tag:\s*(\d+).*?size:\s*([-]?\d+).*?LBA:\s*(\d+).*?opcode:\s*(0x[0-9a-f]+).*?group_id:\s*0x([0-9a-f]+).*?hwq_id:\s*(\d+)"
    ).unwrap();

    let mut ufs_patterns = UFS_PATTERNS.write().unwrap();
    ufs_patterns.insert("Default UFS Pattern".to_string(), default_ufs_pattern);

    // Add default Block pattern
    let default_block_pattern = Regex::new(
        r"^\s*(?P<process>.*?)\s+\[(?P<cpu>\d+)\]\s+(?P<flags>.+?)\s+(?P<time>[\d\.]+):\s+(?P<action>\S+):\s+(?P<devmajor>\d+),(?P<devminor>\d+)\s+(?P<io_type>[A-Z]+)(?:\s+(?P<extra>\d+))?\s+\(\)\s+(?P<sector>\d+)\s+\+\s+(?P<size>\d+)(?:\s+\S+)?\s+\[(?P<comm>.*?)\]$"
    ).unwrap();

    let mut block_patterns = BLOCK_PATTERNS.write().unwrap();
    block_patterns.insert("Default Block Pattern".to_string(), default_block_pattern);
}

/// Test a regex pattern against a text
pub fn test_regex_pattern(text: String, pattern: String) -> Result<String, String> {
    // 정규식 컴파일 시도
    let regex = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(e) => {
            let result = RegexTestResult {
                success: false,
                error: Some(format!("Invalid regex pattern: {}", e)),
                matches: None,
            };
            return serde_json::to_string(&result).map_err(|e| e.to_string());
        }
    };

    // 텍스트 분할 (각 라인별로)
    let lines: Vec<&str> = text.lines().collect();

    // 매치 결과 수집
    let mut matches = Vec::new();

    // 각 라인에 대해 매치 시도
    for line in lines {
        // 빈 라인은 건너뛰기
        if line.trim().is_empty() {
            continue;
        }

        // 라인에 패턴 적용
        if let Some(caps) = regex.captures(line) {
            let mut match_result = RegexMatch {
                full_match: caps.get(0).map_or("", |m| m.as_str()).to_string(),
                captures: Vec::new(),
                groups: None,
            };

            // 캡처 그룹 추출
            for i in 0..caps.len() {
                match_result.captures.push(
                    caps.get(i)
                        .map_or("".to_string(), |m| m.as_str().to_string()),
                );
            }

            // 명명된 그룹이 있는지 확인하고 추출
            let mut named_groups = HashMap::new();
            let has_named_groups = regex.capture_names().any(|name| name.is_some());

            if has_named_groups {
                for name in regex.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        named_groups.insert(name.to_string(), m.as_str().to_string());
                    }
                }
                match_result.groups = Some(named_groups);
            }

            matches.push(match_result);
        }
    }

    // 결과 생성
    let result = RegexTestResult {
        success: true,
        error: None,
        matches: if matches.is_empty() {
            None
        } else {
            Some(matches)
        },
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}
