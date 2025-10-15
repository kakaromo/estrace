import { invoke } from "@tauri-apps/api/core";
import { tableFromIPC } from 'apache-arrow';
import { compareTraceCount, traceSizeCount, traceSaimpleCount } from '$stores/trace';
import { getBufferSize } from "$api/db";

export async function fetchTraceLengths(logname: string) {
  return await invoke('trace_lengths', { logname });
}

// 공통으로 사용되는 지연시간 임계값 상수
export const THRESHOLDS = [
  '0.1ms', '0.5ms', '1ms', '5ms', '10ms', '50ms', '100ms', 
  '500ms', '1s', '5s', '10s', '50s', '100s', '500s', '1000s'
];

/**
 * UFS 관련 통계 데이터를 가져오는 함수
 */
export async function fetchUfsStats(fileName: string, filterParams: any) {
  const { from_time, to_time, from_lba, to_lba, zoom_column } = filterParams;
  try {
    const result = await invoke('ufs_allstats', {
      logname: fileName,
      zoomColumn: zoom_column,
      timeFrom: from_time,
      timeTo: to_time,
      colFrom: from_lba,
      colTo: to_lba,
      thresholds: THRESHOLDS
    });
    return validateAllStats(result);
  } catch (error) {
    console.error('Error fetching UFS stats:', error);
    throw error;
  }
}

/**
 * Block 관련 통계 데이터를 가져오는 함수
 */
export async function fetchBlockStats(fileName: string, filterParams: any) {
  const { from_time, to_time, from_lba, to_lba, zoom_column } = filterParams;
  try {
    const result = await invoke('block_allstats', {
      logname: fileName,
      zoomColumn: zoom_column,
      timeFrom: from_time,
      timeTo: to_time,
      colFrom: from_lba,
      colTo: to_lba,
      thresholds: THRESHOLDS,
      group: true
    });
    return validateAllStats(result);
  } catch (error) {
    console.error('Error fetching Block stats:', error);
    throw error;
  }
}

/**
 * UFSCUSTOM 관련 통계 데이터를 가져오는 함수
 */
export async function fetchUfscustomStats(fileName: string, filterParams: any) {
  const { from_time, to_time, from_lba, to_lba, zoom_column } = filterParams;
  try {
    const result = await invoke('ufscustom_allstats', {
      logname: fileName,
      zoomColumn: zoom_column,
      timeFrom: from_time,
      timeTo: to_time,
      colFrom: from_lba,
      colTo: to_lba,
      thresholds: THRESHOLDS
    });
    return validateAllStats(result);
  } catch (error) {
    console.error('Error fetching UFSCUSTOM stats:', error);
    throw error;
  }
}

/**
 * 필터링된 데이터를 반환하는 함수
 * ⚡ N+1 문제 해결: 필터가 없으면 원본 데이터 재사용
 */
export async function filterTraceData(logname: string, traceData: any, selectedTrace: string, filterParams: any) {
  const { from_time, to_time, from_lba, to_lba, zoom_column } = filterParams;
  
  if (selectedTrace === '') {
    return null;
  }

  // ⚡ 최적화: 필터가 설정되지 않았으면 원본 데이터 그대로 반환 (N+1 방지)
  const hasNoFilter = (!from_time || from_time === 0) && 
                      (!to_time || to_time === 0) && 
                      (!from_lba || from_lba === 0) && 
                      (!to_lba || to_lba === 0);
  
  if (hasNoFilter) {
    console.log('[Performance] ⚡ 필터 없음 - 원본 데이터 재사용 (백엔드 호출 생략)');
    return traceData;
  }

  // 필터가 있을 때만 백엔드 호출
  console.log('[Performance] 필터 적용 - 백엔드에서 데이터 가져오는 중...');
  
  const buffersize = await getBufferSize();
  const result: any = await invoke('filter_trace', {
    logname: logname,
    tracetype: selectedTrace,
    zoomColumn: zoom_column,
    timeFrom: from_time,
    timeTo: to_time,
    colFrom: from_lba,
    colTo: to_lba,
    maxrecords: buffersize
  });

  // Arrow IPC 데이터 직접 변환 (압축 제거됨)
  const ufsData = new Uint8Array(result.ufs.bytes);
  const blockData = new Uint8Array(result.block.bytes);
  const ufscustomData = new Uint8Array(result.ufscustom.bytes);
  
  const ufsTable = tableFromIPC(ufsData);
  const blockTable = tableFromIPC(blockData);
  const ufscustomTable = tableFromIPC(ufscustomData);
  
  console.log('[Performance] filterTraceData 완료 - Arrow Table 직접 사용');
  
  const filteredTraceData = {
    ufs: {
      table: ufsTable,
      data: null,
      total_count: result.ufs.total_count,
      sampled_count: result.ufs.sampled_count,
      sampling_ratio: result.ufs.sampling_ratio
    },
    block: {
      table: blockTable,
      data: null,
      total_count: result.block.total_count,
      sampled_count: result.block.sampled_count,
      sampling_ratio: result.block.sampling_ratio
    },
    ufscustom: {
      table: ufscustomTable,
      data: null,
      total_count: result.ufscustom.total_count,
      sampled_count: result.ufscustom.sampled_count,
      sampling_ratio: result.ufscustom.sampling_ratio
    }
  };

  return filteredTraceData;
}

function parseJsonResult(result: any) {
  try {
    if (typeof result === 'string') {
      return JSON.parse(result);
    }
    if (result instanceof Uint8Array) {
      return JSON.parse(new TextDecoder().decode(result));
    }
    if (Array.isArray(result)) {
      return JSON.parse(new TextDecoder().decode(new Uint8Array(result)));
    }
    return result;
  } catch (e) {
    console.error('Error parsing JSON result:', e);
    return {};
  }
}

/**
 * 통계 데이터 검증 및 기본값 제공 헬퍼 함수들
 */
function validateLatencyStats(result: any) {
  try {
    const parsedResult = parseJsonResult(result);
    
    // 필요한 속성이 없으면 기본값 제공
    if (!parsedResult.latency_counts) {
      parsedResult.latency_counts = {};
    }
    
    if (!parsedResult.summary) {
      parsedResult.summary = {};
    }
    
    return parsedResult;
  } catch (e) {
    console.error('Error parsing latency stats:', e);
    return {
      latency_counts: {},
      summary: {}
    };
  }
}

function validateSizeStats(result: any) {
  try {
    const parsedResult = parseJsonResult(result);
    return parsedResult;
  } catch (e) {
    console.error('Error parsing size stats:', e);
    return {
      opcode_stats: {}
    };
  }
}

function validateContinuityStats(result: any) {
  try {
    const parsedResult = parseJsonResult(result);
    return parsedResult;
  } catch (e) {
    console.error('Error parsing continuity stats:', e);
    return {
      op_stats: {}
    };
  }
}

function validateAllStats(result: any) {
  const parsed = parseJsonResult(result);
  return {
    dtocStat: validateLatencyStats(parsed.dtoc_stat),
    ctodStat: validateLatencyStats(parsed.ctod_stat),
    ctocStat: validateLatencyStats(parsed.ctoc_stat),
    sizeCounts: validateSizeStats(parsed.size_counts),
    continuous: validateContinuityStats(parsed.continuity)
  };
}
