<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Deck, OrthographicView } from '@deck.gl/core';
  import { ScatterplotLayer } from '@deck.gl/layers';
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";
  import { filtertrace } from '$stores/trace';

  interface ScatterChartProps {
    data?: any[];  // 호환성용 (선택적)
    table?: any;   // Arrow Table 객체 (최적화)
    xAxisKey: string;
    yAxisKey: string;
    legendKey: string;
    xAxisLabel?: string;
    yAxisLabel?: string;
    ycolumn: string;
    actionFilter?: string;
  }

  let { 
    data = [], 
    table = null,
    xAxisKey, 
    yAxisKey, 
    legendKey, 
    xAxisLabel = 'time', 
    yAxisLabel = 'sector', 
    ycolumn, 
    actionFilter 
  }: ScatterChartProps = $props();

  // 차트 상태 변수
  let chartTitle = $state('');
  let showTitleDialog = $state(false);
  let inputTitle = $state('');
  let showSymbolSizeDialog = $state(false);
  let inputSymbolSize = $state(1);
  let inputSymbolSizeArray = $state([1]);
  let symbolSize = $state(1);
  let xAxisName = $state(xAxisLabel);
  let yAxisName = $state(yAxisLabel);
  let legendshow = $state(true);
  let prevData: any = null;
  
  // Y축 타입 판단 (latency/dtoc/ctod/ctoc: 소수점 3자리, cpu: -1~8 고정)
  let isLatencyAxis = $derived(
    ycolumn === 'dtoc' ||
    ycolumn === 'ctod' ||
    ycolumn === 'ctoc'
  );
  let isCpuAxis = $derived(yAxisLabel?.toLowerCase().includes('cpu') || ycolumn?.toLowerCase().includes('cpu'));
  
  // 로딩 상태 관리
  let isInitializing = $state(false);
  let loadingProgress = $state(0);
  let loadingMessage = $state('');

  // Deck.gl 요소
  let deckContainer: HTMLDivElement;
  let deckInstance: any = null;
  let canvas: HTMLCanvasElement;
  let resizeObserver: ResizeObserver;
  
  // 데이터 캐싱을 위한 변수
  let transformedDataCache: any[] = [];
  let lastDataLength = 0;
  
  // 초기화 재시도 관련
  let initRetryCount = 0;
  let maxInitRetries = 5;

  // zoom 및 pan 상태
  let viewState: any = $state({
    target: [0, 0, 0] as [number, number, number],
    zoom: 0,
    minZoom: -10,
    maxZoom: 10,
    transitionDuration: 0
  });

  // 범위 설정 상태 변수
  let showXAxisRangeDialog = $state(false);
  let inputXMin = $state(0);
  let inputXMax = $state(0);
  let showYAxisRangeDialog = $state(false);
  let inputYMin = $state(0);
  let inputYMax = $state(0);

  // 축 범위 저장
  let dataBounds = $state({ xMin: 0, xMax: 100, yMin: 0, yMax: 100 });
  let originalDataBounds = { xMin: 0, xMax: 100, yMin: 0, yMax: 100 }; // 원본 범위 저장
  let containerWidth = $state(800);
  let containerHeight = $state(600);

  // 드래그 선택 상태
  let isDragging = $state(false);
  let dragStartX = $state(0);
  let dragStartY = $state(0);
  let dragEndX = $state(0);
  let dragEndY = $state(0);
  let isShiftPressed = $state(false);

  // 축 패딩 상수 - echarts와 유사하게 조정
  const PADDING_LEFT = 100; // Y축 큰 숫자를 위해 증가
  const PADDING_BOTTOM = 50;
  const PADDING_TOP = 30;
  const PADDING_RIGHT = 30; // 범례는 absolute로 오버레이

  // 차트 영역 크기 계산 (derived state)
  let chartWidth = $derived(containerWidth - PADDING_LEFT - PADDING_RIGHT);
  let chartHeight = $derived(containerHeight - PADDING_TOP - PADDING_BOTTOM);

  // 축 틱 생성 함수
  function generateTicks(min: number, max: number, count: number = 8) {
    const range = max - min;
    const step = range / (count - 1);
    const ticks = [];
    
    for (let i = 0; i < count; i++) {
      ticks.push(min + step * i);
    }
    
    return ticks;
  }

  // 색상 상수 및 팔레트
  const WRITE_COLOR = [255, 0, 0];
  const READ_COLOR = [0, 0, 255];
  const DISCARD_COLOR = [0, 255, 0];
  const FLUSH_COLOR = [255, 255, 0];

  const UFS_COMMAND_COLORS: { [key: string]: number[] } = {
    '0x2a': [255, 0, 0],
    '0xa2': [255, 51, 51],
    '0x28': [0, 0, 255],
    '0xb5': [51, 51, 255],
    '0x42': [0, 255, 0],
    '0x1b': [255, 0, 255],
    '0x12': [0, 255, 255],
    '0x35': [255, 255, 0],
    '0xc0': [255, 136, 0],
  };

  const WRITE_PALETTE = [
    [255, 0, 0], [255, 51, 51], [255, 102, 102], [255, 153, 153], [255, 204, 204]
  ];
  
  const READ_PALETTE = [
    [0, 0, 255], [51, 51, 255], [102, 102, 255], [153, 153, 255], [204, 204, 255]
  ];
  
  const DISCARD_PALETTE = [
    [0, 255, 0], [51, 255, 51], [102, 255, 102], [153, 255, 153], [204, 255, 204]
  ];
  
  const FLUSH_PALETTE = [
    [255, 255, 0], [255, 255, 51], [255, 255, 102], [255, 255, 153], [255, 255, 204]
  ];

  const CPU_PALETTE = [
    [255, 0, 0], [255, 127, 0], [255, 255, 0], [0, 255, 0],
    [0, 0, 255], [75, 0, 130], [139, 0, 255], [255, 0, 255],
    [31, 119, 180], [255, 127, 14], [44, 160, 44], [214, 39, 40],
    [148, 103, 189], [140, 86, 75], [227, 119, 194], [127, 127, 127],
    [188, 189, 34], [23, 190, 207], [174, 199, 232], [255, 187, 120]
  ];

  // 색상 매핑
  let blockWriteMapping: Record<string, number[]> = {};
  let blockReadMapping: Record<string, number[]> = {};
  let blockDiscardMapping: Record<string, number[]> = {};
  let blockFlushMapping: Record<string, number[]> = {};
  let cpuColorMapping: Record<string, number[]> = {};
  let writePaletteIndex = 0;
  let readPaletteIndex = 0;
  let discardPaletteIndex = 0;
  let flushPaletteIndex = 0;

  function hexToRgb(hex: string): number[] {
    const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
    return result ? [
      parseInt(result[1], 16),
      parseInt(result[2], 16),
      parseInt(result[3], 16)
    ] : [Math.random() * 255, Math.random() * 255, Math.random() * 255];
  }

  // ⚡ 통합 색상 캐시 (모든 범례를 하나의 Map으로 관리)
  const colorCache = new Map<string, number[]>();
  
  // ⚡ 최적화: 색상 조회 함수 (Map으로 O(1) 조회)
  function getColorForLegend(legend: string): number[] {
    // 캐시 체크 (가장 먼저)
    const cached = colorCache.get(legend);
    if (cached) return cached;
    
    let color: number[];
    
    if (typeof legend !== 'string') {
      color = [Math.random() * 255, Math.random() * 255, Math.random() * 255];
      colorCache.set(legend, color);
      return color;
    }

    // CPU 레전드 처리
    if (legendKey === 'cpu') {
      const cpuNum = parseInt(legend);
      if (!isNaN(cpuNum)) {
        color = CPU_PALETTE[cpuNum % CPU_PALETTE.length];
      } else {
        color = [Math.random() * 255, Math.random() * 255, Math.random() * 255];
      }
      colorCache.set(legend, color);
      return color;
    }

    // UFS 명령어 확인 (toLowerCase 한 번만 호출)
    const lowerLegend = legend.toLowerCase();
    if (lowerLegend.startsWith('0x')) {
      color = UFS_COMMAND_COLORS[lowerLegend];
      if (color) {
        colorCache.set(legend, color);
        return color;
      }
    }
    
    // Block I/O 타입별 색상
    const prefix = legend[0].toUpperCase();
    switch (prefix) {
      case 'W':
        color = WRITE_PALETTE[writePaletteIndex % WRITE_PALETTE.length];
        writePaletteIndex++;
        break;
      case 'R':
        color = READ_PALETTE[readPaletteIndex % READ_PALETTE.length];
        readPaletteIndex++;
        break;
      case 'D':
        color = DISCARD_PALETTE[discardPaletteIndex % DISCARD_PALETTE.length];
        discardPaletteIndex++;
        break;
      case 'F':
        color = FLUSH_PALETTE[flushPaletteIndex % FLUSH_PALETTE.length];
        flushPaletteIndex++;
        break;
      default:
        color = [Math.random() * 255, Math.random() * 255, Math.random() * 255];
    }
    
    colorCache.set(legend, color);
    return color;
  }

  // ⚡ 범례 캐시
  let legendItemsCache: Record<string, number[]> = {};
  let legendCacheKey: string = '';

  // ⚡ 최적화: 범례 아이템 가져오기 (캐싱)
  function getLegendItems(): Record<string, number[]> {
    if (!transformedDataCache || transformedDataCache.length === 0) {
      return {};
    }
    
    // ⚡ 캐시 키 생성 (데이터 길이 + legendKey)
    const cacheKey = `${transformedDataCache.length}-${legendKey}`;
    if (cacheKey === legendCacheKey && Object.keys(legendItemsCache).length > 0) {
      return legendItemsCache;
    }
    
    const legendItems: Record<string, number[]> = {};
    
    // ⚡ 최적화: Set으로 중복 체크 (더 빠름)
    const seenLegends = new Set<string>();
    
    for (let i = 0; i < transformedDataCache.length; i++) {
      const item = transformedDataCache[i];
      if (item.legend && !seenLegends.has(item.legend)) {
        seenLegends.add(item.legend);
        legendItems[item.legend] = item.color;
      }
    }
    
    // ⚡ 정렬 (캐시에 저장)
    const sortedKeys = Object.keys(legendItems).sort((a, b) => {
      const prefixOrder: any = {
        'R': 1, // Read
        'W': 2, // Write
        'D': 3, // Discard
        'F': 4, // Flush
        '0': 5  // 0x로 시작하는 UFS 명령어
      };
      
      const prefixA = a.toLowerCase().startsWith('0x') ? '0' : a[0].toUpperCase();
      const prefixB = b.toLowerCase().startsWith('0x') ? '0' : b[0].toUpperCase();
      
      if (prefixA !== prefixB) {
        const orderA = prefixOrder[prefixA] || 999;
        const orderB = prefixOrder[prefixB] || 999;
        return orderA - orderB;
      }
      
      if (a.toLowerCase().startsWith('0x') && b.toLowerCase().startsWith('0x')) {
        const valueA = parseInt(a.slice(2), 16);
        const valueB = parseInt(b.slice(2), 16);
        return valueA - valueB;
      }
      
      const numA = parseInt((a.match(/\d+/) || ['0'])[0]);
      const numB = parseInt((b.match(/\d+/) || ['0'])[0]);
      return numA - numB;
    });
    
    const sortedLegendItems: Record<string, number[]> = {};
    for (let i = 0; i < sortedKeys.length; i++) {
      const key = sortedKeys[i];
      sortedLegendItems[key] = legendItems[key];
    }
    
    // ⚡ 캐시 저장
    legendItemsCache = sortedLegendItems;
    legendCacheKey = cacheKey;
    
    return sortedLegendItems;
  }

  // ⚡ 최적화: action 필터 로직을 별도 함수로 분리
  function shouldIncludeItem(action: string): boolean {
    if (actionFilter) {
      if (actionFilter === 'd') {
        return action === 'send_req' || action === 'block_rq_issue';
      } else if (actionFilter === 'c') {
        return action === 'complete_rsp' || action === 'block_rq_complete';
      }
    } else if (ycolumn === 'dtoc' || ycolumn === 'ctoc') {
      return action === 'complete_rsp' || action === 'block_rq_complete';
    } else if (ycolumn === 'lba' || ycolumn === 'sector') {
      return action === 'send_req' || action === 'block_rq_issue';
    } else {
      return action === 'send_req' || action === 'block_rq_issue';
    }
    return true;
  }

  // ⚡ 최적화: 데이터를 deck.gl 형식으로 변환 (캐싱 및 필터링 적용)
  function transformDataForDeck(rawData: any[]) {
    const transformStart = performance.now();
    
    if (!rawData || rawData.length === 0) {
      transformedDataCache = [];
      lastDataLength = 0;
      return [];
    }
    
    // ⚡ 캐시 히트: 데이터가 변경되지 않았으면 캐시 반환
    if (rawData === prevData && transformedDataCache.length > 0) {
      console.log('[deck.gl] 캐시 사용, 변환 건너뜀');
      return transformedDataCache;
    }
    
    // ⚡ 최적화: 단일 패스로 필터링+변환 + 배치 처리
    const result: any[] = [];
    const dataLength = rawData.length;
    
    // ⚡ 대용량 데이터 최적화: 배치 처리로 UI 블로킹 방지
    const BATCH_SIZE = 50000; // 5만개씩 처리
    let processedCount = 0;
    
    // 미리 키 캐싱 (객체 속성 접근 최소화)
    const xKey = xAxisKey;
    const yKey = yAxisKey;
    const lKey = legendKey;
    
    for (let i = 0; i < dataLength; i++) {
      const item = rawData[i];
      
      // ⚡ 직접 접근 (구조분해 제거)
      const xValue = item[xKey];
      const yValue = item[yKey];
      
      // ⚡ 빠른 타입 체크 (typeof 최소화)
      const x = typeof xValue === 'bigint' ? Number(xValue) : xValue;
      const y = typeof yValue === 'bigint' ? Number(yValue) : yValue;
      
      // 유효하지 않은 좌표는 건너뜀
      if (x !== x || y !== y) continue; // isNaN보다 빠름
      
      // ⚡ action 필터링 (문자열 연산 최소화)
      const action = item.action || item.command;
      if (!action || !shouldIncludeItem(action)) continue;
      
      // ⚡ 범례 문자열 변환 (한 번만)
      const legendStr = String(item[lKey]);
      
      // ⚡ 변환 및 추가 (스프레드 연산자 제거 - 가장 느림)
      result.push({
        position: [x, y],
        originalX: x,
        originalY: y,
        color: getColorForLegend(legendStr),
        legend: legendStr,
        // 필요한 필드만 복사
        action: item.action,
        command: item.command,
        time: item.time,
        lba: item.lba,
        sector: item.sector,
        cpu: item.cpu,
        qd: item.qd,
        size: item.size
      });
      
      // ⚡ 배치 진행 로그 (대용량 데이터만)
      processedCount++;
      if (dataLength > 100000 && processedCount % BATCH_SIZE === 0) {
        console.log(`[deck.gl] 진행: ${processedCount}/${dataLength} (${((processedCount/dataLength)*100).toFixed(1)}%)`);
      }
    }
    
    transformedDataCache = result;
    lastDataLength = dataLength;
    
    const transformEnd = performance.now();
    console.log(`⚡ [Performance] transformDataForDeck: ${(transformEnd - transformStart).toFixed(2)}ms, 원본: ${dataLength}, 필터링됨: ${result.length}`);
    
    return transformedDataCache;
  }

  // deck.gl 레이어 생성
  function createLayers(transformedData: any[]) {
    console.log('[deck.gl] createLayers 호출, data count:', transformedData.length);
    if (transformedData.length > 0) {
      console.log('[deck.gl] 첫 포인트 확인:', transformedData[0].position);
    }
    
    const layer = new ScatterplotLayer({
      id: 'scatter-plot',
      data: transformedData,
      pickable: true,
      opacity: 0.8,
      stroked: true,
      filled: true,
      radiusScale: 1,
      radiusMinPixels: symbolSize,
      radiusMaxPixels: symbolSize * 3,
      lineWidthMinPixels: 0.5,
      getPosition: (d: any) => {
        const pos = d.position;
        if (!Array.isArray(pos) || pos.length !== 2) {
          console.error('[deck.gl] 잘못된 position:', pos);
          return [0, 0];
        }
        return pos;
      },
      getFillColor: (d: any) => [...d.color, 204] as [number, number, number, number],
      getLineColor: (d: any) => [...d.color.map((c: number) => Math.max(0, c - 30)), 255] as [number, number, number, number],
      getRadius: symbolSize,
      updateTriggers: {
        getRadius: symbolSize,
        getFillColor: data,
        getLineColor: data,
        radiusMinPixels: symbolSize,
        radiusMaxPixels: symbolSize * 3
      }
    });
    
    console.log('[deck.gl] 레이어 생성 완료:', layer);
    return [layer];
  }

  // 데이터 범위 계산
  function calculateDataBounds(transformedData: any[]) {
    if (!transformedData || transformedData.length === 0) {
      return { xMin: 0, xMax: 100, yMin: 0, yMax: 100 };
    }

    let xMin = Infinity, xMax = -Infinity;
    let yMin = Infinity, yMax = -Infinity;

    transformedData.forEach((d: any) => {
      const [x, y] = d.position;
      if (x < xMin) xMin = x;
      if (x > xMax) xMax = x;
      if (y < yMin) yMin = y;
      if (y > yMax) yMax = y;
    });

    // ⚡ CPU 차트는 Y축을 -1~8로 고정
    if (isCpuAxis) {
      yMin = -1;
      yMax = 8;
    }

    return { xMin, xMax, yMin, yMax };
  }

  // deck.gl 초기화
  function initializeDeck() {
    const hasTable = table !== null && table !== undefined;
    const hasData = data && data.length > 0;
    const dataLength = hasTable ? (table.numRows ?? 0) : (data?.length ?? 0);
    
    console.log('[deck.gl] initializeDeck 호출, deckContainer:', !!deckContainer, 'deckInstance:', !!deckInstance, 'hasTable:', hasTable, 'hasData:', hasData, 'dataLength:', dataLength);
    
    if (!deckContainer || deckInstance) {
      console.log('[deck.gl] 초기화 건너뜀 - deckContainer:', !!deckContainer, 'deckInstance:', !!deckInstance);
      return;
    }
    
    if (!hasTable && !hasData) {
      console.log('[deck.gl] table과 data 모두 없어 초기화 지연');
      return;
    }

    // 로딩 시작
    isInitializing = true;
    loadingProgress = 0;
    loadingMessage = '데이터 변환 중...';

    // ⚡ table이 있으면 toArray() 호출, 없으면 data 사용
    const arrayData = hasTable ? table.toArray() : data;
    loadingProgress = 20;
    
    const transformedData = transformDataForDeck(arrayData);
    console.log('[deck.gl] transformedData 길이:', transformedData.length);
    loadingProgress = 60;
    
    if (transformedData.length === 0) {
      console.error('[deck.gl] ❌ transformedData가 비어있습니다! 초기화를 중단합니다.');
      isInitializing = false;
      return;
    }

    // 변환된 데이터를 캐시에 저장 (리사이즈 시 사용)
    transformedDataCache = transformedData;
    lastDataLength = transformedData.length;
    
    const bounds = calculateDataBounds(transformedData);
    console.log('[deck.gl] bounds:', bounds);
    
    // 초기 뷰 설정
    const rangeX = bounds.xMax - bounds.xMin;
    const rangeY = bounds.yMax - bounds.yMin;
    
    const width = deckContainer.clientWidth;
    const height = deckContainer.clientHeight;
    
    // 컨테이너 크기가 0이면 초기화 실패
    if (width === 0 || height === 0) {
      initRetryCount++;
      if (initRetryCount >= maxInitRetries) {
        console.warn('[deck.gl] ⚠️ 초기화 재시도 최대 횟수 초과, 초기화 중단');
        initRetryCount = 0;
        return;
      }
      console.warn(`[deck.gl] ⚠️ 컨테이너 크기가 0입니다! width: ${width}, height: ${height} (재시도 ${initRetryCount}/${maxInitRetries})`);
      setTimeout(() => initializeDeck(), 100);
      return;
    }
    
    // 초기화 성공 시 재시도 카운터 리셋
    initRetryCount = 0;
    
    // 상태 변수에 저장
    containerWidth = width;
    containerHeight = height;
    dataBounds = bounds;
    originalDataBounds = { ...bounds }; // 원본 범위 저장
    
    // 실제 차트 영역 크기 (패딩 제외)
    const actualChartWidth = width - PADDING_LEFT - PADDING_RIGHT;
    const actualChartHeight = height - PADDING_TOP - PADDING_BOTTOM;
    
    // 데이터를 픽셀 공간으로 변환 (정규화)
    const scaledData = transformedData.map(d => {
      const normalizedX = (d.position[0] - bounds.xMin) / rangeX;
      const normalizedY = (d.position[1] - bounds.yMin) / rangeY;
      
      const x = PADDING_LEFT + normalizedX * actualChartWidth;
      const y = PADDING_TOP + (1 - normalizedY) * actualChartHeight; // Y축 반전
      
      return {
        ...d,
        position: [x, y]
      };
    });
    
    console.log('[deck.gl] 스케일링된 데이터 샘플 (처음 3개):', scaledData.slice(0, 3));
    console.log('[deck.gl] 스케일링된 position 범위:', {
      x: [PADDING_LEFT, PADDING_LEFT + actualChartWidth],
      y: [PADDING_TOP, PADDING_TOP + actualChartHeight]
    });
    
    // 픽셀 좌표계에서 뷰 설정
    viewState = {
      target: [width / 2, height / 2, 0],
      zoom: 0,
      minZoom: -5,
      maxZoom: 5,
      transitionDuration: 0
    };

    deckInstance = new Deck({
      parent: deckContainer,
      views: [new OrthographicView()],
      initialViewState: viewState,
      controller: false, // 모든 인터랙션 비활성화 (드래그, 줌 등)

      layers: createLayers(scaledData),
      onViewStateChange: ({viewState: newViewState}: any) => {
        // 차트 영역 경계
        const chartLeft = PADDING_LEFT;
        const chartRight = containerWidth - PADDING_RIGHT;
        const chartTop = PADDING_TOP;
        const chartBottom = containerHeight - PADDING_BOTTOM;
        const chartCenterX = (chartLeft + chartRight) / 2;
        const chartCenterY = (chartTop + chartBottom) / 2;
        const chartWidth = chartRight - chartLeft;
        const chartHeight = chartBottom - chartTop;
        
        // zoom 레벨에 따른 보이는 영역 크기 계산
        const scale = Math.pow(2, newViewState.zoom);
        const viewWidth = containerWidth / scale;
        const viewHeight = containerHeight / scale;
        
        // target이 이동 가능한 범위 계산 (zoom에 따라 변함)
        // 차트 영역을 벗어나지 않도록 제한
        const halfViewWidth = viewWidth / 2;
        const halfViewHeight = viewHeight / 2;
        
        // target의 최소/최대 범위 (차트 영역 내에서만)
        const minTargetX = chartLeft + halfViewWidth;
        const maxTargetX = chartRight - halfViewWidth;
        const minTargetY = chartTop + halfViewHeight;
        const maxTargetY = chartBottom - halfViewHeight;
        
        // zoom이 너무 커서 차트가 뷰보다 작아지면 중앙 고정
        let constrainedTargetX = newViewState.target[0];
        let constrainedTargetY = newViewState.target[1];
        
        if (viewWidth >= chartWidth) {
          // 뷰가 차트보다 크거나 같으면 중앙 고정
          constrainedTargetX = chartCenterX;
        } else {
          // 뷰가 차트보다 작으면 범위 제한
          constrainedTargetX = Math.max(minTargetX, Math.min(maxTargetX, newViewState.target[0]));
        }
        
        if (viewHeight >= chartHeight) {
          // 뷰가 차트보다 크거나 같으면 중앙 고정
          constrainedTargetY = chartCenterY;
        } else {
          // 뷰가 차트보다 작으면 범위 제한
          constrainedTargetY = Math.max(minTargetY, Math.min(maxTargetY, newViewState.target[1]));
        }
        
        viewState = {
          ...newViewState,
          target: [constrainedTargetX, constrainedTargetY, newViewState.target[2]]
        };
      },
      getTooltip: ({object}: any) => {
        if (object) {
          // Y축 포맷 결정: latency는 소수점 3자리, 나머지는 0자리
          const yFormat = isLatencyAxis ? object.originalY.toFixed(3) : object.originalY.toFixed(0);
          
          return {
            html: `<div style="background: rgba(0, 0, 0, 0.8); color: white; padding: 8px 12px; border-radius: 4px; font-size: 12px; line-height: 1.5;">
              <strong>${legendKey}: ${object.legend}</strong><br/>
              ${xAxisName}: ${object.originalX.toFixed(2)}<br/>
              ${yAxisName}: ${yFormat}
            </div>`,
            style: {
              backgroundColor: 'transparent',
              fontSize: '12px',
              zIndex: 1000
            }
          };
        }
        return null;
      }
    } as any);
    
    console.log('[deck.gl] Deck instance 생성 완료, layers:', deckInstance.props.layers?.length ?? 0);
    
    // 로딩 완료
    loadingProgress = 100;
    loadingMessage = '완료';
    setTimeout(() => {
      isInitializing = false;
    }, 300);
  }

  // 차트 리사이즈 처리
  function resizeChart(newWidth: number, newHeight: number) {
    if (!deckInstance || !transformedDataCache || transformedDataCache.length === 0) {
      console.log('[deck.gl] 리사이즈 건너뜀 - deckInstance 또는 캐시된 데이터 없음');
      return;
    }

    console.log('[deck.gl] 차트 리사이즈 시작 (width:', newWidth, 'height:', newHeight, ')');

    // 컨테이너 크기 업데이트
    containerWidth = newWidth;
    containerHeight = newHeight;

    // 데이터 범위는 동일하게 유지
    const rangeX = dataBounds.xMax - dataBounds.xMin;
    const rangeY = dataBounds.yMax - dataBounds.yMin;

    // 새로운 차트 영역 크기
    const actualChartWidth = newWidth - PADDING_LEFT - PADDING_RIGHT;
    const actualChartHeight = newHeight - PADDING_TOP - PADDING_BOTTOM;

    // 캐시된 데이터를 새 크기로 다시 스케일링
    const rescaledData = transformedDataCache.map(d => {
      const normalizedX = (d.position[0] - dataBounds.xMin) / rangeX;
      const normalizedY = (d.position[1] - dataBounds.yMin) / rangeY;

      const x = PADDING_LEFT + normalizedX * actualChartWidth;
      const y = PADDING_TOP + (1 - normalizedY) * actualChartHeight;

      return {
        ...d,
        position: [x, y]
      };
    });

    // viewState 중앙 위치 업데이트
    viewState = {
      ...viewState,
      target: [newWidth / 2, newHeight / 2, 0]
    };

    // deck.gl 업데이트
    deckInstance.setProps({
      width: newWidth,
      height: newHeight,
      initialViewState: viewState,
      layers: createLayers(rescaledData)
    });

    console.log('[deck.gl] 차트 리사이즈 완료');
  }

  // 차트 업데이트
  function updateChart() {
    const hasTable = table !== null && table !== undefined;
    const hasData = data && data.length > 0;
    const dataSource = hasTable ? table : data;
    
    console.log('[deck.gl] updateChart 호출, deckInstance:', !!deckInstance, 'hasTable:', hasTable, 'hasData:', hasData);
    
    if (!deckInstance || (!hasTable && !hasData)) {
      console.log('[deck.gl] 업데이트 건너뜀 - 데이터 없음');
      return;
    }
    
    try {
      // ⚡ table이 있으면 toArray() 호출, 없으면 data 사용
      const arrayData = hasTable ? table.toArray() : data;
      const transformedData = transformDataForDeck(arrayData);
      
      // 변환된 데이터를 캐시에 저장 (리사이즈 시 사용)
      transformedDataCache = transformedData;
      lastDataLength = transformedData.length;
      
      const bounds = calculateDataBounds(transformedData);
      dataBounds = bounds;
      
      // 데이터 범위 계산
      const rangeX = bounds.xMax - bounds.xMin;
      const rangeY = bounds.yMax - bounds.yMin;
      
      // 차트 영역 크기
      const actualChartWidth = containerWidth - PADDING_LEFT - PADDING_RIGHT;
      const actualChartHeight = containerHeight - PADDING_TOP - PADDING_BOTTOM;
      
      // 데이터를 픽셀 공간으로 변환
      const scaledData = transformedData.map(d => {
        const normalizedX = (d.position[0] - bounds.xMin) / rangeX;
        const normalizedY = (d.position[1] - bounds.yMin) / rangeY;
        
        const x = PADDING_LEFT + normalizedX * actualChartWidth;
        const y = PADDING_TOP + (1 - normalizedY) * actualChartHeight;
        
        return {
          ...d,
          position: [x, y]
        };
      });
      
      const layers = createLayers(scaledData);
      console.log('[deck.gl] 새 layers 생성, data count:', scaledData.length);
      
      deckInstance.setProps({
        layers
      });
    } catch (error) {
      console.error('차트 업데이트 중 오류 발생:', error);
    }
  }

  // 타이틀 관련 함수
  function openTitleDialog() {
    inputTitle = chartTitle;
    showTitleDialog = true;
  }
  
  function applyTitleChange() {
    chartTitle = inputTitle;
    showTitleDialog = false;
  }

  // 포인트 크기 관련 함수
  function openSymbolSizeDialog() {
    inputSymbolSize = symbolSize;
    showSymbolSizeDialog = true;
  }

  function applySymbolSizeChange() {
    symbolSize = inputSymbolSize;
    updateChart();
    showSymbolSizeDialog = false;
  }

  // X축 범위 설정 함수
  function openXAxisRangeDialog() {
    const bounds = calculateDataBounds(transformDataForDeck(data));
    inputXMin = bounds.xMin;
    inputXMax = bounds.xMax;
    showXAxisRangeDialog = true;
  }
  
  function applyXAxisRange() {
    if (deckInstance && inputXMin < inputXMax) {
      // dataBounds 업데이트 (X축만)
      dataBounds = {
        ...dataBounds,
        xMin: inputXMin,
        xMax: inputXMax
      };
      
      const centerX = (inputXMax + inputXMin) / 2;
      const centerY = viewState.target[1];
      const rangeX = inputXMax - inputXMin;
      
      viewState = {
        ...viewState,
        target: [centerX, centerY, 0],
        zoom: Math.log2(deckContainer.clientWidth / rangeX) - 1,
        transitionDuration: 300
      };
      
      deckInstance.setProps({
        initialViewState: viewState
      });

      $filtertrace = {
        zoom_column: ycolumn,
        from_time: inputXMin,
        to_time: inputXMax,
        from_lba: dataBounds.yMin,
        to_lba: dataBounds.yMax
      };
      
      console.log('[deck.gl] X축 범위 적용, filtertrace 업데이트:', $filtertrace);
    }
    showXAxisRangeDialog = false;
  }

  // Y축 범위 설정 함수
  function openYAxisRangeDialog() {
    // ⚡ CPU 차트는 Y축 범위 고정 (-1~8)
    if (isCpuAxis) {
      console.log('[deck.gl] CPU 차트는 Y축 범위가 -1~8로 고정되어 있습니다.');
      return;
    }
    
    const bounds = calculateDataBounds(transformDataForDeck(data));
    inputYMin = bounds.yMin;
    inputYMax = bounds.yMax;
    showYAxisRangeDialog = true;
  }
  
  function applyYAxisRange() {
    if (deckInstance && inputYMin < inputYMax) {
      // dataBounds 업데이트 (Y축만)
      dataBounds = {
        ...dataBounds,
        yMin: inputYMin,
        yMax: inputYMax
      };
      
      const centerX = viewState.target[0];
      const centerY = (inputYMax + inputYMin) / 2;
      const rangeY = inputYMax - inputYMin;
      
      viewState = {
        ...viewState,
        target: [centerX, centerY, 0],
        zoom: Math.log2(deckContainer.clientHeight / rangeY) - 1,
        transitionDuration: 300
      };
      
      deckInstance.setProps({
        initialViewState: viewState
      });

      $filtertrace = {
        zoom_column: ycolumn,
        from_time: dataBounds.xMin,
        to_time: dataBounds.xMax,
        from_lba: inputYMin,
        to_lba: inputYMax
      };
      
      console.log('[deck.gl] Y축 범위 적용, filtertrace 업데이트:', $filtertrace);
    }
    showYAxisRangeDialog = false;
  }

  // 줌 리셋
  function resetZoom() {
    // 원본 데이터 범위로 복원
    dataBounds = { ...originalDataBounds };
    
    // filtertrace 스토어 초기화 - 다른 차트들도 리셋되도록
    $filtertrace = {
      zoom_column: ycolumn,
      from_time: originalDataBounds.xMin,
      to_time: originalDataBounds.xMax,
      from_lba: originalDataBounds.yMin,
      to_lba: originalDataBounds.yMax
    };
    
    console.log('[deck.gl] filtertrace 리셋:', $filtertrace);
    
    // 데이터 범위가 변경되었으므로 차트를 다시 렌더링
    updateChart();
    
    // 픽셀 좌표계에서는 단순히 중앙으로 리셋
    viewState = {
      target: [containerWidth / 2, containerHeight / 2, 0],
      zoom: 0,
      minZoom: -5,
      maxZoom: 5,
      transitionDuration: 300
    };
    
    if (deckInstance) {
      deckInstance.setProps({
        initialViewState: viewState
      });
    }
  }

  // 드래그 선택 영역으로 줌
  function zoomToSelection() {
    if (!isDragging) return;
    
    const x1 = Math.min(dragStartX, dragEndX);
    const x2 = Math.max(dragStartX, dragEndX);
    const y1 = Math.min(dragStartY, dragEndY);
    const y2 = Math.max(dragStartY, dragEndY);
    
    // 선택 영역이 너무 작으면 무시
    if (Math.abs(x2 - x1) < 10 || Math.abs(y2 - y1) < 10) {
      isDragging = false;
      return;
    }
    
    // 픽셀 좌표를 데이터 좌표로 변환
    const actualChartWidth = containerWidth - PADDING_LEFT - PADDING_RIGHT;
    const actualChartHeight = containerHeight - PADDING_TOP - PADDING_BOTTOM;
    
    // 선택 영역의 픽셀 좌표를 차트 영역 내 정규화된 좌표로 변환
    const normX1 = Math.max(0, Math.min(1, (x1 - PADDING_LEFT) / actualChartWidth));
    const normX2 = Math.max(0, Math.min(1, (x2 - PADDING_LEFT) / actualChartWidth));
    const normY1 = Math.max(0, Math.min(1, (y1 - PADDING_TOP) / actualChartHeight));
    const normY2 = Math.max(0, Math.min(1, (y2 - PADDING_TOP) / actualChartHeight));
    
    // 정규화된 좌표를 데이터 좌표로 변환 (Y축은 반전되어 있음)
    const dataXMin = dataBounds.xMin + normX1 * (dataBounds.xMax - dataBounds.xMin);
    const dataXMax = dataBounds.xMin + normX2 * (dataBounds.xMax - dataBounds.xMin);
    const dataYMax = dataBounds.yMax - normY1 * (dataBounds.yMax - dataBounds.yMin); // Y축 반전
    const dataYMin = dataBounds.yMax - normY2 * (dataBounds.yMax - dataBounds.yMin); // Y축 반전
    
    console.log('[deck.gl] 선택 영역:', {
      pixel: { x1, x2, y1, y2 },
      normalized: { normX1, normX2, normY1, normY2 },
      data: { dataXMin, dataXMax, dataYMin, dataYMax }
    });
    
    // 새로운 데이터 범위로 업데이트
    dataBounds = {
      xMin: dataXMin,
      xMax: dataXMax,
      yMin: dataYMin,
      yMax: dataYMax
    };
    
    // filtertrace 스토어 업데이트 - 다른 차트들도 같이 줌되도록
    $filtertrace = {
      zoom_column: ycolumn,
      from_time: dataXMin,
      to_time: dataXMax,
      from_lba: dataYMin,
      to_lba: dataYMax
    };
    
    console.log('[deck.gl] filtertrace 업데이트:', $filtertrace);
    
    // 데이터 범위가 변경되었으므로 차트를 다시 렌더링해야 함
    // updateChart()를 호출하여 새로운 범위로 데이터를 다시 스케일링
    updateChart();
    
    // 뷰를 중앙과 zoom 0으로 리셋 (데이터가 이미 새 범위로 스케일링됨)
    viewState = {
      target: [containerWidth / 2, containerHeight / 2, 0],
      zoom: 0,
      minZoom: -5,
      maxZoom: 5,
      transitionDuration: 300
    };
    
    if (deckInstance) {
      deckInstance.setProps({
        initialViewState: viewState
      });
    }
    
    isDragging = false;
  }

  // 마우스 이벤트 핸들러
  function handleMouseDown(event: MouseEvent) {
    // Shift 키를 누른 상태에서만 드래그 선택 활성화
    if (isShiftPressed && event.button === 0) {
      const rect = deckContainer.getBoundingClientRect();
      dragStartX = event.clientX - rect.left;
      dragStartY = event.clientY - rect.top;
      dragEndX = dragStartX;
      dragEndY = dragStartY;
      isDragging = true;
      
      // deck.gl의 기본 pan 동작 비활성화
      if (deckInstance) {
        deckInstance.setProps({
          controller: false
        });
      }
    }
  }

  function handleMouseMove(event: MouseEvent) {
    if (isDragging) {
      const rect = deckContainer.getBoundingClientRect();
      dragEndX = event.clientX - rect.left;
      dragEndY = event.clientY - rect.top;
    }
  }

  function handleMouseUp(event: MouseEvent) {
    if (isDragging) {
      zoomToSelection();
      
      // deck.gl의 controller 다시 활성화
      if (deckInstance) {
        deckInstance.setProps({
          controller: true
        });
      }
    }
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      isShiftPressed = true;
    }
  }

  function handleKeyUp(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      isShiftPressed = false;
      if (isDragging) {
        isDragging = false;
        // deck.gl의 controller 다시 활성화
        if (deckInstance) {
          deckInstance.setProps({
            controller: true
          });
        }
      }
    }
  }

  // 라이프사이클
  onMount(() => {
    console.log('[deck.gl] onMount 실행, data length:', data?.length ?? 0);
    
    // 데이터가 있으면 DOM이 완전히 렌더링된 후 초기화
    const hasTable = table !== null && table !== undefined;
    const hasData = data && data.length > 0;
    
    if (hasTable || hasData) {
      console.log('[deck.gl] onMount - 데이터 있음, 초기화 예약 (hasTable:', hasTable, 'hasData:', hasData, ')');
      // requestAnimationFrame을 사용하여 DOM 렌더링 완료 후 초기화
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          // 컨테이너가 보이는 경우에만 초기화
          if (deckContainer && deckContainer.clientWidth > 0 && deckContainer.clientHeight > 0) {
            console.log('[deck.gl] onMount - 컨테이너 보이므로 초기화 실행');
            initializeDeck();
          } else {
            console.log('[deck.gl] onMount - 컨테이너 보이지 않음, ResizeObserver가 감지할 때까지 대기');
          }
        });
      });
    }
    
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const width = entry.contentRect.width;
        const height = entry.contentRect.height;
        
        // 컨테이너가 보이지 않다가 보이게 될 때 (크기가 0에서 변경)
        const hasTable = table !== null && table !== undefined;
        const hasData = data && data.length > 0;
        
        if (width > 0 && height > 0 && !deckInstance && (hasTable || hasData)) {
          console.log('[deck.gl] ResizeObserver - 컨테이너가 보이게 됨 (width:', width, 'height:', height, '), 차트 초기화 시작');
          initializeDeck();
        }
        // 이미 초기화된 경우 차트 리사이즈
        else if (deckInstance && width > 0 && height > 0) {
          // 크기가 실제로 변경되었는지 확인
          if (width !== containerWidth || height !== containerHeight) {
            console.log('[deck.gl] ResizeObserver - 차트 리사이즈 (width:', width, 'height:', height, ')');
            resizeChart(width, height);
          }
        }
      }
    });
    
    if (deckContainer) {
      resizeObserver.observe(deckContainer);
    }
    
    // 키보드 이벤트 리스너 등록
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
  });

  onDestroy(() => {
    if (deckInstance) {
      deckInstance.finalize();
      deckInstance = null;
    }
    
    if (resizeObserver) {
      resizeObserver.disconnect();
    }
    
    // 키보드 이벤트 리스너 제거
    window.removeEventListener('keydown', handleKeyDown);
    window.removeEventListener('keyup', handleKeyUp);
  });

  // 데이터 변경 감지 - 데이터 길이로 변경 감지하여 무한 루프 방지
  $effect(() => {
    // ⚡ table 또는 data 중 하나라도 있으면 처리
    const hasTable = table !== null && table !== undefined;
    const hasData = data && data.length > 0;
    const dataLength = hasTable ? (table.numRows ?? 0) : (data?.length ?? 0);
    const currentDataSource = hasTable ? table : data;
    
    console.log('[deck.gl] $effect 실행, hasTable:', hasTable, 'hasData:', hasData, 'dataLength:', dataLength, 'deckInstance:', !!deckInstance);
    
    // table도 data도 없으면 무시
    if (!hasTable && !hasData) {
      console.log('[deck.gl] $effect 건너뜀 - table과 data 모두 없음');
      return;
    }
    
    // 컨테이너가 보이지 않으면 초기화 건너뜀 (display:none 상태)
    if (deckContainer && (deckContainer.clientWidth === 0 || deckContainer.clientHeight === 0)) {
      console.log('[deck.gl] $effect 건너뜀 - 컨테이너가 보이지 않음 (display:none 또는 크기 0)');
      return;
    }
    
    // deckInstance가 없으면 초기화 시도
    if (!deckInstance && deckContainer) {
      console.log('[deck.gl] deckInstance 없음, 초기화 시도 (table:', hasTable, ')');
      prevData = currentDataSource; // ⚡ initializeDeck 호출 전에 미리 설정
      initializeDeck();
      return; // ⚡ 초기화 후 바로 리턴 (updateChart 호출 방지)
    }
    
    // 데이터가 실제로 변경되었는지 확인
    if (deckInstance && currentDataSource !== prevData) {
      console.log('[deck.gl] 데이터 변경 감지, 차트 업데이트 중...', dataLength);
      prevData = currentDataSource;
      updateChart();
    }
  });

  // filtertrace 변경 감지 - 다른 차트에서 줌/리셋할 때 현재 차트도 업데이트
  $effect(() => {
    // filtertrace 스토어 변경 감지
    const ft = $filtertrace;
    
    // 초기화되지 않았으면 무시
    if (!deckInstance || !originalDataBounds) {
      return;
    }
    
    // 데이터가 없으면 무시
    if (!data || data.length === 0) {
      return;
    }
    
    // filtertrace 값이 유효하지 않으면 무시 (undefined 체크)
    if (ft.xmin === undefined || ft.xmax === undefined) {
      console.log('[deck.gl] filtertrace 값이 유효하지 않음, 무시');
      return;
    }
    
    // Y축 값 확인
    const ftYMin = yAxisKey === 'qd' ? ft.qdmin : yAxisKey === 'addr' ? ft.addrmin : ft.latencymin;
    const ftYMax = yAxisKey === 'qd' ? ft.qdmax : yAxisKey === 'addr' ? ft.addrmax : ft.latencymax;
    
    if (ftYMin === undefined || ftYMax === undefined) {
      console.log('[deck.gl] filtertrace Y축 값이 유효하지 않음, 무시');
      return;
    }
    
    console.log('[deck.gl] filtertrace 변경 감지:', ft, 'yAxisKey:', yAxisKey);
    
    // 현재 dataBounds와 filtertrace가 다르면 업데이트
    const needsUpdate = 
      dataBounds.xMin !== ft.xmin ||
      dataBounds.xMax !== ft.xmax ||
      dataBounds.yMin !== ftYMin ||
      dataBounds.yMax !== ftYMax;
    
    if (needsUpdate) {
      console.log('[deck.gl] filtertrace와 다름, 차트 업데이트 필요');
      console.log('[deck.gl] 현재 dataBounds:', dataBounds);
      console.log('[deck.gl] 새로운 범위:', { xMin: ft.xmin, xMax: ft.xmax, yMin: ftYMin, yMax: ftYMax });
      
      // dataBounds 업데이트
      dataBounds = {
        xMin: ft.xmin,
        xMax: ft.xmax,
        yMin: ftYMin,
        yMax: ftYMax
      };
      
      // 차트 업데이트 (데이터 재스케일링)
      updateChart();
      
      // 뷰 상태 리셋 (중앙으로)
      viewState = {
        target: [containerWidth / 2, containerHeight / 2],
        zoom: 0
      };
    }
  });
