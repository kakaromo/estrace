import { invoke } from "@tauri-apps/api/core";
import { compareTraceCount, traceSizeCount, traceSaimpleCount } from '$stores/trace';
import { getBufferSize } from "$api/db";

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
  
  const results = {
    dtocStat: null,
    ctodStat: null,
    ctocStat: null,
    sizeCounts: null,
    continuous: null
  };

  try {
    // 병렬로 모든 통계 데이터 요청
    const [dtocStatResult, ctodStatResult, ctocStatResult, sizeCountsResult, continuousResult] = await Promise.all([
      invoke('ufs_latencystats', { 
        logname: fileName, 
        column: 'dtoc', 
        thresholds: THRESHOLDS,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('ufs_latencystats', { 
        logname: fileName, 
        column: 'ctod', 
        thresholds: THRESHOLDS,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('ufs_latencystats', { 
        logname: fileName, 
        column: 'ctoc', 
        thresholds: THRESHOLDS,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('ufs_sizestats', { 
        logname: fileName, 
        column: 'dtoc', 
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('ufs_continuity_stats', { 
        logname: fileName, 
        column: 'dtoc',
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      })
    ]);

    // 결과 파싱
    results.dtocStat = validateLatencyStats(dtocStatResult);
    results.ctodStat = validateLatencyStats(ctodStatResult);
    results.ctocStat = validateLatencyStats(ctocStatResult);
    results.sizeCounts = validateSizeStats(sizeCountsResult);
    results.continuous = validateContinuityStats(continuousResult);

    return results;
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
  
  const results = {
    dtocStat: null,
    ctodStat: null,
    ctocStat: null,
    sizeCounts: null,
    continuous: null
  };

  try {
    // 병렬로 모든 통계 데이터 요청
    const [dtocStatResult, ctodStatResult, ctocStatResult, sizeCountsResult, continuousResult] = await Promise.all([
      invoke('block_latencystats', { 
        logname: fileName, 
        column: 'dtoc', 
        thresholds: THRESHOLDS, 
        group: true,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('block_latencystats', { 
        logname: fileName, 
        column: 'ctod', 
        thresholds: THRESHOLDS, 
        group: true,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('block_latencystats', { 
        logname: fileName, 
        column: 'ctoc', 
        thresholds: THRESHOLDS, 
        group: true,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('block_sizestats', { 
        logname: fileName, 
        column: 'dtoc', 
        group: true,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      }),
      invoke('block_continuity_stats', { 
        logname: fileName, 
        column: 'dtoc', 
        group: true,
        timeFrom: from_time, 
        timeTo: to_time, 
        colFrom: from_lba, 
        colTo: to_lba, 
        zoomColumn: zoom_column 
      })
    ]);

    // 결과 파싱
    results.dtocStat = validateLatencyStats(dtocStatResult);
    results.ctodStat = validateLatencyStats(ctodStatResult);
    results.ctocStat = validateLatencyStats(ctocStatResult);
    results.sizeCounts = validateSizeStats(sizeCountsResult);
    results.continuous = validateContinuityStats(continuousResult);

    return results;
  } catch (error) {
    console.error('Error fetching Block stats:', error);
    throw error;
  }
}

/**
 * 필터링된 데이터를 반환하는 함수
 */
export async function filterTraceData(logname: string, traceData: any, selectedTrace: string, filterParams: any) {
  const { from_time, to_time, from_lba, to_lba, zoom_column } = filterParams;
  // console.log(filterParams);
  if (selectedTrace === '') {
    return null;
  }
  // if (traceData[selectedTrace].total_count === traceData[selectedTrace].sampled_count) {
  //   // 필터가 설정되지 않았으면 원본 데이터 반환
  //   if (from_time === 0 && to_time === 0) {
  //     return traceData[selectedTrace];
  //   }
  //   // 필터 적용
  //   const filteredData = traceData[selectedTrace].data.filter((item) => {
  //     return item.time >= from_time &&
  //           item.time <= to_time &&
  //           item[zoom_column] >= from_lba &&
  //           item[zoom_column] <= to_lba;
  //   });
  //   traceData[selectedTrace].data = filteredData;
  //   return traceData[selectedTrace];
    
  // } else {
  //   // 필터링된 데이터 반환
  //   console.log('logname:', logname);
  //   console.log('selectedTrace:', selectedTrace);
  //   let buffersize = await getBufferSize();
  //   const traceStr: string = await invoke('filter_trace', {
  //     logname: logname,
  //     tracetype: selectedTrace,
  //     zoomColumn: zoom_column,
  //     from_time: from_time,
  //     to_time: to_time,
  //     from_lba: from_lba,
  //     to_lba: to_lba,
  //     maxrecords: buffersize
  //   });
  //   const filteredData : any = JSON.parse(traceStr);
  //   console.log('filteredData:', filteredData);
  //   return filteredData;
  // }

  let buffersize = await getBufferSize();
    // const traceStr: string = await invoke('filter_trace', {
    //   logname: logname,
    //   tracetype: selectedTrace,
    //   zoomColumn: zoom_column,
    //   fromTime: from_time,
    //   toTime: to_time,
    //   fromLba: from_lba,
    //   toLba: to_lba,
    //   maxrecords: buffersize
    // });
    const traceStr: string = await invoke('filter_trace', {
      logname: logname,
      tracetype: selectedTrace,
      zoomColumn: zoom_column,
      timeFrom: from_time, 
      timeTo: to_time, 
      colFrom: from_lba, 
      colTo: to_lba,
      maxrecords: buffersize
    });
    const filteredData : any = JSON.parse(traceStr);
    return filteredData;
}

/**
 * 통계 데이터 검증 및 기본값 제공 헬퍼 함수들
 */
function validateLatencyStats(result: any) {
  try {
    const parsedResult = typeof result === 'string' ? JSON.parse(result) : result;
    
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
    const parsedResult = typeof result === 'string' ? JSON.parse(result) : result;
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
    const parsedResult = typeof result === 'string' ? JSON.parse(result) : result;
    return parsedResult;
  } catch (e) {
    console.error('Error parsing continuity stats:', e);
    return {
      op_stats: {}
    };
  }
}
