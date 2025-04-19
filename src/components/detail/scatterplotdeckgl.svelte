<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Deck, WebMercatorViewport } from '@deck.gl/core';
  import { ScatterplotLayer } from '@deck.gl/layers';
  import { ParquetLoader } from '@loaders.gl/parquet';
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";
  import { filtertrace } from '$stores/trace';
  import { invoke } from '@tauri-apps/api/tauri';

  // 새로운 imports: nanoid를 사용하지 않고 난수를 사용한 고유 ID 생성
  function generateID() {
    return `deck_chart_${Math.random().toString(36).substring(2, 9)}`;
  }

  interface ScatterChartProps {
    data?: any[];
    parquetFile?: string;
    xAxisKey: string;
    yAxisKey: string;
    legendKey: string;
    xAxisLabel?: string;
    yAxisLabel?: string;
    ycolumn: string;
    yAxisRange?: number[];
  }

  let { 
    data, 
    parquetFile,
    xAxisKey, 
    yAxisKey, 
    legendKey, 
    xAxisLabel = 'time', 
    yAxisLabel = 'sector', 
    ycolumn,
    yAxisRange
  }: ScatterChartProps = $props();

  // 차트 식별자 및 상태
  let chartId = $state(generateID());
  let chartTitle = $state('');
  let isInitializing = $state(true);
  let forceUpdate = $state(false);
  
  // 로딩 상태 관리 개선
  let isLoading = $state(true);
  let loadError = $state('');
  let loadingTimeoutId: number | null = null;
  let dataVersion = $state(0);
  let prevDataFingerprint = '';
  let lastFilterUpdate = $state('');
  let ignoreNextFilterChange = $state(false);

  // WebGL 엘리먼트 및 인스턴스
  let deckContainer: HTMLDivElement;
  let deckInstance: any;

  // 뷰포트 상태
  let viewport = $state({
    width: 0,
    height: 0,
    latitude: 0,
    longitude: 0,
    zoom: 0,
    bearing: 0,
    pitch: 0,
  });

  // 색상 매핑 및 시리즈 데이터
  let dataCache = $state<any>({});
  let legendColorMap = $state<Record<string, string>>({});
  let pointCount = $state(0);
  let tileSize = $state(256);
  let tileCache = $state<Map<string, any>>(new Map());
  let visibleTiles = $state<string[]>([]);
  let totalTiles = $state(0);
  let loadedTiles = $state(0);

  // 색상 상수
  const WRITE_COLOR = '#FF0000';
  const READ_COLOR = '#0000FF';
  const DISCARD_COLOR = '#00FF00';
  const FLUSH_COLOR = '#FFFF00';

  // 픽셀 크기 제어용 상태 변수
  let symbolSize = $state(5);
  
  // 컨텍스트 메뉴 상태
  let showContextMenu = $state(false);
  let contextMenuX = 0;
  let contextMenuY = 0;
  
  // 다이얼로그 상태 변수
  let showTitleDialog = $state(false);
  let inputTitle = $state('');
  
  // 사용자 설정 범례 표시 설정
  let legendshow = $state(true);
  let legendorient = $state('vertical');

  // X축 범위 설정 상태 변수
  let showXAxisRangeDialog = $state(false);
  let inputXMin = $state(0);
  let inputXMax = $state(0);
  
  // Y축 범위 설정 상태 변수
  let showYAxisRangeDialog = $state(false);
  let inputYMin = $state(0);
  let inputYMax = $state(0);

  // UFS 명령어별 고정 컬러 매핑
  const UFS_COMMAND_COLORS: { [key: string]: string } = {
    '0x2a': '#FF0000', // Write
    '0xa2': '#FF3333', // Write 관련
    '0x28': '#0000FF', // Read
    '0xb5': '#3333FF', // Read 관련
    '0x42': '#00FF00', // UNMAP
    '0x1b': '#FF00FF', // 자주색
    '0x12': '#00FFFF', // 청록색
    '0x35': '#FFFF00', // 노란색
    '0xc0': '#FF8800', // 주황색
  };

  // 블록 타입별 팔레트
  const WRITE_PALETTE = [
    '#FF0000', '#FF3333', '#FF6666', '#FF9999', '#FFCCCC'
  ];
  
  const READ_PALETTE = [
    '#0000FF', '#3333FF', '#6666FF', '#9999FF', '#CCCCFF'
  ];
  
  const DISCARD_PALETTE = [
    '#00FF00', '#33FF33', '#66FF66', '#99FF99', '#CCFFCC'
  ];
  
  const FLUSH_PALETTE = [
    '#FFFF00', '#FFFF33', '#FFFF66', '#FFFF99', '#FFFFCC'
  ];

  // CPU 색상 팔레트 - 0-7 무지개색, 나머지는 구분 가능한 색상들
  const CPU_PALETTE = [
    '#FF0000', // 빨간색 (CPU 0)
    '#FF7F00', // 주황색 (CPU 1)
    '#FFFF00', // 노란색 (CPU 2)
    '#00FF00', // 초록색 (CPU 3)
    '#0000FF', // 파란색 (CPU 4)
    '#4B0082', // 남색/인디고 (CPU 5)
    '#8B00FF', // 보라색 (CPU 6)
    '#FF00FF', // 자홍색/마젠타 (CPU 7)
    '#1f77b4', // 기존 색상 (CPU 8+)
    '#ff7f0e',
    '#2ca02c',
    '#d62728',
    '#9467bd', 
    '#8c564b',
    '#e377c2',
    '#7f7f7f',
    '#bcbd22',
    '#17becf',
    '#aec7e8',
    '#ffbb78'
  ];

  // 색상 매핑 객체 및 인덱스
  let blockWriteMapping: Record<string, string> = {};
  let blockReadMapping: Record<string, string> = {};
  let blockDiscardMapping: Record<string, string> = {};
  let blockFlushMapping: Record<string, string> = {};
  let writePaletteIndex = 0;
  let readPaletteIndex = 0;
  let discardPaletteIndex = 0;
  let flushPaletteIndex = 0;

  // CPU 색상 매핑용 객체 추가
  let cpuColorMapping: Record<string, string> = {};

  // 색상 관련 함수
  function getRandomColor() {
    return '#' + Math.floor(Math.random() * 16777215).toString(16);
  }

  function getColorForLegend(legend: string | string[]) {
    if (typeof legend !== 'string') return getRandomColor();
    
    // CPU 레전드 처리 - 레전드가 숫자인 경우 CPU로 간주
    if (legendKey === 'cpu') {
      // CPU 색상 매핑
      if (!(legend in cpuColorMapping)) {
        const cpuNum = parseInt(legend);
        if (!isNaN(cpuNum)) {
          // CPU 번호에 맞는 색상 할당 (순환)
          cpuColorMapping[legend] = CPU_PALETTE[cpuNum % CPU_PALETTE.length];
        } else {
          // 숫자가 아닌 경우 임의 색상
          cpuColorMapping[legend] = getRandomColor();
        }
      }
      return cpuColorMapping[legend];
    }

    // UFS 명령어 확인
    if (legend.toLowerCase().startsWith('0x')) {
      const cmdLower = legend.toLowerCase();
      if (UFS_COMMAND_COLORS[cmdLower]) {
        return UFS_COMMAND_COLORS[cmdLower];
      }
    }
    
    const prefix = legend[0].toUpperCase();
    switch (prefix) {
      case 'W': // Write
        if (!(legend in blockWriteMapping)) {
          blockWriteMapping[legend] = WRITE_PALETTE[writePaletteIndex % WRITE_PALETTE.length];
          writePaletteIndex++;
        }
        return blockWriteMapping[legend];
        
      case 'R': // Read
        if (!(legend in blockReadMapping)) {
          blockReadMapping[legend] = READ_PALETTE[readPaletteIndex % READ_PALETTE.length];
          readPaletteIndex++;
        }
        return blockReadMapping[legend];
        
      case 'D': // Discard
        if (!(legend in blockDiscardMapping)) {
          blockDiscardMapping[legend] = DISCARD_PALETTE[discardPaletteIndex % DISCARD_PALETTE.length];
          discardPaletteIndex++;
        }
        return blockDiscardMapping[legend];
        
      case 'F': // Flush
        if (!(legend in blockFlushMapping)) {
          blockFlushMapping[legend] = FLUSH_PALETTE[flushPaletteIndex % FLUSH_PALETTE.length];
          flushPaletteIndex++;
        }
        return blockFlushMapping[legend];
        
      default:
        return getRandomColor();
    }
  }

  // 범례 정렬 함수
  function sortLegends(legends: string[]) {
    const prefixOrder:any = {
      'R': 1, // Read
      'W': 2, // Write
      'D': 3, // Discard
      'F': 4, // Flush
      '0': 5  // 0x로 시작하는 UFS 명령어
    };
    
    return [...legends].sort((a, b) => {
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
  }

  // 타일 기반 데이터 로딩 함수
  async function loadTileData(tileX: number, tileY: number, zoom: number) {
    const tileKey = `${zoom}/${tileX}/${tileY}`;
    
    if (tileCache.has(tileKey)) {
      return tileCache.get(tileKey);
    }
    
    if (!parquetFile) {
      console.error('타일 로딩 실패: parquetFile이 정의되지 않았습니다');
      return [];
    }
    
    try {
      // Tauri 백엔드 호출 - 특정 타일의 데이터 요청
      const result = await invoke('load_tile_data', {
        parquetFile,
        tileKey,
        xAxisKey,
        yAxisKey,
        legendKey,
        xRange: [viewport.longitude - viewport.width/2, viewport.longitude + viewport.width/2],
        yRange: [viewport.latitude - viewport.height/2, viewport.latitude + viewport.height/2],
        zoom
      });
      
      const tileData = JSON.parse(result as string);
      tileCache.set(tileKey, tileData);
      loadedTiles++;
      
      return tileData;
    } catch (error) {
      console.error(`타일 ${tileKey} 로딩 실패:`, error);
      return [];
    }
  }
  
  // 뷰포트 변경 시 필요한 타일 계산
  function calculateVisibleTiles() {
    if (!deckInstance) return [];
    
    const z = Math.floor(viewport.zoom);
    const maxTile = Math.pow(2, z);
    
    // 뷰포트 범위 계산
    const webMercatorVP = new WebMercatorViewport(viewport);
    const bounds = webMercatorVP.getBounds();
    
    // 바운드를 타일 좌표로 변환
    const [minX, minY] = webMercatorVP.lngLatToWorld([bounds[0], bounds[1]]);
    const [maxX, maxY] = webMercatorVP.lngLatToWorld([bounds[2], bounds[3]]);
    
    const minTileX = Math.floor(minX / tileSize) % maxTile;
    const maxTileX = Math.ceil(maxX / tileSize) % maxTile;
    const minTileY = Math.floor(minY / tileSize) % maxTile;
    const maxTileY = Math.ceil(maxY / tileSize) % maxTile;
    
    const tiles = [];
    for (let x = minTileX; x <= maxTileX; x++) {
      for (let y = minTileY; y <= maxTileY; y++) {
        tiles.push(`${z}/${x}/${y}`);
      }
    }
    
    totalTiles = tiles.length;
    return tiles;
  }

  // 현재 뷰포트에 필요한 모든 타일 로드
  async function loadAllVisibleTiles() {
    const tiles = calculateVisibleTiles();
    visibleTiles = tiles;
    loadedTiles = 0;
    
    if (tiles.length === 0) return [];
    
    const tilePromises = tiles.map(tile => {
      const [z, x, y] = tile.split('/').map(Number);
      return loadTileData(x, y, z);
    });
    
    const allTileData = await Promise.all(tilePromises);
    return allTileData.flat();
  }

  // deck.gl 인스턴스 초기화
  function initDeck() {
    if (!deckContainer) return;
    
    isLoading = true;
    loadError = '';
    
    try {
      // 초기 뷰포트 설정
      const initialViewState = {
        longitude: 0,
        latitude: 0,
        zoom: 0,
        pitch: 0,
        bearing: 0
      };
      
      // deck.gl 인스턴스 생성
      deckInstance = new Deck({
        canvas: deckContainer,
        initialViewState,
        controller: true,
        getTooltip: ({object}) => object && {
          html: `
            <div>
              <div><b>${legendKey}:</b> ${object[legendKey]}</div>
              <div><b>${xAxisLabel}:</b> ${object[xAxisKey]}</div>
              <div><b>${yAxisLabel}:</b> ${object[yAxisKey]}</div>
            </div>
          `,
          style: {
            background: 'rgba(0, 0, 0, 0.8)',
            color: '#fff',
            padding: '5px',
            borderRadius: '4px',
            fontSize: '12px',
          }
        },
        onViewStateChange: ({viewState}) => {
          viewport = viewState;
          updateLayersWithNewTiles();
        },
        layers: [],
      });
      
      // 초기 데이터 로드
      updateLayersWithNewTiles();
      
      isLoading = false;
      isInitializing = false;
    } catch (error) {
      console.error(`[${chartId}] Deck.gl 초기화 오류:`, error);
      loadError = '차트 초기화 중 오류가 발생했습니다';
      isLoading = false;
    }
  }
  
  // 타일 데이터 기반으로 레이어 업데이트
  async function updateLayersWithNewTiles() {
    if (!deckInstance) return;
    
    try {
      isLoading = true;
      
      // 현재 뷰포트에 필요한 모든 타일 로드
      const pointData = await loadAllVisibleTiles();
      pointCount = pointData.length;
      
      // 레이어가 없으면 데이터 없음 표시
      if (pointData.length === 0) {
        deckContainer.innerHTML = '<div class="empty-message">표시할 데이터가 없습니다</div>';
        isLoading = false;
        return;
      }
      
      // 색상 매핑 구성
      const colorMap = {};
      const uniqueLegends = [...new Set(pointData.map(d => d[legendKey]))];
      uniqueLegends.forEach(legend => {
        colorMap[legend] = getColorForLegend(legend);
      });
      
      // ScatterplotLayer 생성
      const scatterLayer = new ScatterplotLayer({
        id: 'scatter-plot',
        data: pointData,
        getPosition: d => [d[xAxisKey], d[yAxisKey], 0],
        getRadius: symbolSize,
        getFillColor: d => {
          const color = colorMap[d[legendKey]] || '#000000';
          // hex 색상을 RGB 배열로 변환
          const r = parseInt(color.slice(1, 3), 16);
          const g = parseInt(color.slice(3, 5), 16);
          const b = parseInt(color.slice(5, 7), 16);
          return [r, g, b, 255]; // R, G, B, Alpha
        },
        pickable: true,
        stroked: false,
        opacity: 0.8,
        radiusMinPixels: 1,
        radiusMaxPixels: 100,
      });
      
      // 레이어 업데이트
      deckInstance.setProps({
        layers: [scatterLayer],
      });
      
      isLoading = false;
    } catch (error) {
      console.error(`[${chartId}] 레이어 업데이트 오류:`, error);
      loadError = '데이터 업데이트 중 오류가 발생했습니다';
      isLoading = false;
    }
  }
  
  // 로딩 타임아웃 관리
  function startLoadingTimeout() {
    clearLoadingTimeout();
    loadingTimeoutId = setTimeout(() => {
      if (isLoading) {
        loadError = '데이터 로드 시간이 초과되었습니다';
        isLoading = false;
      }
    }, 30000) as unknown as number; // 30초 타임아웃
  }

  function clearLoadingTimeout() {
    if (loadingTimeoutId !== null) {
      clearTimeout(loadingTimeoutId);
      loadingTimeoutId = null;
    }
  }
  
  // 이벤트 핸들러 및 설정
  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    showContextMenu = true;
    contextMenuX = e.clientX;
    contextMenuY = e.clientY;
  }
  
  function openTitleDialog() {
    inputTitle = chartTitle;
    showTitleDialog = true;
  }
  
  function saveTitleDialog() {
    chartTitle = inputTitle;
    showTitleDialog = false;
  }
  
  function openXAxisRangeDialog() {
    inputXMin = viewport.longitude - viewport.width/2;
    inputXMax = viewport.longitude + viewport.width/2;
    showXAxisRangeDialog = true;
  }
  
  function saveXAxisRangeDialog() {
    if (deckInstance && inputXMin < inputXMax) {
      // X축 범위 설정
      const viewStateUpdate = {
        ...viewport,
        longitude: (inputXMin + inputXMax) / 2,
        width: inputXMax - inputXMin
      };
      
      deckInstance.setProps({
        viewState: viewStateUpdate
      });
      
      // filtertrace 스토어 업데이트
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: inputXMin,
        to_time: inputXMax,
        from_lba: viewport.latitude - viewport.height/2,
        to_lba: viewport.latitude + viewport.height/2
      };
    }
    showXAxisRangeDialog = false;
  }
  
  function openYAxisRangeDialog() {
    inputYMin = viewport.latitude - viewport.height/2;
    inputYMax = viewport.latitude + viewport.height/2;
    showYAxisRangeDialog = true;
  }
  
  function saveYAxisRangeDialog() {
    if (deckInstance && inputYMin < inputYMax) {
      // Y축 범위 설정
      const viewStateUpdate = {
        ...viewport,
        latitude: (inputYMin + inputYMax) / 2,
        height: inputYMax - inputYMin
      };
      
      deckInstance.setProps({
        viewState: viewStateUpdate
      });
      
      // filtertrace 스토어 업데이트
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: viewport.longitude - viewport.width/2,
        to_time: viewport.longitude + viewport.width/2,
        from_lba: inputYMin,
        to_lba: inputYMax
      };
    }
    showYAxisRangeDialog = false;
  }
  
  function handleSymbolSizeChange(value: number[]) {
    symbolSize = value[0];
    if (deckInstance) {
      updateLayersWithNewTiles();
    }
  }
  
  function toggleLegend() {
    legendshow = !legendshow;
  }
  
  function toggleLegendOrientation() {
    legendorient = legendorient === 'vertical' ? 'horizontal' : 'vertical';
  }
  
  // 필터 변경 시 동작
  $effect(() => {
    if (ignoreNextFilterChange) {
      ignoreNextFilterChange = false;
      return;
    }
    
    const filterString = JSON.stringify($filtertrace);
    if (lastFilterUpdate !== filterString && $filtertrace) {
      lastFilterUpdate = filterString;
      
      // filtertrace 값이 변경되면 뷰포트 업데이트
      if (deckInstance && $filtertrace.from_time !== undefined && $filtertrace.to_time !== undefined) {
        const xCenter = ($filtertrace.from_time + $filtertrace.to_time) / 2;
        const xWidth = $filtertrace.to_time - $filtertrace.from_time;
        
        const yCenter = ($filtertrace.from_lba + $filtertrace.to_lba) / 2;
        const yHeight = $filtertrace.to_lba - $filtertrace.from_lba;
        
        const viewStateUpdate = {
          ...viewport,
          longitude: xCenter,
          latitude: yCenter,
          width: xWidth,
          height: yHeight
        };
        
        deckInstance.setProps({
          viewState: viewStateUpdate
        });
        
        // 타일 업데이트
        updateLayersWithNewTiles();
      }
    }
  });
  
  // 컴포넌트 라이프사이클 함수
  onMount(() => {
    if (!deckContainer) return;
    
    console.log(`[${chartId}] 컴포넌트 마운트`);
    
    // 지연된 초기화로 DOM 준비 시간 확보
    setTimeout(() => {
      initDeck();
    }, 200 + Math.random() * 300); // 여러 차트가 있을 때 순차적으로 처리
    
    // 이벤트 리스너 등록
    deckContainer.addEventListener('contextmenu', handleContextMenu);
  });
  
  onDestroy(() => {
    if (deckInstance) {
      deckInstance.finalize();
      deckInstance = null;
    }
    
    // 이벤트 리스너 제거
    if (deckContainer) {
      deckContainer.removeEventListener('contextmenu', handleContextMenu);
    }
    
    clearLoadingTimeout();
  });
