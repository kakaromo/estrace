<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Plotly from 'plotly.js-dist-min';
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";
  import { filtertrace } from '$stores/trace';

  // 새로운 imports: nanoid를 사용하지 않고 난수를 사용한 고유 ID 생성
  function generateID() {
    return `chart_${Math.random().toString(36).substring(2, 9)}`;
  }

  interface ScatterChartProps {
    data: any[];
    xAxisKey: string;
    yAxisKey: string;
    legendKey: string;
    xAxisLabel?: string;
    yAxisLabel?: string;
    ycolumn: string;
    yAxisRange?: number[]; // 추가: Y축 범위 제한 옵션
  }

  let { 
    data, 
    xAxisKey, 
    yAxisKey, 
    legendKey, 
    xAxisLabel = 'time', 
    yAxisLabel = 'sector', 
    ycolumn,
    yAxisRange // 추가된 prop
  }: ScatterChartProps = $props();

  // 차트 식별자 및 상태
  let chartId = $state(generateID());
  let chartTitle = $state('');
  let isInitializing = $state(true); // 초기 로딩 플래그
  let forceUpdate = $state(false); // 강제 업데이트 플래그
  
  // 로딩 상태 관리 개선
  let isLoading = $state(false);
  let loadError = $state('');
  let loadingTimeoutId: number | null = null;
  let dataVersion = $state(0);
  let prevDataFingerprint = '';
  
  // 기존 상태 변수들
  let showTitleDialog = $state(false);
  let inputTitle = $state('');
  let showSymbolSizeDialog = $state(false);
  let inputSymbolSize = $state(3);
  let symbolSize = $state(3);
  let xAxisName = $state('time');
  let yAxisName = $state('sector');
  let legendposition = $state('middle');
  let legendorient = $state('vertical');
  let legendshow = $state(true);
  let tooltipshow = $state(true);
  let prevData = $state(null);
  
  // 축 이름 초기화
  xAxisName = xAxisLabel;
  yAxisName = yAxisLabel;
  prevData = data;

  // 차트 요소
  let chartContainer: HTMLElement;
  let plotlyInstance: any;
  let resizeObserver: ResizeObserver;

  // zoom 범위 및 tooltip 상태
  let { xZoomFrom, xZoomTo, yZoomFrom, yZoomTo } = $state({ xZoomFrom: 0, xZoomTo: 0, yZoomFrom: 0, yZoomTo: 0 });
  let globalSeriesData: Record<string, number[][]> = {};

  // X축 범위 설정 상태 변수
  let showXAxisRangeDialog = $state(false);
  let inputXMin = $state(0);
  let inputXMax = $state(0);
  
  // Y축 범위 설정 상태 변수
  let showYAxisRangeDialog = $state(false);
  let inputYMin = $state(0);
  let inputYMax = $state(0);

  // 색상 상수
  const WRITE_COLOR = '#FF0000';
  const READ_COLOR = '#0000FF';
  const DISCARD_COLOR = '#00FF00';
  const FLUSH_COLOR = '#FFFF00';

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

  // 데이터 해시 캐시 개선
  let dataHashCache = new Map();
  let processingUpdate = false; // 업데이트 중복 방지 락

  // CPU 색상 매핑용 객체 추가
  let cpuColorMapping: Record<string, string> = {};
  let seriesColorMap: Record<string, string> = {};
  let legends: string[] = [];

  // 차트 타입과 동기화 설정
  let chartType = $state('scatter'); // 'scatter', 'heatmap' 등
  let enableSync = $state(true); // 차트 동기화 활성화 여부
  let lastFilterUpdate = $state(''); // 마지막 필터 업데이트 추적
  let ignoreNextFilterChange = $state(false); // 특정 필터 변경 무시 플래그

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
  
  // 차트 데이터 준비 함수
  function prepareSeriesData() {
    let seriesData: Record<string, number[][]> = {};
    if (!data) return seriesData;
    
    try {
      data.forEach(item => {
        const x = parseFloat(item[xAxisKey]);
        const y = parseFloat(item[yAxisKey]);
        
        // Block 및 UFS 데이터에 대한 필터링 로직 추가
        // action 필드는 UFS에서는 'command', Block에서는 'action'으로 사용됨
        const action = item.action || item.command || '';

        // Block 데이터는 block_rq_issue 또는 block_rq_complete로 식별
        if (ycolumn === 'dtoc' || ycolumn === 'ctoc') {
          // 만약 ctod 차트라면 complete 응답만 포함
          // UFS는 complete_rsp, Block은 block_rq_complete
          if (!(action === 'complete_rsp' || action === 'block_rq_complete')) {
            return; // 필터링된 데이터는 건너뜀
          }
        } else {
          // 그 외 차트는 issue/send 요청만 포함
          // UFS는 send_req, Block은 block_rq_issue
          if (!(action === 'send_req' || action === 'block_rq_issue')) {
            return; // 필터링된 데이터는 건너뜀
          }
        }

        if (!isNaN(x) && !isNaN(y)) {
          const legend = String(item[legendKey] || '');
          if (!seriesData[legend]) seriesData[legend] = [];
          seriesData[legend].push([x, y]);
        }
      });
      
      // x 값 기준 정렬
      Object.keys(seriesData).forEach(legend => {
        seriesData[legend].sort((a: number[], b: number[]) => a[0] - b[0]);
      });

      // 범례 설정 및 정렬
      legends = sortLegends(Object.keys(seriesData));
      legends.forEach(legend => {
        seriesColorMap[legend] = getColorForLegend(legend);
      });
      
      globalSeriesData = seriesData;
    } catch (error) {
      console.error(`[${chartId}] 시리즈 데이터 준비 오류:`, error);
      // 빈 데이터로 폴백
      globalSeriesData = {};
      legends = [];
    }
    
    return globalSeriesData;
  }

  // Plotly용 데이터 준비
  function preparePlotlyData() {
    const plotData = [];
    let totalPoints = 0;
    
    try {
      const seriesData = prepareSeriesData();
      
      // 전체 데이터 포인트 수 계산
      Object.values(seriesData).forEach(points => {
        totalPoints += points.length;
      });
      
      console.log(`[${chartId}] 데이터 포인트 수: ${totalPoints}`);

      // 성능 최적화: 데이터가 많을 경우 WebGL 사용
      const useWebGLRenderer = totalPoints > 500;
      const chartType = useWebGLRenderer ? 'scattergl' : 'scatter';

      for (const legend of legends) {
        const points = seriesData[legend] || [];
        if (points.length === 0) continue;
        
        const x = points.map(p => p[0]);
        const y = points.map(p => p[1]);

        plotData.push({
          type: chartType,
          mode: 'markers',
          name: legend,
          x: x,
          y: y,
          marker: {
            size: symbolSize,
            color: seriesColorMap[legend],
            line: { width: 0 }
          },
          hoverinfo: 'x+y+name'
        });
      }
    } catch (error) {
      console.error(`[${chartId}] Plotly 데이터 준비 오류:`, error);
    }
    
    return plotData;
  }

  // Plotly 레이아웃 가져오기
  function getLayout() {
    // Y축 범위 설정
    const yaxisConfig = {
      title: {
        text: yAxisName,
        font: { size: 13, weight: 'bold' }
      },
      showgrid: true,
      zeroline: true,
      // 축약 표시 방지를 위한 설정
      exponentformat: 'none',
      showexponent: 'all',
      tickformat: 'd',
      hoverformat: '.2f',
      separatethousands: true
    };

    // yAxisRange가 제공된 경우 범위 설정 추가
    if (yAxisRange && Array.isArray(yAxisRange) && yAxisRange.length === 2) {
      yaxisConfig.range = yAxisRange;
      yaxisConfig.fixedrange = true; // CPU의 경우 범위 고정 (사용자 줌 방지)
    }

    return {
      title: {
        text: chartTitle,
        font: { size: 16 }
      },
      showlegend: legendshow,
      legend: {
        orientation: legendorient === 'vertical' ? 'v' : 'h',
        x: 1,
        y: 0.5,
        xanchor: 'left',
        yanchor: 'middle',
        bgcolor: 'rgba(255, 255, 255, 0.9)',
        bordercolor: '#e6f7ff',
        borderwidth: 1,
        font: { size: 12 }
      },
      margin: { l: 70, r: 50, t: 60, b: 70 },
      xaxis: {
        title: {
          text: xAxisName,
          font: { size: 13, weight: 'bold' }
        },
        showgrid: true,
        zeroline: true,
        // 축약 표시 방지를 위한 설정
        exponentformat: 'none',
        showexponent: 'all',
        tickformat: 'd',
        hoverformat: '.2f',
        separatethousands: true
      },
      yaxis: yaxisConfig,
      hovermode: 'closest',
      // 성능 최적화 설정
      uirevision: chartId, // 줌/패닝 상태 유지
    };
  }

  // Plotly 설정
  function getConfig() {
    return {
      responsive: true,
      displayModeBar: true,
      displaylogo: false,
      modeBarButtonsToAdd: ['select2d', 'lasso2d'],
      modeBarButtonsToRemove: ['autoScale2d'],
      scrollZoom: true
    };
  }

  // 차트 타이틀 수정 함수
  function applyTitleChange() {
    chartTitle = inputTitle;
    showTitleDialog = false;
    
    if (chartContainer && plotlyInstance) {
      Plotly.relayout(chartContainer, { 'title.text': chartTitle });
    }
  }
  
  // X축 범위 설정 함수
  function applyXAxisRange() {
    if (chartContainer && inputXMin < inputXMax) {
      // X축 범위 설정
      Plotly.relayout(chartContainer, {
        'xaxis.range': [inputXMin, inputXMax]
      });
      
      // 상태 변수 업데이트
      xZoomFrom = inputXMin;
      xZoomTo = inputXMax;
      
      // filtertrace 스토어 업데이트
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: inputXMin,
        to_time: inputXMax,
        from_lba: yZoomFrom,
        to_lba: yZoomTo
      };
    }
    showXAxisRangeDialog = false;
  }
  
  // Y축 범위 설정 함수
  function applyYAxisRange() {
    if (chartContainer && inputYMin < inputYMax) {
      // Y축 범위 설정
      Plotly.relayout(chartContainer, {
        'yaxis.range': [inputYMin, inputYMax]
      });
      
      // 상태 변수 업데이트
      yZoomFrom = inputYMin;
      yZoomTo = inputYMax;
      
      // filtertrace 스토어 업데이트
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: xZoomFrom,
        to_time: xZoomTo,
        from_lba: inputYMin,
        to_lba: inputYMax
      };
    }
    showYAxisRangeDialog = false;
  }

  // 차트 렌더링 설정
  const RENDER_TIMEOUT = 15000; // 15초 타임아웃
  const UPDATE_DELAY = 50; // 업데이트 디바운스

  // 로딩 타임아웃 관리
  function startLoadingTimeout() {
    if (loadingTimeoutId) {
      clearTimeout(loadingTimeoutId);
    }
    loadingTimeoutId = window.setTimeout(() => {
      if (isLoading) {
        console.warn(`[${chartId}] 렌더링 타임아웃 발생, 렌더링 재시도`);
        loadError = '차트 렌더링 시간이 초과되었습니다';
        isLoading = false;
        
        // 타임아웃 후 강제 업데이트 시도
        forceUpdate = true;
        dataHashCache.clear();
        setTimeout(() => {
          initChart();
        }, 100);
      }
    }, RENDER_TIMEOUT);
  }

  function clearLoadingTimeout() {
    if (loadingTimeoutId) {
      clearTimeout(loadingTimeoutId);
      loadingTimeoutId = null;
    }
  }

  // 데이터 변경 감지를 위한 간소화된 지문 함수
  function getDataFingerprint(): string {
    if (!data || data.length === 0) return 'empty';
    
    try {
      // 최소한의 정보만 사용하여 안정적인 지문 생성
      return `v${dataVersion}:len${data.length}:${xAxisKey}:${yAxisKey}:${legendKey}`;
    } catch (e) {
      console.error(`[${chartId}] 지문 생성 오류:`, e);
      return `v${dataVersion}:error`;
    }
  }

  // Props 변경 감지 - 디바운스 처리
  let updateTimeoutId: number | null = null;
  
  $effect(() => {
    // 초기화 중에는 스킵
    if (isInitializing) return;
    
    // 기본적인 데이터 존재 확인
    if (!data) return;
    
    const fingerprint = getDataFingerprint();
    
    if (fingerprint !== prevDataFingerprint || forceUpdate) {
      console.log(`[${chartId}] 데이터 변경 감지: ${fingerprint}`);
      prevDataFingerprint = fingerprint;
      forceUpdate = false;
      dataVersion++;
      
      // 중복 업데이트 방지를 위한 디바운스
      if (updateTimeoutId) clearTimeout(updateTimeoutId);
      updateTimeoutId = window.setTimeout(() => {
        if (!processingUpdate) {
          processingUpdate = true;
          try {
            updateChart();
          } finally {
            processingUpdate = false;
          }
        }
      }, UPDATE_DELAY);
    }
  });

  // 필터트레이스 스토어 변경 감지 및 처리
  $effect(() => {
    // 초기화 중에는 스킵
    if (isInitializing || !enableSync) return;
    
    // 자신이 트리거한 변경은 무시 (무한 루프 방지)
    if (ignoreNextFilterChange) {
      ignoreNextFilterChange = false;
      return;
    }

    const filterHash = JSON.stringify($filtertrace);
    
    // 변경이 없으면 스킵
    if (lastFilterUpdate === filterHash) return;
    
    // 유효한 필터 변경만 처리
    if ($filtertrace && 
        $filtertrace.from_time !== undefined && 
        $filtertrace.to_time !== undefined &&
        ($filtertrace.from_time > 0 || $filtertrace.to_time > 0)) {
      
      console.log(`[${chartId}] 외부 필터 변경 감지:`, $filtertrace);
      lastFilterUpdate = filterHash;
      
      // 차트가 초기화되었으면 동기화 적용
      if (plotlyInstance && chartContainer) {
        syncChartToFilters();
      }
    }
  });

  // 필터에 따라 차트 뷰포트 동기화
  function syncChartToFilters() {
    if (!plotlyInstance || !chartContainer || !$filtertrace) return;
    
    try {
      console.log(`[${chartId}] 차트 뷰포트 동기화 중...`);
      
      // X축 범위 설정 (시간)
      if ($filtertrace.from_time > 0 || $filtertrace.to_time > 0) {
        const xAxisUpdate = {
          'xaxis.range': [$filtertrace.from_time, $filtertrace.to_time]
        };
        
        // Y축 범위 설정 (특정 열에 해당하는 경우만, 그리고 고정 범위가 없을 때만)
        let update = xAxisUpdate;
        if (!yAxisRange && $filtertrace.zoom_column === ycolumn && 
            ($filtertrace.from_lba > 0 || $filtertrace.to_lba > 0)) {
          update = {
            ...xAxisUpdate,
            'yaxis.range': [$filtertrace.from_lba, $filtertrace.to_lba]
          };
        }
        
        // 레이아웃 업데이트 (애니메이션 없이 즉시 적용)
        Plotly.relayout(chartContainer, update)
          .then(() => {
            console.log(`[${chartId}] 뷰포트 동기화 완료`);
            // 내부 상태 업데이트 (이벤트 발생 없이)
            xZoomFrom = $filtertrace.from_time;
            xZoomTo = $filtertrace.to_time;
            
            if (!yAxisRange && $filtertrace.zoom_column === ycolumn) {
              yZoomFrom = $filtertrace.from_lba;
              yZoomTo = $filtertrace.to_lba;
            }
          })
          .catch(err => {
            console.error(`[${chartId}] 뷰포트 동기화 오류:`, err);
          });
      }
    } catch (error) {
      console.error(`[${chartId}] 동기화 오류:`, error);
    }
  }

  // 차트 초기화
  function initChart() {
    if (!chartContainer) return;
    isLoading = true;
    loadError = '';
    startLoadingTimeout();
    
    console.log(`[${chartId}] 차트 초기화 시작`);
    chartContainer.innerHTML = '<div class="loading-message">차트 데이터 준비 중...</div>';

    // 이전 인스턴스 정리
    if (plotlyInstance) {
      try {
        Plotly.purge(chartContainer);
        plotlyInstance = null;
      } catch (e) {
        console.error(`[${chartId}] 차트 정리 오류:`, e);
      }
    }

    // 안전한 비동기 처리
    setTimeout(() => {
      if (!chartContainer) {
        clearLoadingTimeout();
        isLoading = false;
        console.warn(`[${chartId}] 컨테이너 찾을 수 없음`);
        return;
      }

      try {
        const plotData = preparePlotlyData();
        
        if (plotData.length === 0) {
          chartContainer.innerHTML = '<div class="empty-message">표시할 데이터가 없습니다</div>';
          clearLoadingTimeout();
          isLoading = false;
          console.log(`[${chartId}] 빈 데이터 처리 완료`);
          return;
        }
        
        Plotly.newPlot(chartContainer, plotData, getLayout(), getConfig())
          .then((plotDiv) => {
            plotlyInstance = plotDiv;
            clearLoadingTimeout();
            isLoading = false;
            isInitializing = false; // 초기화 완료
            console.log(`[${chartId}] 차트 초기화 완료`);
            
            // 기존 이벤트 리스너 등록
            setupEventListeners();
          })
          .catch(err => {
            console.error(`[${chartId}] Plotly 초기화 오류:`, err);
            loadError = '차트 초기화 중 오류가 발생했습니다';
            clearLoadingTimeout();
          });
      } catch (error) {
        console.error(`[${chartId}] 데이터 준비 오류:`, error);
        loadError = '데이터 준비 중 오류가 발생했습니다';
        clearLoadingTimeout();
        isLoading = false;
      }
    }, 200 + Math.random() * 300); // 여러 차트가 있을 때 순차적으로 처리
  }

  // 이벤트 리스너 등록
  function setupEventListeners() {
    if (!chartContainer) return;

    // 이벤트 리스너 등록
    chartContainer.on('plotly_relayout', handleRelayout);
    chartContainer.on('plotly_doubleclick', handleDoubleClick);
    
    // 객체 참조를 저장하여 나중에 제거 가능하도록 함
    chartContainer._relayoutHandler = handleRelayout;
    chartContainer._doubleClickHandler = handleDoubleClick;
  }

  // 이벤트 핸들러 분리 및 개선
  function handleRelayout(eventData) {
    // 줌 이벤트 처리
    if (eventData['xaxis.range[0]'] !== undefined && eventData['xaxis.range[1]'] !== undefined) {
      xZoomFrom = eventData['xaxis.range[0]'];
      xZoomTo = eventData['xaxis.range[1]'];
    }
    
    if (eventData['yaxis.range[0]'] !== undefined && eventData['yaxis.range[1]'] !== undefined) {
      yZoomFrom = eventData['yaxis.range[0]'];
      yZoomTo = eventData['yaxis.range[1]'];
    }

    // 유효한 범위 변경인 경우만 필터 업데이트
    if ((xZoomFrom !== 0 || xZoomTo !== 0)) {
      // 다음 필터 변경 무시 설정 (자신의 변경이므로)
      ignoreNextFilterChange = true;
      
      // 필터트레이스 업데이트 (다른 차트에 알림)
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: xZoomFrom,
        to_time: xZoomTo,
        from_lba: yZoomFrom,
        to_lba: yZoomTo
      };
      
      // 현재 필터 상태 저장
      lastFilterUpdate = JSON.stringify($filtertrace);
      
      console.log(`[${chartId}] 범위 변경 전파:`, $filtertrace);
    }
  }

  function handleDoubleClick() {
    // 줌 리셋
    xZoomFrom = 0;
    xZoomTo = 0;
    yZoomFrom = 0;
    yZoomTo = 0;
    
    // 다음 필터 변경 무시 설정
    ignoreNextFilterChange = true;
    
    // 필터트레이스 초기화 (다른 차트에 알림)
    $filtertrace = {
      zoom_column: ycolumn,
      from_time: 0,
      to_time: 0,
      from_lba: 0,
      to_lba: 0
    };
    
    // 현재 필터 상태 저장
    lastFilterUpdate = JSON.stringify($filtertrace);
    
    console.log(`[${chartId}] 줌 리셋 전파`);
  }

  // 업데이트 로직 간소화
  function updateChart() {
    if (!chartContainer) {
      return;
    }

    // 초기화가 필요한 경우
    if (!plotlyInstance) {
      initChart();
      return;
    }

    console.log(`[${chartId}] 차트 업데이트 시작`);
    isLoading = true;
    startLoadingTimeout();
    
    try {
      const plotData = preparePlotlyData();
      
      if (plotData.length === 0) {
        chartContainer.innerHTML = '<div class="empty-message">표시할 데이터가 없습니다</div>';
        clearLoadingTimeout();
        isLoading = false;
        console.log(`[${chartId}] 빈 데이터로 업데이트 완료`);
        return;
      }

      // 데이터만 업데이트
      Plotly.react(chartContainer, plotData, getLayout(), getConfig())
        .then(() => {
          clearLoadingTimeout();
          isLoading = false;
          console.log(`[${chartId}] 차트 업데이트 완료`);
        })
        .catch(err => {
          console.error(`[${chartId}] 차트 업데이트 오류:`, err);
          loadError = '차트 업데이트 중 오류가 발생했습니다';
          clearLoadingTimeout();
          isLoading = false;
          
          // 심각한 오류 시 완전히 다시 초기화
          setTimeout(() => {
            if (chartContainer) {
              try {
                Plotly.purge(chartContainer);
              } catch (e) { /* 무시 */ }
              plotlyInstance = null;
              initChart();
            }
          }, 500);
        });
    } catch (error) {
      console.error(`[${chartId}] 데이터 준비 오류:`, error);
      loadError = '데이터 준비 중 오류가 발생했습니다';
      clearLoadingTimeout();
      isLoading = false;
    }
  }

  // 컴포넌트 라이프사이클 이벤트
  onMount(() => {
    if (!chartContainer) return;
    
    console.log(`[${chartId}] 컴포넌트 마운트`);
    
    // 지연된 초기화로 DOM 준비 시간 확보
    setTimeout(() => {
      dataHashCache.clear();
      prevDataFingerprint = getDataFingerprint();
      initChart();
    }, 200 + Math.random() * 300); // 여러 차트가 있을 때 순차적으로 처리
    
    // 리사이즈 감지
    resizeObserver = new ResizeObserver(() => {
      if (chartContainer && plotlyInstance) {
        Plotly.Plots.resize(chartContainer);
      }
    });
    resizeObserver.observe(chartContainer);

    // Plotly 전역 확장 메소드 정의
    if (!Element.prototype.on) {
      Element.prototype.on = function(eventType: string, callback: Function) {
        this.addEventListener(eventType, callback);
        return this;
      };
    }

    // 이미 활성화된 필터가 있다면 초기 상태에 적용
    if ($filtertrace && 
        ($filtertrace.from_time > 0 || $filtertrace.to_time > 0)) {
      lastFilterUpdate = JSON.stringify($filtertrace);
      console.log(`[${chartId}] 초기 필터 상태 적용:`, $filtertrace);
    }
  });
  
  onDestroy(() => {
    console.log(`[${chartId}] 컴포넌트 소멸`);
    
    clearLoadingTimeout();
    
    if (updateTimeoutId) {
      clearTimeout(updateTimeoutId);
    }
    
    if (resizeObserver) {
      resizeObserver.disconnect();
    }
    
    if (chartContainer) {
      // 이벤트 리스너 제거
      if (chartContainer._relayoutHandler) {
        chartContainer.removeEventListener('plotly_relayout', chartContainer._relayoutHandler);
      }
      if (chartContainer._doubleClickHandler) {
        chartContainer.removeEventListener('plotly_doubleclick', chartContainer._doubleClickHandler);
      }
      
      // Plotly 정리
      try {
        Plotly.purge(chartContainer);
      } catch (e) {
        console.error(`[${chartId}] Plotly 정리 오류:`, e);
      }
    }
    
    // 메모리 정리
    plotlyInstance = null;
    dataHashCache.clear();
  });