</script>

<div class="scatter-chart-container">
  {#if chartTitle}
    <div class="chart-title">{chartTitle}</div>
  {/if}
  
  <!-- Loading Progress -->
  {#if isInitializing}
    <div class="loading-overlay">
      <div class="loading-content">
        <div class="loading-spinner"></div>
        <div class="loading-text">{loadingMessage}</div>
        <div class="progress-bar">
          <div class="progress-fill" style="width: {loadingProgress}%"></div>
        </div>
        <div class="progress-percent">{loadingProgress}%</div>
      </div>
    </div>
  {/if}
  
  <div class="chart-wrapper">
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div 
          class="chart-container-wrapper"
          onmousedown={handleMouseDown}
          onmousemove={handleMouseMove}
          onmouseup={handleMouseUp}
          onmouseleave={() => {
            if (isDragging) {
              isDragging = false;
              if (deckInstance) {
                deckInstance.setProps({ controller: true });
              }
            }
          }}
        >
          <div 
            bind:this={deckContainer} 
            class="deck-container"
          ></div>
          
          <!-- 드래그 선택 영역 표시 -->
          {#if isDragging}
            <div 
              class="selection-box"
              style="
                left: {Math.min(dragStartX, dragEndX)}px;
                top: {Math.min(dragStartY, dragEndY)}px;
                width: {Math.abs(dragEndX - dragStartX)}px;
                height: {Math.abs(dragEndY - dragStartY)}px;
              "
            ></div>
          {/if}
          
          <!-- 축과 그리드 오버레이 -->
          <svg class="axis-overlay" width={containerWidth} height={containerHeight}>
            <!-- 그리드 배경 -->
            <rect 
              x={PADDING_LEFT} 
              y={PADDING_TOP} 
              width={chartWidth} 
              height={chartHeight} 
              fill="none"
            />
            
            <!-- X축 -->
            <g class="x-axis">
              <line 
                x1={PADDING_LEFT} 
                y1={containerHeight - PADDING_BOTTOM} 
                x2={containerWidth - PADDING_RIGHT} 
                y2={containerHeight - PADDING_BOTTOM} 
                stroke="#333" 
                stroke-width="2"
              />
              
              {#each generateTicks(dataBounds.xMin, dataBounds.xMax, 10) as tick, i}
                {@const x = PADDING_LEFT + ((tick - dataBounds.xMin) / (dataBounds.xMax - dataBounds.xMin)) * chartWidth}
                <g>
                  <!-- 틱 마크 -->
                  <line 
                    x1={x} 
                    y1={containerHeight - PADDING_BOTTOM} 
                    x2={x} 
                    y2={containerHeight - PADDING_BOTTOM + 6} 
                    stroke="#333" 
                    stroke-width="1"
                  />
                  <!-- 그리드 라인 -->
                  <line 
                    x1={x} 
                    y1={PADDING_TOP} 
                    x2={x} 
                    y2={containerHeight - PADDING_BOTTOM} 
                    stroke="#e0e0e0" 
                    stroke-width="1"
                    stroke-dasharray="2,2"
                  />
                  <!-- 틱 레이블 -->
                  <text 
                    x={x} 
                    y={containerHeight - PADDING_BOTTOM + 20} 
                    text-anchor="middle" 
                    font-size="12" 
                    fill="#666"
                  >
                    {tick.toFixed(0)}
                  </text>
                </g>
              {/each}
              
              <!-- X축 레이블 -->
              <text 
                x={PADDING_LEFT + chartWidth / 2} 
                y={containerHeight - 10} 
                text-anchor="middle" 
                font-size="14" 
                font-weight="600"
                fill="#333"
              >
                {xAxisName}
              </text>
            </g>
            
            <!-- Y축 -->
            <g class="y-axis">
              <line 
                x1={PADDING_LEFT} 
                y1={PADDING_TOP} 
                x2={PADDING_LEFT} 
                y2={containerHeight - PADDING_BOTTOM} 
                stroke="#333" 
                stroke-width="2"
              />
              
              {#each generateTicks(dataBounds.yMin, dataBounds.yMax, 10) as tick, i}
                {@const y = containerHeight - PADDING_BOTTOM - ((tick - dataBounds.yMin) / (dataBounds.yMax - dataBounds.yMin)) * chartHeight}
                <g>
                  <!-- 틱 마크 -->
                  <line 
                    x1={PADDING_LEFT - 6} 
                    y1={y} 
                    x2={PADDING_LEFT} 
                    y2={y} 
                    stroke="#333" 
                    stroke-width="1"
                  />
                  <!-- 그리드 라인 -->
                  <line 
                    x1={PADDING_LEFT} 
                    y1={y} 
                    x2={containerWidth - PADDING_RIGHT} 
                    y2={y} 
                    stroke="#e0e0e0" 
                    stroke-width="1"
                    stroke-dasharray="2,2"
                  />
                  <!-- 틱 레이블 -->
                  <text 
                    x={PADDING_LEFT - 10} 
                    y={y + 4} 
                    text-anchor="end" 
                    font-size="12" 
                    fill="#666"
                  >
                    {isLatencyAxis ? tick.toFixed(3) : tick.toFixed(0)}
                  </text>
                </g>
              {/each}
              
              <!-- Y축 레이블 -->
              <text 
                x={20} 
                y={PADDING_TOP + chartHeight / 2} 
                text-anchor="middle" 
                font-size="14" 
                font-weight="600"
                fill="#333"
                transform="rotate(-90, 20, {PADDING_TOP + chartHeight / 2})"
              >
                {yAxisName}
              </text>
            </g>
          </svg>
        </div>
      </ContextMenu.Trigger>
    
    <ContextMenu.Content>
      <ContextMenu.Label>
        💡 Shift + Drag to zoom selected area
      </ContextMenu.Label>
      <ContextMenu.Separator />
      <ContextMenu.Item onclick={openTitleDialog}>
        Set Title
      </ContextMenu.Item>
      <ContextMenu.Item onclick={openSymbolSizeDialog}>
        Set Point Size
      </ContextMenu.Item>
      <ContextMenu.Item onclick={openXAxisRangeDialog}>
        Set X-Axis Range
      </ContextMenu.Item>
      <ContextMenu.Item onclick={openYAxisRangeDialog}>
        Set Y-Axis Range
      </ContextMenu.Item>
      <ContextMenu.Item onclick={resetZoom}>
        Reset Zoom
      </ContextMenu.Item>
    </ContextMenu.Content>
  </ContextMenu.Root>
  
  <!-- Legend -->
  {#if legendshow && Object.keys(getLegendItems()).length > 0}
    <div class="legend-container">
      <div class="legend-title">{legendKey}</div>
      <div class="legend-items">
        {#each Object.entries(getLegendItems()) as [legend, color]}
          <div class="legend-item">
            <span 
              class="legend-color" 
              style="background-color: rgb({color[0]}, {color[1]}, {color[2]})"
            ></span>
            <span class="legend-label">{legend}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
  </div>
</div>

<!-- Title Dialog -->
<Dialog.Root bind:open={showTitleDialog}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Set Chart Title</Dialog.Title>
    </Dialog.Header>
    <div class="space-y-4">
      <div>
        <Label for="title-input">Title</Label>
        <Input 
          id="title-input" 
          bind:value={inputTitle} 
          placeholder="Enter chart title"
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button onclick={applyTitleChange}>Apply</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Symbol Size Dialog -->
<Dialog.Root bind:open={showSymbolSizeDialog}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Set Point Size</Dialog.Title>
    </Dialog.Header>
    <div class="space-y-4">
      <div>
        <Label for="size-input">Size: {inputSymbolSizeArray[0]}</Label>
        <Slider 
          bind:value={inputSymbolSizeArray}
          min={1}
          max={20}
          step={1}
          onValueChange={(v) => inputSymbolSize = v[0]}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button onclick={applySymbolSizeChange}>Apply</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- X-Axis Range Dialog -->
<Dialog.Root bind:open={showXAxisRangeDialog}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Set X-Axis Range</Dialog.Title>
    </Dialog.Header>
    <div class="space-y-4">
      <div>
        <Label for="x-min-input">Min</Label>
        <Input 
          id="x-min-input" 
          type="number" 
          bind:value={inputXMin}
        />
      </div>
      <div>
        <Label for="x-max-input">Max</Label>
        <Input 
          id="x-max-input" 
          type="number" 
          bind:value={inputXMax}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button onclick={applyXAxisRange}>Apply</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Y-Axis Range Dialog -->
<Dialog.Root bind:open={showYAxisRangeDialog}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Set Y-Axis Range</Dialog.Title>
    </Dialog.Header>
    <div class="space-y-4">
      <div>
        <Label for="y-min-input">Min</Label>
        <Input 
          id="y-min-input" 
          type="number" 
          bind:value={inputYMin}
        />
      </div>
      <div>
        <Label for="y-max-input">Max</Label>
        <Input 
          id="y-max-input" 
          type="number" 
          bind:value={inputYMax}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button onclick={applyYAxisRange}>Apply</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .scatter-chart-container {
    width: 100%;
    height: 100%;
    min-height: 600px;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .chart-title {
    text-align: center;
    font-size: 18px;
    font-weight: bold;
    padding: 10px;
    flex-shrink: 0;
  }

  .chart-wrapper {
    flex: 1;
    display: block; /* flex 대신 block 사용 */
    width: 100%;
    height: 600px; /* 명확한 높이 지정 */
    min-height: 600px;
    position: relative;
  }

  .chart-container-wrapper {
    position: absolute; /* relative 대신 absolute 사용 */
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
  }

  .deck-container {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: #ffffff;
    border: 1px solid #e0e0e0;
  }

  .axis-overlay {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    z-index: 10;
  }

  .legend-container {
    position: absolute;
    right: 10px;
    top: 10px;
    width: 180px;
    max-height: 580px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.95);
    border: 1px solid #e0e0e0;
    border-radius: 4px;
    overflow-y: auto;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
  }

  .legend-title {
    font-weight: 600;
    font-size: 14px;
    margin-bottom: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid #e5e7eb;
    color: #374151;
  }

  .legend-items {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: #6b7280;
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
    transition: background-color 0.2s;
  }

  .legend-item:hover {
    background-color: #f3f4f6;
  }

  .legend-color {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
    border: 1px solid rgba(0, 0, 0, 0.1);
  }

  .legend-label {
    flex: 1;
    word-break: break-word;
  }

  .selection-box {
    position: absolute;
    border: 2px dashed #1890ff;
    background-color: rgba(24, 144, 255, 0.1);
    pointer-events: none;
    z-index: 100;
  }

  .loading-overlay {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(255, 255, 255, 0.95);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(2px);
  }

  .loading-content {
    text-align: center;
    padding: 30px;
    background: white;
    border-radius: 12px;
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
    min-width: 300px;
  }

  .loading-spinner {
    width: 50px;
    height: 50px;
    margin: 0 auto 20px;
    border: 4px solid #f3f3f3;
    border-top: 4px solid #3b82f6;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }

  .loading-text {
    font-size: 16px;
    font-weight: 600;
    color: #374151;
    margin-bottom: 15px;
  }

  .progress-bar {
    width: 100%;
    height: 8px;
    background-color: #e5e7eb;
    border-radius: 4px;
    overflow: hidden;
    margin-bottom: 10px;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #3b82f6, #8b5cf6);
    transition: width 0.3s ease;
    border-radius: 4px;
  }

  .progress-percent {
    font-size: 14px;
    font-weight: 500;
    color: #6b7280;
  }

  :global(.deck-tooltip) {
    z-index: 1000;
  }
</style>
