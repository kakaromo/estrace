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

  import { filtertrace } from '$stores/trace';

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

  // 차트 상태 변수
  let chartTitle = $state('');
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
  let prevDataHash = '';
  
  // 축 이름 초기화
  xAxisName = xAxisLabel;
  yAxisName = yAxisLabel;
  prevData = data;

  // 차트 요소
  let chartContainer: any;
  let chartInstance: any;
  let resizeObserver: any;

  // zoom 범위 및 tooltip 상태
  let { xZoomFrom, xZoomTo, yZoomFrom, yZoomTo } = $state({ xZoomFrom: 0, xZoomTo: 0, yZoomFrom: 0, yZoomTo: 0 });
  let {tooltipVisible, tooltipContent, tooltipX, tooltipY} = $state({tooltipVisible: false, tooltipContent: '', tooltipX: 0, tooltipY: 0});
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

  // 차트 옵션 업데이트 함수
  function updateChartOption(options: { title?: { text: any; left: string; top: string; }; series?: any; }) {
    if (chartInstance) {
      chartInstance.setOption(options);
    }
  }
  
  // 차트 타이틀 관련 함수
  function setChartTitle(title: string) {
    chartTitle = title;
    updateChartOption({
      title: {
        text: title,
        left: 'center',
        top: '15px'
      }
    });
  }
  
  function openTitleDialog() {
    inputTitle = chartTitle;
    showTitleDialog = true;
  }
  
  function applyTitleChange() {
    setChartTitle(inputTitle);
    showTitleDialog = false;
  }

  // 포인트 크기 관련 함수
  function openSymbolSizeDialog() {
    inputSymbolSize = symbolSize;
    showSymbolSizeDialog = true;
  }

  function applySymbolSizeChange() {
    symbolSize = inputSymbolSize;
    
    if (chartInstance) {
      const option = chartInstance.getOption();
      const updatedSeries = option.series.map((series: { symbolSize: number; }) => {
        series.symbolSize = symbolSize;
        return series;
      });
      
      updateChartOption({
        series: updatedSeries
      });
    }
    
    showSymbolSizeDialog = false;
  }

  // 색상 관련 함수
  function getRandomColor() {
    return '#' + Math.floor(Math.random() * 16777215).toString(16);
  }

  // CPU 색상 매핑용 객체 추가
  let cpuColorMapping: Record<string, string> = {};

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

  // X축 범위 설정 함수
  function openXAxisRangeDialog() {
    // 현재 차트의 X축 범위 가져오기
    const xAxisModel = chartInstance.getModel().getComponent('xAxis', 0);
    if (xAxisModel) {
      const xExtent = xAxisModel.axis.scale.getExtent();
      inputXMin = xExtent[0];
      inputXMax = xExtent[1];      
    }
    showXAxisRangeDialog = true;
  }
  
  function applyXAxisRange() {
    if (chartInstance && inputXMin < inputXMax) {
      // X축 범위 설정
      chartInstance.setOption({
        xAxis: {
          min: inputXMin,
          max: inputXMax
        }
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
  function openYAxisRangeDialog() {
    // 현재 차트의 Y축 범위 가져오기
    const yAxisModel = chartInstance.getModel().getComponent('yAxis', 0);
    if (yAxisModel) {
      const yExtent = yAxisModel.axis.scale.getExtent();
      inputYMin = yExtent[0];
      inputYMax = yExtent[1];
    }
    showYAxisRangeDialog = true;
  }
  
  function applyYAxisRange() {
    if (chartInstance && inputYMin < inputYMax) {
      // Y축 범위 설정
      chartInstance.setOption({
        yAxis: {
          min: inputYMin,
          max: inputYMax
        }
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

  let seriesColorMap: Record<string, string> = {};
  let legends: string[] = [];
  
  // 차트 데이터 준비 함수
function prepareChartData() {
  let seriesData: Record<string, number[][]> = {};
  if (!data) return [];
  
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
      const legend = String(item[legendKey]);
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
  
  // 시리즈 생성
  return legends.map(legend => {
    return {
      name: legend,
      type: 'scatter',
      large: true,
      largeThreshold: 2000,
      animation: true,
      animationDuration: 1500,
      animationEasing: 'cubicOut',
      animationDelay: function(idx: number) {
        return idx * 5;
      },
      animationDurationUpdate: 300,
      animationEasingUpdate: 'cubicInOut',
      animationDelayUpdate: function(idx: number) {
        return idx * 5;
      },
      symbolSize: symbolSize,
      data: seriesData[legend],
      itemStyle: {
        color: seriesColorMap[legend]
      }
    }
  });
}

  // Y축 패딩 계산
  function calculateYAxisPadding() {
    let maxDigits = 0;
    
    Object.keys(globalSeriesData).forEach(seriesName => {
        globalSeriesData[seriesName].forEach((point: any[]) => {
            const yValue = Number(point[1]).toFixed(2);
            const digits = yValue.replace('.', '').length;
            maxDigits = Math.max(maxDigits, digits);
        });
    });

    const basePadding = 60;
    const baseDigits = 8;
    return Math.ceil((maxDigits * basePadding) / baseDigits);
  }

  // 차트 초기화
  function initChart() {
    const series = prepareChartData();
    const dynamicPadding = calculateYAxisPadding();
    
    chartInstance = echarts.init(chartContainer);
    const option = {
      tooltip: {
        show: true, // 내장 툴팁 활성화
        trigger: 'item',
        formatter: function(params: { seriesName: any; value: any[]; }) {
          // 데이터 포인트에 대한 상세 정보 표시
          return `${params.seriesName}<br/>
                  ${xAxisName}: ${params.value[0]}<br/>
                  ${yAxisName}: ${params.value[1]}`;
        },
        backgroundColor: 'rgba(0,0,0,0.7)',
        textStyle: {
          color: '#fff'
        },
        borderRadius: 4
      },
      toolbox: {
        show: true,
        feature: {
          dataZoom: { },
          dataView: { readOnly: false },
          restore: {},
          saveAsImage: {}
        }
      },
      dataZoom: [
        {
          type: 'inside',
          id: 'dataZoomX',
          xAxisIndex: [0],
          start: 0,
          end: 100
        },
        {
          type: 'inside',
          id: 'dataZoomY',
          yAxisIndex: [0],
          start: 0,
          end: 100
        }
      ],
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
        padding: [5, 10, 5, 10],
        itemGap: 8,
        borderColor: '#e6f7ff',
        borderWidth: 1,
        borderRadius: 8,
        backgroundColor: 'rgba(255, 255, 255, 0.9)',
        shadowBlur: 10,
        shadowColor: 'rgba(0, 145, 234, 0.2)',
        shadowOffsetX: 3,
        shadowOffsetY: 3,
        textStyle: {
          fontSize: 12,
          fontWeight: 'normal',
          color: '#555'
        },
        formatter: function(name: string) {
          if (name.length > 10) {
            return name.slice(0, 10) + '...';
          }
          return name;
        } 
      },
      grid: {
        left: '10%',
        right: '15%',
        top: '60px',
        bottom: '60px',
        containLabel: true
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
          padding: dynamicPadding,
          fontWeight: 'bolder'
        }
      },
      series: series
    };
    chartInstance.setOption(option);

    // restore 이벤트 핸들러
    chartInstance.on('restore', () => {
      tooltipVisible = false;
      
      $filtertrace = {
        zoom_column: ycolumn,
        from_time: 0,
        to_time: 0,
        from_lba: 0,
        to_lba: 0
      };
    });

    // X축 dataZoom 명시적으로 초기화
    chartInstance.dispatchAction({
      type: 'dataZoom',
      id: 'dataZoomX',
      start: 0,
      end: 100
    });
    
    // Y축 dataZoom 명시적으로 초기화
    chartInstance.dispatchAction({
      type: 'dataZoom',
      id: 'dataZoomY',
      start: 0,
      end: 100
    });
    
    // 내부 상태 변수도 초기화
    xZoomFrom = 0;
    xZoomTo = 0;
    yZoomFrom = 0;
    yZoomTo = 0;
    
    // 차트 업데이트 (필요한 경우)
    chartInstance.setOption({
      xAxis: { scale: true },
      yAxis: { scale: true }
    });

    // 이벤트 핸들러 등록
    chartContainer.addEventListener('mousemove', handleMouseMove);
    chartContainer.addEventListener('mouseleave', () => { tooltipVisible = false; });

    // datazoom 이벤트
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
    });

    chartInstance.on('datazoom')
  }

  // 데이터 변경 감지 및 차트 업데이트
  function hashData(data: string | any[]) {
    if (!data || data?.length === 0) return '';
  
    // filtertrace 값에 기반한 해시 생성
    if ($filtertrace) {
      const filterHash = JSON.stringify({
        zoom_column: $filtertrace.zoom_column,
        from_time: $filtertrace.from_time,
        to_time: $filtertrace.to_time,
        from_lba: $filtertrace.from_lba,
        to_lba: $filtertrace.to_lba
      });
      
      // 데이터가 있다면 첫 항목도 함께 고려
      if (data.length > 0) {
        return filterHash + data[0][xAxisKey] + data[0][yAxisKey] + data[0][legendKey];
      }
      return filterHash;
    }
    
    // filtertrace가 없는 경우 기존 방식으로 폴백
    if (data.length > 0) {
      return data[0][xAxisKey] + data[0][yAxisKey] + data[0][legendKey];
    }
    return '';
  }

  $effect(() => {
    const currentDataHash = hashData(data);
       
    if (prevDataHash !== currentDataHash) {
      prevDataHash = currentDataHash;
      if (chartInstance) {
        const series = prepareChartData();
        chartInstance.setOption({
          series: series
        }, { replaceMerge: ['series'] });
      }
    }
  });

  // 마우스 이벤트 처리
  function handleMouseMove(e: any) {
    // const rect = chartContainer.getBoundingClientRect();
    // const offsetX = e.clientX - rect.left;
    // const offsetY = e.clientY - rect.top;
    // const dataCoord = chartInstance.convertFromPixel('grid', [offsetX, offsetY]);
    
    // let minDist = Infinity;
    // let nearestPoint = null;
    
    // Object.keys(globalSeriesData).forEach(seriesName => {
    //   globalSeriesData[seriesName].forEach(point => {
    //     const dx = point[0] - dataCoord[0];
    //     const dy = point[1] - dataCoord[1];
    //     const dist = Math.sqrt(dx * dx + dy * dy);
        
    //     if (dist < minDist && dist < 100) {
    //       minDist = dist;
    //       nearestPoint = { seriesName, x: point[0], y: point[1] };
    //     }
    //   });
    // });
    
    // if (nearestPoint) {
    //   tooltipContent = `${nearestPoint.seriesName}<br>${xAxisLabel}: ${nearestPoint.x}<br>${yAxisLabel}: ${nearestPoint.y}`;
    //   tooltipX = e.pageX;
    //   tooltipY = e.pageY;      
    //   tooltipVisible = true;
    // } else {
    //   tooltipVisible = false;
    // }
  }

  function updateChartSize() {
    if (chartInstance) {
      chartInstance.resize();
    }
  }

  // 컴포넌트 라이프사이클 이벤트
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
    <div class="chart-container" bind:this={chartContainer}></div>

    {#if tooltipVisible}
      <div class="custom-tooltip" style="left: {tooltipX + 70}px; top: {tooltipY + 10}px;">
        {@html tooltipContent}
      </div>
    {/if}
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
    transform: translate(-50%, -100%);
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