</script>

<!-- Title 수정 다이얼로그 -->
<Dialog.Root open={showTitleDialog} onOpenChange={(open) => showTitleDialog = open}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>차트 제목 변경</Dialog.Title>
      <Dialog.Description>
        차트에 표시될 제목을 입력하세요.
      </Dialog.Description>
    </Dialog.Header>
    
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="chart-title" class="text-right">
          제목
        </Label>
        <Input id="chart-title" class="col-span-3" bind:value={inputTitle} />
      </div>
    </div>
    
    <Dialog.Footer>   
      <Button type="submit" onclick={applyTitleChange}>저장</Button>   
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
  
<!-- Symbol 크기 수정 다이얼로그 -->
<Dialog.Root open={showSymbolSizeDialog} onOpenChange={(open) => showSymbolSizeDialog = open}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>포인트 크기 조정</Dialog.Title>
      <Dialog.Description>
        차트에 표시되는 포인트 크기를 조정하세요.
      </Dialog.Description>
    </Dialog.Header>
    
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="symbol-size" class="text-right">
          크기
        </Label>
        <div class="col-span-3">
          <div class="slider-container">
            <Slider 
              id="symbol-size" 
              min={1} 
              max={10} 
              step={0.5} 
              value={[inputSymbolSize]}
              onValueChange={(values) => inputSymbolSize = values[0]} 
            />
          </div>
          <div class="slider-value">{inputSymbolSize}</div>
        </div>
      </div>
    </div>
    
    <Dialog.Footer>   
      <Button type="submit" onclick={applySymbolSizeChange}>적용</Button>   
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- X축 범위 설정 다이얼로그 -->
<Dialog.Root open={showXAxisRangeDialog} onOpenChange={(open) => showXAxisRangeDialog = open}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>X축 범위 설정</Dialog.Title>
      <Dialog.Description>
        {xAxisName} 축의 표시 범위를 설정하세요.
      </Dialog.Description>
    </Dialog.Header>
    
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="x-min" class="text-right">
          최소값
        </Label>
        <Input 
          id="x-min" 
          type="number" 
          class="col-span-3" 
          bind:value={inputXMin} 
          step="any" 
        />
      </div>
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="x-max" class="text-right">
          최대값
        </Label>
        <Input 
          id="x-max" 
          type="number" 
          class="col-span-3" 
          bind:value={inputXMax} 
          step="any" 
        />
      </div>
    </div>
    
    <Dialog.Footer>   
      <Button type="submit" onclick={applyXAxisRange}>적용</Button>   
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Y축 범위 설정 다이얼로그 -->
<Dialog.Root open={showYAxisRangeDialog} onOpenChange={(open) => showYAxisRangeDialog = open}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Y축 범위 설정</Dialog.Title>
      <Dialog.Description>
        {yAxisName} 축의 표시 범위를 설정하세요.
      </Dialog.Description>
    </Dialog.Header>
    
    <div class="grid gap-4 py-4">
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="y-min" class="text-right">
          최소값
        </Label>
        <Input 
          id="y-min" 
          type="number" 
          class="col-span-3" 
          bind:value={inputYMin} 
          step="any" 
        />
      </div>
      <div class="grid grid-cols-4 items-center gap-4">
        <Label for="y-max" class="text-right">
          최대값
        </Label>
        <Input 
          id="y-max" 
          type="number" 
          class="col-span-3" 
          bind:value={inputYMax} 
          step="any" 
        />
      </div>
    </div>
    
    <Dialog.Footer>   
      <Button type="submit" onclick={applyYAxisRange}>적용</Button>   
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
 
