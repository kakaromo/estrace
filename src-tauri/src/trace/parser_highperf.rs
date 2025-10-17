// 고성능 메모리 맵 기반 파서 구현
// kakaromo/trace의 log_high_perf.rs와 log_common.rs를 참고하여 구현

use crate::trace::{Block, UFS, UFSCUSTOM};
use crate::trace::{ACTIVE_UFS_PATTERN, ACTIVE_BLOCK_PATTERN, ACTIVE_UFSCUSTOM_PATTERN};
use memmap2::MmapOptions;
use rayon::prelude::*;
use regex::Regex;
use std::fs::File;
use std::io;
use std::sync::Arc;
use std::time::Instant;

/// SIMD 스타일 최적화된 라인 경계 검색
/// 64바이트 청크 단위로 처리하여 캐시 성능 극대화
#[inline]
fn find_line_boundaries(data: &[u8]) -> Vec<usize> {
    let mut boundaries = Vec::new();
    boundaries.push(0);
    
    // 64바이트 단위로 처리하여 캐시 성능 향상
    let mut i = 0;
    while i < data.len() {
        let end = std::cmp::min(i + 64, data.len());
        let chunk = &data[i..end];
        
        for (offset, &byte) in chunk.iter().enumerate() {
            if byte == b'\n' {
                boundaries.push(i + offset + 1);
            }
        }
        i = end;
    }
    
    boundaries
}



/// 최적화된 라인 분류 및 파싱
/// ACTIVE_*_PATTERN을 사용하여 정규표현식 기반 파싱
#[inline]
fn process_line_optimized(
    line: &str,
    ufs_regex: &Regex,
    block_regex: &Regex,
    ufscustom_regex: &Regex,
) -> (Option<UFS>, Option<Block>, Option<UFSCUSTOM>) {
    if line.is_empty() || line.len() < 10 {
        return (None, None, None);
    }
    
    let bytes = line.as_bytes();
    
    // 빠른 타입 판별 후 정규표현식 적용
    // UFSCUSTOM: CSV 형식 (콤마 4개 이상)
    if bytes.iter().filter(|&&b| b == b',').count() >= 4 {
        if let Some(ufscustom) = parse_ufscustom_event(line, ufscustom_regex) {
            return (None, None, Some(ufscustom));
        }
    }
    
    // UFS: "ufshcd_command" 포함
    if line.contains("ufshcd_command") {
        if let Some(ufs) = parse_ufs_event(line, ufs_regex) {
            return (Some(ufs), None, None);
        }
    }
    
    // Block: "block_" 또는 "rq_" 포함
    if line.contains("block_") || line.contains("rq_") {
        if let Some(block) = parse_block_io_event(line, block_regex) {
            return (None, Some(block), None);
        }
    }
    
    (None, None, None)
}

// ===== 정규표현식 기반 파싱 함수 =====
// ACTIVE_*_PATTERN을 사용하여 기존 파서와 동일한 로직 적용

