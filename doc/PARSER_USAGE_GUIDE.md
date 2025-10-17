# 고성능 파서 사용 가이드

## 빠른 시작

### 1. Tauri 명령 사용 (프론트엔드)

```typescript
// src/routes/+page.svelte 또는 다른 컴포넌트에서
import { invoke } from '@tauri-apps/api/core';

async function parseTraceHighPerf() {
    try {
        const result = await invoke('readtrace_highperf', {
            logname: '/path/to/your/trace.log'
        });
        
        console.log('✅ 파싱 완료:', result);
        // 결과: "고성능 파싱 완료: UFS=123456, Block=23456, UFSCUSTOM=34567"
    } catch (error) {
        console.error('❌ 파싱 실패:', error);
    }
}

// 버튼 클릭 이벤트
<button on:click={parseTraceHighPerf}>
    고성능 파싱 시작
</button>
```

### 2. Rust에서 직접 사용

```rust
// 다른 Rust 코드에서 직접 호출
use crate::trace::parser_highperf::parse_log_file_highperf;

fn my_function() {
    let filepath = "/path/to/trace.log";
    
    match parse_log_file_highperf(filepath) {
        Ok((ufs_traces, block_traces, ufscustom_traces)) => {
            println!("✅ 파싱 성공!");
            println!("  UFS: {} 개", ufs_traces.len());
            println!("  Block: {} 개", block_traces.len());
            println!("  UFSCUSTOM: {} 개", ufscustom_traces.len());
            
            // 데이터 활용
            for ufs in ufs_traces.iter().take(10) {
                println!("UFS: time={}, lba={}, size={}", 
                         ufs.time, ufs.lba, ufs.size);
            }
        }
        Err(e) => {
            eprintln!("❌ 파싱 실패: {}", e);
        }
    }
}
```

## 성능 비교 테스트

### 기존 파서와 고성능 파서 비교

```rust
use std::time::Instant;

// 기존 파서
let start = Instant::now();
let old_result = readtrace(filepath.to_string(), Some(1000000)).await?;
let old_time = start.elapsed();
println!("기존 파서: {:.2}초", old_time.as_secs_f64());

// 고성능 파서
let start = Instant::now();
let new_result = parse_log_file_highperf(filepath)?;
let new_time = start.elapsed();
println!("고성능 파서: {:.2}초", new_time.as_secs_f64());

println!("성능 향상: {:.1}배", old_time.as_secs_f64() / new_time.as_secs_f64());
```

## 예상 출력

```
🚀 고성능 파싱 시작: /data/android_trace.log
📁 파일 크기: 512.30 MB
🗺️  메모리 매핑 완료
⚙️  8 CPU 코어 사용, 청크 크기: 64.00 MB
📦 8 개 청크로 분할 완료
⏳ 청크 0: 12.5% 완료
⏳ 청크 10: 100.0% 완료
✅ 병렬 파싱 완료: 3.21초
🔗 결과 병합 완료: 0.82초
🔄 데이터 정렬 중...
✅ 정렬 완료: 1.05초
🎉 파싱 완료!
  📊 UFS: 856,423, Block: 142,567, UFSCUSTOM: 0
  ⏱️  총 시간: 5.08초
  🚄 처리 속도: 100.8 MB/s
```

## 로그 형식별 파싱 예제

### UFSCUSTOM (CSV)
```
입력: 0x28,1048576,32,123.456,123.789
파싱 결과:
  - opcode: "0x28"
  - lba: 1048576
  - size: 32
  - start_time: 123.456
  - end_time: 123.789
  - dtoc: 333.0 ms (자동 계산)
```

### UFS
```
입력: kworker/u16:3 [7] 123.456789: ufshcd_command: send_req: ... tag: 5 size: 32768 LBA: 1048576 opcode: 0x28 group_id: 0x01 hwq_id: 0
파싱 결과:
  - cpu: 7
  - time: 123.456789
  - action: "send_req"
  - tag: 5
  - size: 8 (32768 / 4096)
  - lba: 1048576
  - opcode: "0x28"
```

