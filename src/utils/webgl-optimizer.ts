/**
 * WebGL 최적화 유틸리티
 * Apache Arrow 데이터를 직접 Float32Array로 변환하여 성능 향상
 */

import type { Table } from 'apache-arrow';

export interface WebGLOptimizedData {
  positions: Float32Array;  // [x1, y1, x2, y2, ...]
  colorIndices: Uint8Array; // 각 포인트의 색상 인덱스
  legends: Map<string, number>; // legend 값 -> 인덱스 매핑
  bounds: {
    xMin: number;
    xMax: number;
    yMin: number;
    yMax: number;
  };
  pointCount: number;
}

/**
 * Arrow Table을 WebGL 최적화 포맷으로 변환
 * - 중간 JavaScript 객체 생성 없이 직접 TypedArray로 변환
 * - BigInt 처리 최소화
 */
export function arrowToWebGLData(
  table: Table,
  xKey: string,
  yKey: string,
  legendKey: string,
  actionFilter?: string | null
): WebGLOptimizedData {
  const startTime = performance.now();
  
  // 필터링이 필요한 경우 컬럼 확인
  const needsFiltering = actionFilter && (yKey === 'dtoc' || yKey === 'ctod' || yKey === 'ctoc');
  const actionColumn = needsFiltering ? table.getChild('action') : null;
  
  // 예상 크기로 배열 미리 할당
  const estimatedSize = table.numRows;
  const positions = new Float32Array(estimatedSize * 2);
  const colorIndices = new Uint8Array(estimatedSize);
  const legends = new Map<string, number>();
  
  let actualCount = 0;
  let xMin = Infinity;
  let xMax = -Infinity;
  let yMin = Infinity;
  let yMax = -Infinity;
  
  // 컬럼 직접 접근 (toArray() 호출 방지)
  const xColumn = table.getChild(xKey);
  const yColumn = table.getChild(yKey);
  const legendColumn = table.getChild(legendKey);
  
  if (!xColumn || !yColumn || !legendColumn) {
    throw new Error(`Required columns not found: ${xKey}, ${yKey}, ${legendKey}`);
  }
  
  // 각 행 처리
  for (let i = 0; i < table.numRows; i++) {
    // 필터링 체크
    if (needsFiltering && actionColumn) {
      const action = actionColumn.get(i);
      const shouldInclude = 
        (actionFilter === 'send_req' && (action === 'send_req' || action === 'block_rq_issue')) ||
        (actionFilter === 'complete_rsp' && (action === 'complete_rsp' || action === 'block_rq_complete'));
      
      if (!shouldInclude) {
        continue;
      }
    }
    
    // X 값 추출 (BigInt 처리)
    let xValue = xColumn.get(i);
    if (typeof xValue === 'bigint') {
      xValue = Number(xValue);
    }
    
    // Y 값 추출 (BigInt 처리)
    let yValue = yColumn.get(i);
    if (typeof yValue === 'bigint') {
      yValue = Number(yValue);
    }
    
    // 유효성 검사
    if (!isFinite(xValue) || !isFinite(yValue)) {
      continue;
    }
    
    // Legend 값 추출 및 인덱스 매핑
    const legendValue = String(legendColumn.get(i));
    let legendIndex = legends.get(legendValue);
    if (legendIndex === undefined) {
      legendIndex = legends.size;
      legends.set(legendValue, legendIndex);
    }
    
    // 데이터 저장
    const posIndex = actualCount * 2;
    positions[posIndex] = xValue;
    positions[posIndex + 1] = yValue;
    colorIndices[actualCount] = legendIndex;
    
    // Bounds 업데이트
    xMin = Math.min(xMin, xValue);
    xMax = Math.max(xMax, xValue);
    yMin = Math.min(yMin, yValue);
    yMax = Math.max(yMax, yValue);
    
    actualCount++;
  }
  
  const endTime = performance.now();
  console.log(`[WebGL Optimizer] 변환 완료: ${actualCount}개 포인트, ${(endTime - startTime).toFixed(2)}ms`);
  
  // 실제 사용된 크기로 자르기
  return {
    positions: actualCount === estimatedSize 
      ? positions 
      : positions.slice(0, actualCount * 2),
    colorIndices: actualCount === estimatedSize
      ? colorIndices
      : colorIndices.slice(0, actualCount),
    legends,
    bounds: {
      xMin: xMin === Infinity ? 0 : xMin,
      xMax: xMax === -Infinity ? 0 : xMax,
      yMin: yMin === Infinity ? 0 : yMin,
      yMax: yMax === -Infinity ? 0 : yMax,
    },
    pointCount: actualCount
  };
}

/**
 * 기존 필터링된 데이터를 WebGL 포맷으로 변환
 * (이미 객체 배열로 변환된 경우 사용)
 */
export function dataToWebGLFormat(
  data: any[],
  xKey: string,
  yKey: string,
  legendKey: string
): WebGLOptimizedData {
  const startTime = performance.now();
  
  const positions = new Float32Array(data.length * 2);
  const colorIndices = new Uint8Array(data.length);
  const legends = new Map<string, number>();
  
  let xMin = Infinity;
  let xMax = -Infinity;
  let yMin = Infinity;
  let yMax = -Infinity;
  
  for (let i = 0; i < data.length; i++) {
    const row = data[i];
    
    // 값 추출
    let xValue = row[xKey];
    let yValue = row[yKey];
    const legendValue = String(row[legendKey]);
    
    // BigInt 처리
    if (typeof xValue === 'bigint') xValue = Number(xValue);
    if (typeof yValue === 'bigint') yValue = Number(yValue);
    
    // Legend 인덱스
    let legendIndex = legends.get(legendValue);
    if (legendIndex === undefined) {
      legendIndex = legends.size;
      legends.set(legendValue, legendIndex);
    }
    
    // 데이터 저장
    positions[i * 2] = xValue;
    positions[i * 2 + 1] = yValue;
    colorIndices[i] = legendIndex;
    
    // Bounds
    xMin = Math.min(xMin, xValue);
    xMax = Math.max(xMax, xValue);
    yMin = Math.min(yMin, yValue);
    yMax = Math.max(yMax, yValue);
  }
  
  const endTime = performance.now();
  console.log(`[WebGL Optimizer] 변환 완료: ${data.length}개 포인트, ${(endTime - startTime).toFixed(2)}ms`);
  
  return {
    positions,
    colorIndices,
    legends,
    bounds: {
      xMin: xMin === Infinity ? 0 : xMin,
      xMax: xMax === -Infinity ? 0 : xMax,
      yMin: yMin === Infinity ? 0 : yMin,
      yMax: yMax === -Infinity ? 0 : yMax,
    },
    pointCount: data.length
  };
}
