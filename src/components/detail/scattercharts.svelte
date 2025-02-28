<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as echarts from 'echarts';
  import 'echarts-gl';
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";

  import { trace, selectedTrace, filtertrace } from '$stores/trace';

  interface ScatterChartProps {
    data: any[];
    xAxisKey: string;
    yAxisKey: string;
    legendKey: string;
    xAxisLabel?: string;
    yAxisLabel?: string;
    ycolumn: string;
  }

  let { data, xAxisKey, yAxisKey, legendKey, xAxisLabel = 'time', yAxisLabel = 'sector', ycolumn } :ScatterChartProps = $props();

  let chartTitle = $state('');
  let showTitleDialog = $state(false);
  let inputTitle = $state('');

  // 포인트 크기 설정 다이얼로그
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
  let prevDataHash = '';
  xAxisName = xAxisLabel;
  yAxisName = yAxisLabel;

  prevData = data;

  let chartContainer;
  let chartInstance;
  let resizeObserver;
  console.log('xAxisKey:', xAxisKey, 'yAxisKey:', yAxisKey, 'legendKey:', legendKey, 'xAxisLabel:', xAxisLabel, 'yAxisLabel:', yAxisLabel);

  // zoom 범위 상태
  let { xZoomFrom, xZoomTo, yZoomFrom, yZoomTo } = $state({ xZoomFrom: 0, xZoomTo: 0, yZoomFrom: 0, yZoomTo: 0 });

  // 커스텀 tooltip 상태
  // let tooltipVisible = false;
  // let tooltipContent = '';
  // let tooltipX = 0;
  // let tooltipY = 0;

  let {tooltipVisible, tooltipContent, tooltipX, tooltipY} = $state({tooltipVisible: false, tooltipContent: '', tooltipX: 0, tooltipY: 0});

  // 커스텀 tooltip을 위한 데이터 검색용 global seriesData 저장
  let globalSeriesData = {};

  const WRITE_COLOR = '#FF0000';
  const READ_COLOR = '#0000FF';
  const DISCARD_COLOR = '#00FF00';
  const FLUSH_COLOR = '#FFFF00';

  // UFS 명령어별 고정 컬러 매핑 (원색 사용)
  const UFS_COMMAND_COLORS = {
    // Write 계열 - 빨간색 계열
    '0x2a': '#FF0000', // Write - 순수한 빨간색
    '0xa2': '#FF3333', // Write 관련 - 밝은 빨간색
    
    // Read 계열 - 파란색 계열
    '0x28': '#0000FF', // Read - 순수한 파란색
    '0xb5': '#3333FF', // Read 관련 - 밝은 파란색
    
    // UNMAP 계열 - 녹색 계열
    '0x42': '#00FF00', // UNMAP - 순수한 녹색
    
    // 기타 명령어들 - 다른 원색
    '0x1b': '#FF00FF', // 자주색
    '0x12': '#00FFFF', // 청록색
    '0x35': '#FFFF00', // 노란색
    '0xc0': '#FF8800', // 주황색
  };

  // 블록 타입별 팔레트 (더 선명하고 원색에 가까운 색상으로 업데이트)
  const WRITE_PALETTE = [
    '#FF0000', // 순수 빨강
    '#FF3333',
    '#FF6666',
    '#FF9999',
    '#FFCCCC'
  ];
  
  const READ_PALETTE = [
    '#0000FF', // 순수 파랑
    '#3333FF',
    '#6666FF',
    '#9999FF',
    '#CCCCFF'
  ];
  
  const DISCARD_PALETTE = [
    '#00FF00', // 순수 녹색
    '#33FF33',
    '#66FF66',
    '#99FF99',
    '#CCFFCC'
  ];
  
  const FLUSH_PALETTE = [
    '#FFFF00', // 순수 노랑
    '#FFFF33',
    '#FFFF66',
    '#FFFF99',
    '#FFFFCC'
  ];

  let blockWriteMapping = {};
  let blockReadMapping = {};
  let blockDiscardMapping = {};
  let blockFlushMapping = {};  // 추가: Flush 블록 매핑

  let writePaletteIndex = 0;
  let readPaletteIndex = 0;
  let discardPaletteIndex = 0;
  let flushPaletteIndex = 0;   // 추가: Flush 인덱스

  // 차트 옵션 업데이트 함수
  function updateChartOption(options) {
    if (chartInstance) {
      chartInstance.setOption(options);
    }
  }
  
  // 차트 타이틀 설정 함수
  function setChartTitle(title) {
    chartTitle = title;
    updateChartOption({
      title: {
        text: title,
        left: 'center',
        top: '15px'
      }
    });
  }
  
  // 타이틀 다이얼로그 열기
  function openTitleDialog() {
    inputTitle = chartTitle;
    showTitleDialog = true;
  }
  
  // 타이틀 변경 적용
  function applyTitleChange() {
    console.log('inputTitle:', inputTitle);
    setChartTitle(inputTitle);
    showTitleDialog = false;
  }

  function openSymbolSizeDialog() {
    inputSymbolSize = symbolSize;
    showSymbolSizeDialog = true;
  }

  function applySymbolSizeChange() {
    symbolSize = inputSymbolSize;
    
    // 모든 시리즈의 symbolSize 업데이트
    if (chartInstance) {
      const option = chartInstance.getOption();
      const updatedSeries = option.series.map(series => {
        series.symbolSize = symbolSize;
        return series;
      });
      
      updateChartOption({
        series: updatedSeries
      });
    }
    
    showSymbolSizeDialog = false;
  }

  function getRandomColor() {
    return '#' + Math.floor(Math.random() * 16777215).toString(16);
  }

  function getColorForLegend(legend) {
    if (typeof legend !== 'string') return getRandomColor();

    // UFS 명령어 확인 (0x로 시작하는 경우)
    if (legend.toLowerCase().startsWith('0x')) {
      const cmdLower = legend.toLowerCase();
      // UFS_COMMAND_COLORS에 정의된 색상이 있으면 해당 색상 사용
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
        
      case 'F': // Flush (추가)
        if (!(legend in blockFlushMapping)) {
          blockFlushMapping[legend] = FLUSH_PALETTE[flushPaletteIndex % FLUSH_PALETTE.length];
          flushPaletteIndex++;
        }
        return blockFlushMapping[legend];
        
      default:
        return getRandomColor();
    }
  }
  
  function sortLegends(legends) {
    // 접두어 순서 정의 (중요도 순서대로)
    const prefixOrder = {
      'R': 1, // Read
      'W': 2, // Write
      'D': 3, // Discard
      'F': 4, // Flush
      '0': 5  // 0x로 시작하는 UFS 명령어
    };
    
    // 범례 항목을 접두어 및 번호순으로 정렬
    return [...legends].sort((a, b) => {
      // 접두어 추출 (첫 글자 또는 '0x'인 경우)
      const prefixA = a.toLowerCase().startsWith('0x') ? '0' : a[0].toUpperCase();
      const prefixB = b.toLowerCase().startsWith('0x') ? '0' : b[0].toUpperCase();
      
      // 접두어가 다른 경우 접두어 순서로 정렬
      if (prefixA !== prefixB) {
        const orderA = prefixOrder[prefixA] || 999;
        const orderB = prefixOrder[prefixB] || 999;
        return orderA - orderB; // 접두어 순으로 오름차순
      }
      
      // 접두어가 같은 경우:
      // UFS 명령어(0x로 시작)는 명령어 값으로 정렬
      if (a.toLowerCase().startsWith('0x') && b.toLowerCase().startsWith('0x')) {
        // 16진수 값으로 변환하여 비교
        const valueA = parseInt(a.slice(2), 16);
        const valueB = parseInt(b.slice(2), 16);
        return valueA - valueB;
      }
      
      // 일반 작업은 숫자 부분 추출해서 정렬 (예: W1, W2, W10...)
      const numA = parseInt((a.match(/\d+/) || ['0'])[0]);
      const numB = parseInt((b.match(/\d+/) || ['0'])[0]);
      return numA - numB;
    });
  }

  let seriesColorMap = {};
  let legends = [];
  
  function prepareChartData() {
    let seriesData = {};
    if (!data) return [];
    data.forEach(item => {
      const x = parseFloat(item[xAxisKey]);
      const y = parseFloat(item[yAxisKey]);
      

      if (!isNaN(x) && !isNaN(y)) {
        const legend = item[legendKey];
        if (!seriesData[legend]) seriesData[legend] = [];
        seriesData[legend].push([x, y]);
      }
    });
    // 각 legend의 데이터들을 x 값 기준 오름차순 정렬
    Object.keys(seriesData).forEach(legend => {
      seriesData[legend].sort((a, b) => a[0] - b[0]);
    });

    // legend 목록 설정 및 정렬
    legends = sortLegends(Object.keys(seriesData));

    // legend 목록 및 색상 매핑 설정
    legends.forEach(legend => {
      seriesColorMap[legend] = getColorForLegend(legend);
    });
    // global 변수에 저장 (custom tooltip용)
    globalSeriesData = seriesData;
    // scatterGL 시리즈 생성
    const series = legends.map(legend => {
      return {
        name: legend,
        type: 'scatter',
        large: true,
        largeThreshold: 2000,
        // 애니메이션 설정
        animation: true,
        animationDuration: 1500,
        animationEasing: 'cubicOut',  // 애니메이션 이징 효과
        animationDelay: function(idx) {
          return idx * 5; // 데이터 포인트마다 지연 적용
        },
        animationDurationUpdate: 300, // 데이터 업데이트 시 애니메이션 지속 시간
        animationEasingUpdate: 'cubicInOut', // 업데이트 시 이징 효과
        animationDelayUpdate: function(idx) {
          return idx * 5; // 데이터 포인트마다 지연 적용
        },
        symbolSize: symbolSize,
        data: seriesData[legend],
        itemStyle: {
          color: seriesColorMap[legend]
        }
      }
    });
    return series;
  }
  function calculateYAxisPadding() {
    let maxDigits = 0;
    
    // 모든 시리즈의 y값을 확인
    Object.keys(globalSeriesData).forEach(seriesName => {
        globalSeriesData[seriesName].forEach(point => {
            // 소수점 2자리까지만 표시하도록 변환
            const yValue = Number(point[1]).toFixed(2);
            // 소수점을 포함한 전체 자리수 계산 (소수점 제외)
            const digits = yValue.replace('.', '').length;
            maxDigits = Math.max(maxDigits, digits);
        });
    });

    // 8자리일 때 60이므로, 자리수에 비례하여 padding 계산
    const basePadding = 60;
    const baseDigits = 8;
    const calculatedPadding = Math.ceil((maxDigits * basePadding) / baseDigits);
    
    console.log('Max digits (including 2 decimal places):', maxDigits, 'Calculated padding:', calculatedPadding);
    return calculatedPadding;
  }  

  function initChart() {
    const series = prepareChartData();
    const dynamicPadding = calculateYAxisPadding();
    
    chartInstance = echarts.init(chartContainer);
    const option = {
      // 기본 tooltip 비활성화
      tooltip: { show: false },
      toolbox: {
        show: true,
        feature: {
          dataZoom: {},
          dataView: { readOnly: false },
          magicType: { type: ['line', 'bar'] },
          restore: {},
          saveAsImage: {}
        }
      },
      title: {
        text: chartTitle,
        left: 'center',
        top: '15px'
      },
      legend: {
        data: legends,
        show: legendshow,
        orient: legendorient,
        left: 'right',
        top: 'middle',
        align: 'left',       
        padding: [5, 10, 5, 10], // 상, 우, 하, 좌 여백
        itemGap: 8, // 아이템 간격
        // 테두리 추가
        borderColor: '#e6f7ff',       // 밝은 하늘색 테두리
        borderWidth: 1,
        borderRadius: 8,              // 더 둥근 모서리
        backgroundColor: 'rgba(255, 255, 255, 0.9)', // 더 투명한 배경
        // 부드러운 그림자
        shadowBlur: 10,
        shadowColor: 'rgba(0, 145, 234, 0.2)',
        shadowOffsetX: 3,
        shadowOffsetY: 3,
         // 글꼴 스타일
         textStyle: {
          fontSize: 12,
          fontWeight: 'normal',
          color: '#555'              // 부드러운 회색 텍스트
        },
        formatter: function(name) {
          // 긴 이름의 경우 짧게 표시
          if (name.length > 10) {
            return name.slice(0, 10) + '...';
          }
          return name;
        } 
      },
      grid: {
        left: '10%',
        right: '15%',      // 오른쪽 여백 늘려서 legend 공간 확보
        top: '60px',
        bottom: '60px',
        containLabel: true // 라벨까지 포함
      },
      xAxis: {
        type: 'value',
        scale: true,
        name: xAxisName,
        nameLocation: 'middle',
        nameTextStyle: {
          fontSize: 13,
          padding: 15, 
          fontWeight: 'bolder'
        }
      },
      yAxis: {
        type: 'value',
        scale: true,
        name: yAxisName,
        nameLocation: 'middle',
        nameTextStyle: {
          fontSize: 13,
          padding: dynamicPadding, // 동적으로 계산된 padding 적용
          fontWeight: 'bolder'
        }
      },
      series: series
    };
    chartInstance.setOption(option);
    console.log('series:', series);
    // restore 이벤트 핸들러 추가: restore 시 custom 상태 초기화
    chartInstance.on('restore', () => {
      tooltipVisible = false;
      console.log('restore event');
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: 0,
        to_time: 0,
        from_lba: 0,
        to_lba: 0
      }; 
    });

    // 커스텀 tooltip을 위한 mousemove, mouseleave 이벤트 등록
    chartContainer.addEventListener('mousemove', handleMouseMove);
    chartContainer.addEventListener('mouseleave', () => { tooltipVisible = false; });
    console.log('chart initialized');
    // 기존 dataZoom 이벤트도 등록 (옵션: zoom 정보 표시용)
    chartInstance.on('datazoom', function () {
      const xAxisModel = chartInstance.getModel().getComponent('xAxis', 0);
      if (xAxisModel) {
        const xExtent = xAxisModel.axis.scale.getExtent();
        xZoomFrom = xExtent[0];
        xZoomTo = xExtent[1];
      }
      const yAxisModel = chartInstance.getModel().getComponent('yAxis', 0);
      if (yAxisModel) {
        const yExtent = yAxisModel.axis.scale.getExtent();
        yZoomFrom = yExtent[0];
        yZoomTo = yExtent[1];
      }
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: xZoomFrom,
        to_time: xZoomTo,
        from_lba: yZoomFrom,
        to_lba: yZoomTo
      };
      console.log('filtertrace:', $filtertrace);
    });
    console.log('datazoom event registered');
    
  }
  // 이전 데이터와 비교를 위한 hash 함수
  function hashData(data) {
    if (data?.length === 0) return '';
    else if (data?.length > 0) return data[0][xAxisKey] + data[0][yAxisKey] + data[0][legendKey];
    return '';
  }

  $effect(() => {
    const currentDataHash = hashData(data);
    if (prevDataHash !== currentDataHash) {
      console.log('data changed:');
      prevDataHash = currentDataHash;
      if (chartInstance) {
        const series = prepareChartData();
        
        // 차트 옵션 업데이트
        chartInstance.setOption({
          series: series
        }, { replaceMerge: ['series'] }); // series만 완전히 교체
      }
    }
  });

  

  // 간단한 nearest point 검색 (임계값: 1 단위)
  function handleMouseMove(e) {
    const rect = chartContainer.getBoundingClientRect();
    const offsetX = e.clientX - rect.left;
    const offsetY = e.clientY - rect.top;
    // pixel → data 좌표 변환 (grid 좌표계 사용)
    const dataCoord = chartInstance.convertFromPixel('grid', [offsetX, offsetY]);
    
    let minDist = Infinity;
    let nearestPoint = null;
    // 각 series 데이터에서 가까운 점을 탐색
    Object.keys(globalSeriesData).forEach(seriesName => {
      globalSeriesData[seriesName].forEach(point => {
        const dx = point[0] - dataCoord[0];
        const dy = point[1] - dataCoord[1];

        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < minDist && dist < 100) { // 임계값 100 (필요에 따라 조정)
          minDist = dist;
          nearestPoint = { seriesName, x: point[0], y: point[1] };
        }
      });
    });
    if (nearestPoint) {
      tooltipContent = `${nearestPoint.seriesName}<br>${xAxisLabel}: ${nearestPoint.x}<br>${yAxisLabel}: ${nearestPoint.y}`;
      tooltipX = e.pageX;
      tooltipY = e.pageY;      
      tooltipVisible = true;
    } else {
      tooltipVisible = false;
    }
  }

  function updateChartSize() {
    if (chartInstance) {
      chartInstance.resize();
    }
  }

  onMount(() => {
    initChart();
    resizeObserver = new ResizeObserver(entries => {
      for (let entry of entries) {
        if (entry.target === chartContainer) {
          updateChartSize();
        }
      }
    });
    resizeObserver.observe(chartContainer);
    console.log('chart mounted');
  });
  
  onDestroy(() => {
    chartContainer.removeEventListener('mousemove', handleMouseMove);
    if (resizeObserver) {
      resizeObserver.disconnect();
    }
    if (chartInstance) {
      chartInstance.dispose();
    }
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
      <Button type="submit" onclick={applyTitleChange}>Save changes</Button>   
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
 
<ContextMenu.Root>
  <ContextMenu.Trigger>
    <div class="chart-container" bind:this={chartContainer}></div>

    {#if tooltipVisible}
      <div class="custom-tooltip" style="left: {tooltipX + 70}px; top: {tooltipY + 10}px;">
        {@html tooltipContent}
      </div>
    {/if}

    <!-- {#if xZoomFrom !== 0 && xZoomTo !== 0 && yZoomFrom !== 0 && yZoomTo !== 0}
      <div class="zoom-info">
        X: {xZoomFrom} ~ {xZoomTo}<br>
        Y: {yZoomFrom} ~ {yZoomTo}
      </div>
    {/if} -->
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <ContextMenu.Item on:click={openTitleDialog}>차트 제목 변경</ContextMenu.Item>
    <ContextMenu.Separator />
    <ContextMenu.Item on:click={openSymbolSizeDialog}>포인트 크기 조정</ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu.Root>



<style>
  .chart-container {
    width: 100%;
    height: 500px;
    position: relative;
  }
  .custom-tooltip {
    position: absolute;
    background: rgba(0,0,0,0.7);
    color: #fff;
    padding: 4px 8px;
    font-size: 12px;
    border-radius: 4px;
    pointer-events: none;
    white-space: nowrap;
    z-index: 100;
    /* tooltip 위치를 마우스 좌표보다 위쪽으로 이동 */
    transform: translate(-50%, -100%);
  }
  .zoom-info {
    position: absolute;
    bottom: 10px;
    left: 10px;
    background: rgba(255,255,255,0.8);
    padding: 4px 8px;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 12px;
    z-index: 10;
  }
  /* 슬라이더 스타일 */
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