</script>

<div class="chart-container" bind:this={deckContainer} data-testid="deckgl-container">
  {#if chartTitle}
    <div class="chart-title">{chartTitle}</div>
  {/if}
  
  {#if isLoading}
    <div class="loading-overlay">
      <div class="loading-spinner"></div>
      <div class="loading-text">데이터 로드 중...</div>
      <div class="loading-stats">
        <div>타일: {loadedTiles}/{totalTiles}</div>
        <div>포인트: {pointCount.toLocaleString()}</div>
      </div>
    </div>
  {/if}
  
  {#if loadError}
    <div class="error-overlay">
      <div class="error-icon">⚠️</div>
      <div class="error-text">{loadError}</div>
      <div class="error-actions">
        <Button variant="destructive" size="sm" on:click={() => { loadError = ''; initDeck(); }}>다시 시도</Button>
      </div>
    </div>
  {/if}
</div>

<ContextMenu.Root open={showContextMenu} onOpenChange={(open) => showContextMenu = open}>
  <ContextMenu.Trigger />
  <ContextMenu.Portal>
    <ContextMenu.Content style="position: fixed; top: {contextMenuY}px; left: {contextMenuX}px;" class="z-10">
      <ContextMenu.Item on:click={openTitleDialog}>차트 제목 설정</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item on:click={openXAxisRangeDialog}>X축 범위 설정</ContextMenu.Item>
      <ContextMenu.Item on:click={openYAxisRangeDialog}>Y축 범위 설정</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Item on:click={toggleLegend}>범례 {legendshow ? '숨기기' : '표시'}</ContextMenu.Item>
      <ContextMenu.Item on:click={toggleLegendOrientation}>범례 방향 변경 ({legendorient})</ContextMenu.Item>
      <ContextMenu.Separator />
      <ContextMenu.Label>포인트 크기</ContextMenu.Label>
      <div class="slider-container">
        <Slider
          value={[symbolSize]}
          min={1}
          max={20}
          step={1}
          on:valuechange={(e) => handleSymbolSizeChange(e.detail)}
        />
        <div class="slider-value">{symbolSize}px</div>
      </div>
    </ContextMenu.Content>
  </ContextMenu.Portal>
</ContextMenu.Root>

<Dialog.Root open={showTitleDialog} onOpenChange={(open) => showTitleDialog = open}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>차트 제목 설정</Dialog.Title>
    </Dialog.Header>
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="title" class="text-right">제목</Label>
        <Input
          id="title"
          class="col-span-3"
          bind:value={inputTitle}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button variant="outline" on:click={() => showTitleDialog = false}>취소</Button>
      <Button on:click={saveTitleDialog}>저장</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root open={showXAxisRangeDialog} onOpenChange={(open) => showXAxisRangeDialog = open}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>X축 범위 설정</Dialog.Title>
    </Dialog.Header>
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="xmin" class="text-right">최소값</Label>
        <Input
          id="xmin"
          type="number"
          class="col-span-3"
          bind:value={inputXMin}
        />
      </div>
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="xmax" class="text-right">최대값</Label>
        <Input
          id="xmax"
          type="number"
          class="col-span-3"
          bind:value={inputXMax}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button variant="outline" on:click={() => showXAxisRangeDialog = false}>취소</Button>
      <Button on:click={saveXAxisRangeDialog}>적용</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root open={showYAxisRangeDialog} onOpenChange={(open) => showYAxisRangeDialog = open}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>Y축 범위 설정</Dialog.Title>
    </Dialog.Header>
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="ymin" class="text-right">최소값</Label>
        <Input
          id="ymin"
          type="number"
          class="col-span-3"
          bind:value={inputYMin}
        />
      </div>
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="ymax" class="text-right">최대값</Label>
        <Input
          id="ymax"
          type="number"
          class="col-span-3"
          bind:value={inputYMax}
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button variant="outline" on:click={() => showYAxisRangeDialog = false}>취소</Button>
      <Button on:click={saveYAxisRangeDialog}>적용</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .chart-container {
    width: 100%;
    height: 500px;
    position: relative;
    border: 1px solid rgba(0, 0, 0, 0.05);
    border-radius: 4px;
    overflow: hidden;
  }
  .chart-title {
    position: absolute;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    font-size: 16px;
    font-weight: bold;
    color: #333;
    z-index: 5;
    background-color: rgba(255, 255, 255, 0.8);
    padding: 2px 8px;
    border-radius: 4px;
  }
  .loading-overlay {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background-color: rgba(255, 255, 255, 0.85);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    z-index: 100;
    backdrop-filter: blur(3px);
  }
  .loading-spinner {
    border: 4px solid rgba(0, 0, 0, 0.1);
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border-left-color: #09f;
    animation: spin 1s linear infinite;
    margin-bottom: 15px;
  }
  .loading-text {
    font-size: 16px;
    color: #333;
  }
  .loading-stats {
    margin-top: 8px;
    font-size: 12px;
    color: #666;
    text-align: center;
  }
  .error-overlay {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background-color: rgba(255, 255, 255, 0.9);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .error-icon {
    font-size: 32px;
    margin-bottom: 10px;
  }
  .error-text {
    font-size: 16px;
    color: #d32f2f;
    margin-bottom: 20px;
  }
  .error-actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .empty-message {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
    font-size: 16px;
  }
  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
  .slider-container {
    padding: 0 1rem;
  }
  .slider-value {
    font-size: 1rem;
    font-weight: bold;
    text-align: center;
    margin-top: 0.5rem;
  }
</style>