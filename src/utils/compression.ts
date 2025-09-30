import { gunzipSync } from 'fflate';

/**
 * Gzip으로 압축된 데이터를 해제하는 함수
 * @param compressedBytes Gzip으로 압축된 바이트 배열
 * @returns 압축 해제된 바이트 배열
 */
export function decompressGzipData(compressedBytes: Uint8Array): Uint8Array {
    try {
        // Gzip 압축 해제
        const decompressed = gunzipSync(compressedBytes);
        return decompressed;
    } catch (error) {
        console.error('Gzip 압축 해제 실패:', error);
        throw new Error(`데이터 압축 해제에 실패했습니다: ${error instanceof Error ? error.message : String(error)}`);
    }
}

/**
 * 압축된 데이터인지 확인하고 필요시 압축 해제하는 함수
 * @param bytes 바이트 배열
 * @param compressed 압축 여부 플래그
 * @returns 압축 해제된 바이트 배열
 */
export function handleCompressedData(bytes: Uint8Array, compressed: boolean, originalSize?: number): Uint8Array {
    if (compressed) {
        console.log('🗜️ Gzip 압축된 데이터 감지, 압축 해제 중...');
        const startTime = performance.now();
        const decompressed = decompressGzipData(bytes);
        const endTime = performance.now();
        console.log(`✅ Gzip 압축 해제 완료: ${bytes.length} -> ${decompressed.length} bytes (소요시간: ${(endTime - startTime).toFixed(2)}ms)`);
        return decompressed;
    }
    return bytes;
}

/**
 * 압축 정보를 로깅하는 함수
 * @param label 데이터 라벨 (예: "UFS", "Block")
 * @param compressed 압축 여부
 * @param originalSize 원본 크기
 * @param compressedSize 압축된 크기
 * @param compressionRatio 압축비
 */
export function logCompressionInfo(
    label: string, 
    compressed: boolean, 
    originalSize: number, 
    compressedSize: number, 
    compressionRatio: number
) {
    if (compressed) {
        const savingPercentage = ((1 - compressionRatio) * 100).toFixed(1);
        const compressionFactor = (1 / compressionRatio).toFixed(1);
        console.log(`📊 ${label} 압축 정보:`, {
            원본크기: `${(originalSize / 1024).toFixed(1)} KB`,
            압축크기: `${(compressedSize / 1024).toFixed(1)} KB`,
            압축률: `${savingPercentage}% 절약`,
            압축비: `${compressionFactor}:1`
        });
    } else {
        console.log(`📊 ${label} 데이터: 압축되지 않음 (${(compressedSize / 1024).toFixed(1)} KB)`);
    }
}