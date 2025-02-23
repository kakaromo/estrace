<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as echarts from 'echarts';
  import 'echarts-gl';
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";

  import * as Dialog from "$lib/components/ui/dialog";

  import { trace, selectedTrace, filtertrace } from '$stores/trace';

  let { data, xAxisKey, yAxisKey, legendKey, xAxisLabel = 'time', yAxisLabel = 'sector' } = $props();

  let title = $state('');
  let symbolSize = $state(3);
  let xAxisName = $state('time');
  let yAxisName = $state('sector');
  let legendposition = $state('top');
  let legendorient = $state('horizontal');
  let legendshow = $state(true);
  let tooltipshow = $state(true);

  xAxisName = xAxisLabel;
  yAxisName = yAxisLabel;

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
  const DISCARD_COLOR = '#800080';

  const WRITE_PALETTE = ['#FFCCCC', '#FF9999', '#FF6666', '#FF3333', '#FF0000'];
  const READ_PALETTE = ['#CCCCFF', '#9999FF', '#6666FF', '#3333FF', '#0000FF'];
  const DISCARD_PALETTE = ['#E6CCE6', '#D9B3D9', '#CC99CC', '#BF80BF', '#B266B2'];

  let blockWriteMapping = {};
  let blockReadMapping = {};
  let blockDiscardMapping = {};
  let writePaletteIndex = 0;
  let readPaletteIndex = 0;
  let discardPaletteIndex = 0;

  function getRandomColor() {
    return '#' + Math.floor(Math.random() * 16777215).toString(16);
  }

  function getColorForLegend(legend) {
    if (typeof legend !== 'string') return getRandomColor();

    if (legend.toLowerCase().startsWith('0x')) {
      if (legend.toLowerCase() === '0x2a') return WRITE_COLOR;
      if (legend.toLowerCase() === '0x28') return READ_COLOR;
      if (legend.toLowerCase() === '0x42') return DISCARD_COLOR;
    }
    const prefix = legend[0].toUpperCase();
    if (prefix === 'W') {
      if (!(legend in blockWriteMapping)) {
        blockWriteMapping[legend] = WRITE_PALETTE[writePaletteIndex % WRITE_PALETTE.length];
        writePaletteIndex++;
      }
      return blockWriteMapping[legend];
    } else if (prefix === 'R') {
      if (!(legend in blockReadMapping)) {
        blockReadMapping[legend] = READ_PALETTE[readPaletteIndex % READ_PALETTE.length];
        readPaletteIndex++;
      }
      return blockReadMapping[legend];
    } else if (prefix === 'D') {
      if (!(legend in blockDiscardMapping)) {
        blockDiscardMapping[legend] = DISCARD_PALETTE[discardPaletteIndex % DISCARD_PALETTE.length];
        discardPaletteIndex++;
      }
      return blockDiscardMapping[legend];
    }
    return getRandomColor();
  }

  let seriesColorMap = {};
  let legends = [];
  function prepareChartData() {
    let seriesData = {};
    data.forEach(item => {
      const x = parseFloat(item[xAxisKey]);
      const y = parseFloat(item[yAxisKey]);
      

      if (!isNaN(x) && !isNaN(y) && y !== 0) {
        const legend = item[legendKey];
        if (!seriesData[legend]) seriesData[legend] = [];
        seriesData[legend].push([x, y]);
      }
    });
    // 각 legend의 데이터들을 x 값 기준 오름차순 정렬
    Object.keys(seriesData).forEach(legend => {
      seriesData[legend].sort((a, b) => a[0] - b[0]);
    });
    // legend 목록 및 색상 매핑 설정
    legends = Object.keys(seriesData);
    legends.forEach(legend => {
      seriesColorMap[legend] = getColorForLegend(legend);
    });
    // global 변수에 저장 (custom tooltip용)
    globalSeriesData = seriesData;
    // scatterGL 시리즈 생성
    const series = legends.map(legend => {
      return {
        name: legend,
        type: 'scatterGL',
        symbolSize: symbolSize,
        data: seriesData[legend],
        itemStyle: {
          color: seriesColorMap[legend]
        }
      }
    });
    return series;
  }

  function initChart() {
    const series = prepareChartData();
    
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
      legend: {
        data: legends,
        show: legendshow,
        orient: legendorient,
        left: 'center',
        top: legendposition
      },
      xAxis: {
        type: 'value',
        scale: true,
        name: xAxisName
      },
      yAxis: {
        type: 'value',
        scale: true,
        name: yAxisName
      },
      series: series
    };
    chartInstance.setOption(option);
    console.log('series:', series);
    // restore 이벤트 핸들러 추가: restore 시 custom 상태 초기화
    chartInstance.on('restore', () => {
      tooltipVisible = false;
      xZoomFrom = 0;
      xZoomTo = 0;
      yZoomFrom = 0;
      yZoomTo = 0;      
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
    });
    console.log('datazoom event registered');
    $filtertrace = {
          from_time: xZoomFrom,
          to_time: xZoomTo,
          from_lba: yZoomFrom,
          to_lba: yZoomTo
      };
    console.log('filtertrace:', $filtertrace);
  }

  $effect(() => {
    $filtertrace = {
          from_time: xZoomFrom,
          to_time: xZoomTo,
          from_lba: yZoomFrom,
          to_lba: yZoomTo
      };
		// if (!['lba', 'sector'].includes(yAxisKey)) {
    //   const lbaRange = findLbaRangeInTimeRange(xZoomFrom, xZoomTo);
    //   $filtertrace = {
    //       from_time: xZoomFrom,
    //       to_time: xZoomTo,
    //       from_lba: lbaRange.from,
    //       to_lba: lbaRange.to
    //   };
    // } else {
    //   $filtertrace = {
    //       from_time: xZoomFrom,
    //       to_time: xZoomTo,
    //       from_lba: yZoomFrom,
    //       to_lba: yZoomTo
    //   };
    // }
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
  
  // y축이 latency 관련일 때 해당 시간대의 lba 범위를 찾는 함수
  function findLbaRangeInTimeRange(timeFrom: number, timeTo: number) {
    let minLba = Infinity;
    let maxLba = -Infinity;

    Object.keys(globalSeriesData).forEach(seriesName => {
        globalSeriesData[seriesName].forEach(point => {
            const time = point[0];
            const lba = data.find(item => 
                item[xAxisKey] === time && 
                item[legendKey] === seriesName
            )?.[yAxisKey === 'lba' ? yAxisKey : 'lba'];

            if (time >= timeFrom && time <= timeTo && lba !== undefined) {
                minLba = Math.min(minLba, lba);
                maxLba = Math.max(maxLba, lba);
            }
        });
    });

    return {
        from: minLba === Infinity ? 0 : minLba,
        to: maxLba === -Infinity ? 0 : maxLba
    };
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
</style>

  
 
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
    <ContextMenu.Item>Item 1</ContextMenu.Item>
    <ContextMenu.Item>Item 2</ContextMenu.Item>
    <ContextMenu.Item>Item 3</ContextMenu.Item>
  </ContextMenu.Content>
</ContextMenu.Root>