### Block
```
입력: kworker/u16:0 [0] d..1. 123.456: block_rq_issue: 8,0 R 0 () 2048 + 8 [kworker/u16:0]
파싱 결과:
  - cpu: 0
  - time: 123.456
  - flags: "d..1."
  - action: "block_rq_issue"
  - devmajor: 8, devminor: 0
  - io_type: "R"
  - sector: 2048
  - size: 8
  - comm: "kworker/u16:0"
```

## 에러 처리

```rust
match parse_log_file_highperf(filepath) {
    Ok(result) => {
        // 성공
    }
    Err(e) => {
        match e.kind() {
            std::io::ErrorKind::NotFound => {
                eprintln!("파일을 찾을 수 없습니다: {}", filepath);
            }
            std::io::ErrorKind::PermissionDenied => {
                eprintln!("파일 접근 권한이 없습니다");
            }
            _ => {
                eprintln!("파싱 오류: {}", e);
            }
        }
    }
}
```

## 대용량 파일 처리 팁

### 1. 메모리 부족 시
```rust
// 청크 크기 조정
let optimal_chunk_size = std::cmp::max(
    file_size / (cpu_count as u64 * 8), // 4 대신 8로 증가
    32 * 1024 * 1024  // 64MB 대신 32MB로 감소
);
```

### 2. 진행 상황 모니터링
```rust
// 현재 구현에서는 자동으로 진행률 출력
// 향후 콜백 기능 추가 예정
```

### 3. 취소 기능 (향후 추가)
```rust
// TODO: CANCEL_SIGNAL 활용
use crate::trace::CANCEL_SIGNAL;

// 취소 설정
{
    let mut cancel = CANCEL_SIGNAL.lock().unwrap();
    *cancel = true;
}
```

## 성능 측정

### 벤치마크 코드
```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;
    
    #[test]
    #[ignore] // cargo test --ignored로 실행
    fn bench_parse_large_file() {
        let filepath = "/path/to/large/trace.log";
        
        let start = Instant::now();
        let result = parse_log_file_highperf(filepath).unwrap();
        let elapsed = start.elapsed();
        
        let file_size = std::fs::metadata(filepath).unwrap().len();
        let throughput = file_size as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64();
        
        println!("파일 크기: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));
        println!("처리 시간: {:.2}초", elapsed.as_secs_f64());
        println!("처리 속도: {:.2} MB/s", throughput);
        println!("총 레코드: {}", result.0.len() + result.1.len() + result.2.len());
    }
}
```

### 실행
```bash
# 벤치마크 테스트
cd src-tauri
cargo test --release --ignored bench_parse_large_file -- --nocapture

# 프로파일링
cargo build --release
time ./target/release/estrace
```

## 문제 해결

### 파싱 실패 시
1. **로그 형식 확인**: 실제 로그 파일의 형식이 예상과 다를 수 있음
2. **인코딩 확인**: UTF-8이 아닌 경우 변환 필요
3. **디버그 모드**: `--features debug-parser`로 상세 로그 확인

### 성능이 예상보다 낮을 때
1. **CPU 코어 확인**: `num_cpus::get()` 결과 확인
2. **디스크 I/O**: SSD vs HDD 차이 고려
3. **메모리 부족**: 시스템 모니터 확인

## 참고 자료

- [HIGH_PERFORMANCE_PARSER.md](./HIGH_PERFORMANCE_PARSER.md) - 구현 상세
- [PARSER_COMPLETE_REPORT.md](./PARSER_COMPLETE_REPORT.md) - 완료 보고서
- [kakaromo/trace](https://github.com/kakaromo/trace) - 참고 구현

## 라이선스

MIT License - 자유롭게 사용 가능