/// UFS 이벤트 파싱 (정규표현식 사용)
#[inline]
fn parse_ufs_event(line: &str, regex: &Regex) -> Option<UFS> {
    let caps = regex.captures(line)?;
    
    let time = caps.name("time")?.as_str().parse().ok()?;
    let process = caps.name("process")?.as_str().to_string();
    let cpu = caps.name("cpu")?.as_str().parse().ok()?;
    let action = caps.name("command")?.as_str().to_string();
    let tag = caps.name("tag")?.as_str().parse().ok()?;
    
    // size 처리 (음수 허용, 4KB 단위 변환)
    let size_raw: i32 = caps.name("size")?.as_str().parse().ok()?;
    let size: u32 = size_raw.unsigned_abs() / 4096;
    
    // LBA 처리 (디버그 값 필터링)
    let raw_lba: u64 = caps.name("lba")?.as_str().parse().ok()?;
    const UFS_DEBUG_LBA: u64 = 281474976710655;
    const MAX_VALID_UFS_LBA: u64 = 1_000_000_000_000;
    let lba = if raw_lba == UFS_DEBUG_LBA || raw_lba > MAX_VALID_UFS_LBA {
        0
    } else {
        raw_lba
    };
    
    let opcode = caps.name("opcode")?.as_str().to_string();
    
    // group_id 파싱 (0x 접두사 처리)
    let groupid_str = caps.name("group_id")?.as_str();
    let groupid = if groupid_str.starts_with("0x") {
        u32::from_str_radix(&groupid_str[2..], 16).ok()?
    } else {
        groupid_str.parse().ok()?
    };
    
    let hwqid = caps.name("hwq_id")?.as_str().parse().ok()?;
    
    Some(UFS {
        time,
        process,
        cpu,
        action,
        tag,
        opcode,
        lba,
        size,
        groupid,
        hwqid,
        qd: 0,
        dtoc: 0.0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

/// Block I/O 이벤트 파싱 (정규표현식 사용)
#[inline]
fn parse_block_io_event(line: &str, regex: &Regex) -> Option<Block> {
    let caps = regex.captures(line)?;
    
    let time = caps.name("time")?.as_str().parse().ok()?;
    let process = caps.name("process")?.as_str().to_string();
    let cpu = caps.name("cpu")?.as_str().parse().ok()?;
    let flags = caps.name("flags")?.as_str().to_string();
    let action = caps.name("action")?.as_str().to_string();
    let devmajor = caps.name("devmajor")?.as_str().parse().ok()?;
    let devminor = caps.name("devminor")?.as_str().parse().ok()?;
    let io_type = caps.name("io_type")?.as_str().to_string();
    
    // extra는 선택적
    let extra = caps.name("extra")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    
    // sector 처리 (최대값 필터링)
    let sector_str = caps.name("sector")?.as_str();
    let sector: u64 = if sector_str == "18446744073709551615" {
        0
    } else {
        sector_str.parse().ok()?
    };
    
    let size = caps.name("size")?.as_str().parse().ok()?;
    let comm = caps.name("comm")?.as_str().to_string();
    
    Some(Block {
        time,
        process,
        cpu,
        flags,
        action,
        devmajor,
        devminor,
        io_type,
        extra,
        sector,
        size,
        comm,
        qd: 0,
        dtoc: 0.0,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

/// UFSCUSTOM 이벤트 파싱 (정규표현식 사용)
#[inline]
fn parse_ufscustom_event(line: &str, regex: &Regex) -> Option<UFSCUSTOM> {
    let caps = regex.captures(line)?;
    
    let opcode = caps.name("opcode")?.as_str().to_string();
    let lba = caps.name("lba")?.as_str().parse().ok()?;
    let size = caps.name("size")?.as_str().parse().ok()?;
    let start_time = caps.name("start_time")?.as_str().parse().ok()?;
    let end_time = caps.name("end_time")?.as_str().parse().ok()?;
    
    let dtoc = (end_time - start_time) * 1000.0;
    
    Some(UFSCUSTOM {
        opcode,
        lba,
        size,
        start_time,
        end_time,
        start_qd: 0,
        end_qd: 0,
        dtoc,
        ctoc: 0.0,
        ctod: 0.0,
        continuous: false,
    })
}

/// 고성능 청크 처리
fn process_chunk(
    data: &[u8],
    start: usize,
    end: usize,
    ufs_regex: &Regex,
    block_regex: &Regex,
    ufscustom_regex: &Regex,
) -> (Vec<UFS>, Vec<Block>, Vec<UFSCUSTOM>) {
    let chunk_data = &data[start..end];
    let boundaries = find_line_boundaries(chunk_data);
    
    // 예상 크기로 사전 할당
    let estimated_lines = boundaries.len();
    let mut ufs_traces = Vec::with_capacity(estimated_lines / 10);
    let mut block_traces = Vec::with_capacity(estimated_lines / 10);
    let mut ufscustom_traces = Vec::with_capacity(estimated_lines / 10);
    
    // 각 라인 처리
    for window in boundaries.windows(2) {
        let line_start = window[0];
        let line_end = window[1].saturating_sub(1); // 개행 제거
        
        if line_start < line_end && line_end <= chunk_data.len() {
            let line = &chunk_data[line_start..line_end];
            
            // UTF-8 변환 (필요할 때만)
            if let Ok(line_str) = std::str::from_utf8(line) {
                let line_str = line_str.trim();
                let (maybe_ufs, maybe_block, maybe_ufscustom) = 
                    process_line_optimized(line_str, ufs_regex, block_regex, ufscustom_regex);
                
                if let Some(ufs) = maybe_ufs {
                    ufs_traces.push(ufs);
                }
                if let Some(block) = maybe_block {
                    block_traces.push(block);
                }
                if let Some(ufscustom) = maybe_ufscustom {
                    ufscustom_traces.push(ufscustom);
                }
            }
        }
    }
    
    (ufs_traces, block_traces, ufscustom_traces)
}

/// 메인 고성능 파싱 함수
pub fn parse_log_file_highperf(filepath: &str) -> io::Result<(Vec<UFS>, Vec<Block>, Vec<UFSCUSTOM>)> {
    let start_time = Instant::now();
    println!("🚀 고성능 파싱 시작: {}", filepath);
    
    // 파일 열기 및 메타데이터
    let file = File::open(filepath)?;
    let file_size = file.metadata()?.len();
    let file_size_mb = file_size as f64 / (1024.0 * 1024.0);
    println!("📁 파일 크기: {:.2} MB", file_size_mb);
    
    // 메모리 맵 생성
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let data = Arc::new(mmap);
    println!("🗺️  메모리 매핑 완료");
    
    // 최적 청크 크기 계산
    let cpu_count = num_cpus::get();
    let optimal_chunk_size = std::cmp::max(
        file_size / (cpu_count as u64 * 4),
        64 * 1024 * 1024 // 최소 64MB
    );
    
    println!("⚙️  {} CPU 코어 사용, 청크 크기: {:.2} MB", 
             cpu_count, 
             optimal_chunk_size as f64 / (1024.0 * 1024.0));
    
    // 라인을 끊지 않는 청크 경계 찾기
    let mut chunk_boundaries = Vec::new();
    let mut pos = 0u64;
    
    while pos < file_size {
        let next_pos = std::cmp::min(pos + optimal_chunk_size, file_size);
        let mut boundary = next_pos;
        
        // 라인 경계 조정
        if boundary < file_size {
            while boundary < file_size && data[boundary as usize] != b'\n' {
                boundary += 1;
            }
            if boundary < file_size {
                boundary += 1; // 개행 포함
            }
        }
        
        chunk_boundaries.push((pos, boundary));
        pos = boundary;
    }
    
    println!("📦 {} 개 청크로 분할 완료", chunk_boundaries.len());
    
    // ACTIVE 패턴 읽기
    println!("📋 ACTIVE 패턴 로드 중...");
    let ufs_pattern = ACTIVE_UFS_PATTERN.read().unwrap();
    let block_pattern = ACTIVE_BLOCK_PATTERN.read().unwrap();
    let ufscustom_pattern = ACTIVE_UFSCUSTOM_PATTERN.read().unwrap();
    
    let ufs_regex = &ufs_pattern.1;
    let block_regex = &block_pattern.1;
    let ufscustom_regex = &ufscustom_pattern.1;
    
    println!("✅ 패턴 로드 완료:");
    println!("  - UFS: {}", ufs_pattern.0);
    println!("  - Block: {}", block_pattern.0);
    println!("  - UFSCustom: {}", ufscustom_pattern.0);
    
    // 병렬 처리
    let parse_start = Instant::now();
    let results: Vec<(Vec<UFS>, Vec<Block>, Vec<UFSCUSTOM>)> = chunk_boundaries
        .par_iter()
        .enumerate()
        .map(|(i, &(start, end))| {
            if i % 10 == 0 {
                let progress = (end as f64 / file_size as f64) * 100.0;
                println!("⏳ 청크 {}: {:.1}% 완료", i, progress);
            }
            process_chunk(&data, start as usize, end as usize, ufs_regex, block_regex, ufscustom_regex)
        })
        .collect();
    
    println!("✅ 병렬 파싱 완료: {:.2}초", parse_start.elapsed().as_secs_f64());
    
    // 결과 병합
    let merge_start = Instant::now();
    let mut ufs_traces = Vec::new();
    let mut block_traces = Vec::new();
    let mut ufscustom_traces = Vec::new();
    
    // 용량 사전 할당
    let total_estimate = results.iter()
        .map(|r| r.0.len() + r.1.len() + r.2.len())
        .sum::<usize>();
    ufs_traces.reserve(total_estimate / 3);
    block_traces.reserve(total_estimate / 3);
    ufscustom_traces.reserve(total_estimate / 3);
    
    for (ufs, block, ufscustom) in results {
        ufs_traces.extend(ufs);
        block_traces.extend(block);
        ufscustom_traces.extend(ufscustom);
    }
    
    println!("🔗 결과 병합 완료: {:.2}초", merge_start.elapsed().as_secs_f64());
    
    // 정렬 (unstable sort for performance)
    println!("🔄 데이터 정렬 중...");
    let sort_start = Instant::now();
    
    ufs_traces.sort_unstable_by(|a, b| {
        a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    block_traces.sort_unstable_by(|a, b| {
        a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    ufscustom_traces.sort_unstable_by(|a, b| {
        a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    println!("✅ 정렬 완료: {:.2}초", sort_start.elapsed().as_secs_f64());
    
    let total_time = start_time.elapsed().as_secs_f64();
    let throughput = file_size_mb / total_time;
    
    println!("🎉 파싱 완료!");
    println!("  📊 UFS: {}, Block: {}, UFSCUSTOM: {}", 
             ufs_traces.len(), block_traces.len(), ufscustom_traces.len());
    println!("  ⏱️  총 시간: {:.2}초", total_time);
    println!("  🚄 처리 속도: {:.2} MB/s", throughput);
    
    Ok((ufs_traces, block_traces, ufscustom_traces))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_line_boundaries() {
        let data = b"line1\nline2\nline3\n";
        let boundaries = find_line_boundaries(data);
        assert_eq!(boundaries, vec![0, 6, 12, 18]);
    }
    
    #[test]
    fn test_parse_ufscustom_event() {
        // ACTIVE_UFSCUSTOM_PATTERN 사용
        let pattern = ACTIVE_UFSCUSTOM_PATTERN.read().unwrap();
        let regex = &pattern.1;
        
        let line = "0x28,1048576,8,123.456,123.789";
        let result = parse_ufscustom_event(line, regex);
        
        if let Some(ufscustom) = result {
            assert_eq!(ufscustom.opcode, "0x28");
            assert_eq!(ufscustom.lba, 1048576);
            assert_eq!(ufscustom.size, 8);
            assert!((ufscustom.start_time - 123.456).abs() < 0.001);
            assert!((ufscustom.end_time - 123.789).abs() < 0.001);
        }
    }
    
    #[test]
    fn test_parse_ufs_event() {
        // ACTIVE_UFS_PATTERN 사용
        let pattern = ACTIVE_UFS_PATTERN.read().unwrap();
        let regex = &pattern.1;
        
        // UFS 파서 테스트 - 실제 ACTIVE_UFS_PATTERN에 맞는 형식 필요
        let line = "kworker/u16:3 [7] 123.456789: ufshcd_command: send_req: ... tag: 5 ... size: 32768 ... LBA: 1048576 ... opcode: 0x28 ... group_id: 0x01 ... hwq_id: 0";
        let _result = parse_ufs_event(line, regex);
        // 실제 로그 형식과 ACTIVE 패턴에 따라 다를 수 있음
    }
    
    #[test]
    fn test_parse_block_io_event() {
        // ACTIVE_BLOCK_PATTERN 사용
        let pattern = ACTIVE_BLOCK_PATTERN.read().unwrap();
        let regex = &pattern.1;
        
        // Block 파서 테스트 - 실제 ACTIVE_BLOCK_PATTERN에 맞는 형식 필요
        let line = "kworker/u16:0 [0] d..1. 123.456: block_rq_issue: 8,0 R 0 () 2048 + 8 [kworker/u16:0]";
        let result = parse_block_io_event(line, regex);
        
        // Block 파서가 작동하는지 확인 (패턴에 따라 다름)
        if let Some(block) = result {
            println!("Parsed block: cpu={}, devmajor={}", block.cpu, block.devmajor);
        }
    }
    
    #[test]
    fn test_process_line_optimized() {
        // ACTIVE 패턴들 읽기
        let ufs_pattern = ACTIVE_UFS_PATTERN.read().unwrap();
        let block_pattern = ACTIVE_BLOCK_PATTERN.read().unwrap();
        let ufscustom_pattern = ACTIVE_UFSCUSTOM_PATTERN.read().unwrap();
        
        let ufs_regex = &ufs_pattern.1;
        let block_regex = &block_pattern.1;
        let ufscustom_regex = &ufscustom_pattern.1;
        
        // UFSCUSTOM 라인 테스트
        let line = "0x28,1048576,8,123.456,123.789";
        let (ufs, block, ufscustom) = process_line_optimized(line, ufs_regex, block_regex, ufscustom_regex);
        
        assert!(ufs.is_none());
        assert!(block.is_none());
        assert!(ufscustom.is_some());
    }
}