<ContextMenu.Root>
  <ContextMenu.Trigger>
    <div class="chart-container" 
         bind:this={chartContainer} 
         data-chart-id={chartId}
         data-chart-type={chartType}
         data-sync={enableSync ? 'true' : 'false'}>
      {#if isLoading}
      <div class="loading-overlay">
        <div class="loading-spinner"></div>
        <div class="loading-text">차트 데이터 준비 중...</div>
      </div>
      {/if}
      {#if loadError}
      <div class="error-overlay">
        <div class="error-icon">⚠️</div>
        <div class="error-text">{loadError}</div>
        <div class="error-actions">
          <Button class="retry-button" size="sm" variant="outline" onclick={() => {
            loadError = "";
            forceUpdate = true;
            initChart();
          }}>재시도</Button>
        </div>
      </div>
      {/if}
    </div>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <ContextMenu.Item on:click={openTitleDialog}>차트 제목 변경</ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item on:click={openSymbolSizeDialog}>포인트 크기 조정</ContextMenu.Item>
    <ContextMenu.Item on:click={openXAxisRangeDialog}>X축 범위 설정</ContextMenu.Item>
    <ContextMenu.Item on:click={openYAxisRangeDialog}>Y축 범위 설정</ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu.Root>

<style>
  .chart-container {
    width: 100%;
    height: 500px;
    position: relative;
    border: 1px solid rgba(0, 0, 0, 0.05);
    border-radius: 4px;
    overflow: hidden;
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
  .retry-button {
    margin-top: 15px;
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
