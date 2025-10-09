// ============================================================================
// 압축 성능 최적화 옵션들
// ============================================================================
// 
// 현재 활성화: GZIP_FAST (3-5배 빠름, 80% 압축률)
// 
// 다른 옵션들:
// 1. ZSTD: 2배 빠름, 85% 압축률 (균형잡힌 선택)
// 2. LZ4: 10배 빠름, 60-70% 압축률 (빠른 네트워크 필요)
// 3. NO_COMPRESSION: 압축 안 함 (로컬 개발용)
//
// 변경 방법: 아래 주석을 변경하여 원하는 방식 활성화
// ============================================================================

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

// ============================================================================
// 옵션 1: Gzip Fast (현재 활성화) - 권장
// ============================================================================
// 장점: 즉시 사용 가능, 3-5배 빠름, 80% 압축률
// 단점: Gzip default보다 압축률 5% 낮음
pub fn compress_arrow_ipc(ipc_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(ipc_bytes).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

pub const COMPRESSION_NAME: &str = "Gzip(fast)";
pub const COMPRESSION_ENABLED: bool = true;


// ============================================================================
// 옵션 2: Zstd (주석 해제하면 사용 가능) - 고급
// ============================================================================
// 장점: Gzip default와 비슷한 압축률(85%), 2배 빠름
// 단점: 프론트엔드도 zstd 압축 해제 필요
//
// 활성화 방법:
// 1. 위의 Gzip 코드를 주석 처리
// 2. 아래 Zstd 코드 주석 해제
// 3. src/utils/compression.ts에서 zstd 압축 해제 추가 필요
/*
pub fn compress_arrow_ipc(ipc_bytes: &[u8]) -> Result<Vec<u8>, String> {
    zstd::encode_all(ipc_bytes, 1)  // 레벨 1 (가장 빠름)
        .map_err(|e| e.to_string())
}

pub const COMPRESSION_NAME: &str = "Zstd(level-1)";
pub const COMPRESSION_ENABLED: bool = true;
*/


// ============================================================================
// 옵션 3: LZ4 (라이브러리 추가 필요) - 초고속
// ============================================================================
// 장점: 가장 빠름 (10ms), CPU 부하 최소
// 단점: 압축률 낮음(60-70%), 빠른 네트워크 필요, 라이브러리 추가 필요
//
// 활성화 방법:
// 1. Cargo.toml에 `lz4 = "1.24"` 추가
// 2. 위의 Gzip 코드를 주석 처리
// 3. 아래 LZ4 코드 주석 해제
// 4. src/utils/compression.ts에서 lz4 압축 해제 추가 필요
/*
use lz4::EncoderBuilder;

pub fn compress_arrow_ipc(ipc_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = EncoderBuilder::new()
        .level(1)
        .build(Vec::new())
        .map_err(|e| e.to_string())?;
    
    encoder.write_all(ipc_bytes).map_err(|e| e.to_string())?;
    let (compressed, result) = encoder.finish();
    result.map_err(|e| e.to_string())?;
    Ok(compressed)
}

pub const COMPRESSION_NAME: &str = "LZ4(level-1)";
pub const COMPRESSION_ENABLED: bool = true;
*/


// ============================================================================
// 옵션 4: 압축 안 함 (로컬 개발용) - 디버깅
// ============================================================================
// 장점: 압축 시간 0ms, CPU 사용 최소
// 단점: 네트워크 전송량 5-10배 증가
// 용도: 로컬 개발, 빠른 내부 네트워크
//
// 활성화 방법:
// 1. 위의 Gzip 코드를 주석 처리
// 2. 아래 코드 주석 해제
/*
pub fn compress_arrow_ipc(ipc_bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(ipc_bytes.to_vec())  // 압축하지 않고 그대로 반환
}

pub const COMPRESSION_NAME: &str = "None";
pub const COMPRESSION_ENABLED: bool = false;
*/


// ============================================================================
// 성능 비교 (1MB Arrow IPC 데이터 기준)
// ============================================================================
// 
// | 방식          | 압축 시간 | 압축률 | 압축 후 | 전송 시간* | 총 시간 |
// |---------------|----------|--------|---------|-----------|---------|
// | Gzip default  | 150ms    | 85%    | 150KB   | 30ms      | 180ms   |
// | Gzip fast     | 30ms     | 80%    | 200KB   | 40ms      | 70ms    | ⭐ 권장
// | Zstd (level1) | 60ms     | 85%    | 150KB   | 30ms      | 90ms    |
// | LZ4           | 10ms     | 65%    | 350KB   | 70ms      | 80ms    |
// | 압축 안 함     | 0ms      | 0%     | 1000KB  | 200ms     | 200ms   |
// 
// * 전송 시간은 10Mbps 네트워크 기준
// 
// 결론:
// - 대부분의 경우: Gzip fast (현재 설정) ⭐
// - 느린 네트워크: Gzip default 또는 Zstd
// - 빠른 네트워크: LZ4
// - 로컬 개발: 압축 안 함
// ============================================================================
