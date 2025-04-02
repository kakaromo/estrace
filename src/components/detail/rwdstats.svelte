<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import * as echarts from 'echarts/core';
    import { PieChart } from 'echarts/charts';
    import {
        TitleComponent,
        TooltipComponent,
        LegendComponent,
        GridComponent,
        DatasetComponent,
        TransformComponent
    } from 'echarts/components';
    import { LabelLayout, UniversalTransition } from 'echarts/features';
    import { CanvasRenderer } from 'echarts/renderers';

    // ECharts 초기화
    echarts.use([
        TitleComponent,
        TooltipComponent,
        LegendComponent,
        GridComponent,
        DatasetComponent,
        TransformComponent,
        PieChart,
        LabelLayout,
        UniversalTransition,
        CanvasRenderer
    ]);

    // props 정의
    interface RWDStatsProps {
        data: any;
        tracetype: string;
        isrwd: boolean;
    }
    let { data, tracetype, isrwd = false } : RWDStatsProps = $props();
    
    // 이전 데이터 해시 (변경 감지용)
    let prevDataHash = '';

    // 차트 컨테이너 및 차트 정의
    let chartElements = $state<{
        [key: string]: { container: null | HTMLElement, instance: null | echarts.ECharts };
        read: { container: null | HTMLElement, instance: null | echarts.ECharts };
        write: { container: null | HTMLElement, instance: null | echarts.ECharts };
        discard: { container: null | HTMLElement, instance: null | echarts.ECharts };
    }>({
        read: { container: null, instance: null },
        write: { container: null, instance: null },
        discard: { container: null, instance: null }
    });
    
    // 차트 컨테이너 참조
    let chartsContainer = $state<HTMLDivElement | null>(null);
    
    // 차트 초기화 타이머
    let chartInitTimer: NodeJS.Timeout | null = null;
    
    // 차트 데이터 상태
    let chartData = $state<{
        pieData: { [key: string]: any[] },
        stats: { [key: string]: any }
    } | null>(null);
    
    // IO 타입 정의
    const IO_TYPES = [
        {
            id: 'read',
            key: (tracetype: string) => tracetype === 'ufs' ? '0x28' : 'R',
            title: (tracetype: string) => `${tracetype.toUpperCase()} Read Continuity`,
            name: 'Read Continuity',
            colors: ['#4ade80', '#fb7185'],
            statTitle: 'Read Statistics'
        },
        {
            id: 'write',
            key: (tracetype: string) => tracetype === 'ufs' ? '0x2a' : 'W',
            title: (tracetype: string) => `${tracetype.toUpperCase()} Write Continuity`,
            name: 'Write Continuity',
            colors: ['#60a5fa', '#f97316'],
            statTitle: 'Write Statistics'
        },
        {
            id: 'discard',
            key: (tracetype: string) => tracetype === 'ufs' ? '0x42' : 'D',
            title: (tracetype: string) => {
                const label = tracetype === 'ufs' ? 'UNMAP' : 'Discard';
                return `${tracetype.toUpperCase()} ${label} Continuity`;
            },
            name: (tracetype: string) => {
                const label = tracetype === 'ufs' ? 'UNMAP' : 'Discard';
                return `${label} Continuity`;
            },
            colors: ['#a855f7', '#fbbf24'],
            statTitle: (tracetype: string) => tracetype === 'ufs' ? 'UNMAP Statistics' : 'Discard Statistics'
        }
    ];
    
    // 바이트 단위 포맷팅
    function formatBytes(bytes: number): string {
        if (bytes === 0 || isNaN(bytes)) return '0 Bytes';
        
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }
    
    // 차트 데이터 전처리
    function prepareChartData() {
        // 기본 데이터 구조
        const result: {
            pieData: { [key: string]: any[] },
            stats: { [key: string]: any }
        } = {
            pieData: {},
            stats: {}
        };
        
        // 데이터가 없을 경우 기본값 반환
        if (!data || !data.op_stats) {
            IO_TYPES.forEach(type => {
                result.pieData[type.id] = [
                    { value: 0, name: 'Cont.', itemStyle: { color: type.colors[0] } },
                    { value: 0, name: 'Discont.', itemStyle: { color: type.colors[1] } }
                ];
                
                result.stats[type.id] = { 
                    total: 0, 
                    ratio: '0.00', 
                    bytes: '0 Bytes', 
                    bytesRatio: '0.00' 
                };
            });
            
            return result;
        }
        
        // 각 IO 타입별로 데이터 처리
        IO_TYPES.forEach(type => {
            const key = type.key(tracetype);
            const ioData = data.op_stats[key] || {
                continuous: 0, 
                non_continuous: 0,
                ratio: 0,
                total_bytes: 0,
                continuous_bytes: 0,
                bytes_ratio: 0
            };
            
            // 파이 차트 데이터
            result.pieData[type.id] = [
                { value: ioData.continuous || 0, name: 'Cont.', itemStyle: { color: type.colors[0] } },
                { value: ioData.non_continuous || 0, name: 'Discont.', itemStyle: { color: type.colors[1] } }
            ];
            
            // 통계 정보
            result.stats[type.id] = {
                total: (ioData.continuous || 0) + (ioData.non_continuous || 0),
                ratio: (ioData.ratio || 0).toFixed(2),
                bytes: formatBytes(ioData.total_bytes || 0),
                bytesRatio: (ioData.bytes_ratio || 0).toFixed(2)
            };
        });
        
        return result;
    }
    
    // 차트 옵션 생성
    function createChartOption(ioType: { id: string; key: (tracetype: any) => "0x28" | "R"; title: (tracetype: any) => string; name: string; colors: string[]; statTitle: string; } | { id: string; key: (tracetype: any) => "0x2a" | "W"; title: (tracetype: any) => string; name: string; colors: string[]; statTitle: string; } | { id: string; key: (tracetype: string) => "0x42" | "D"; title: (tracetype: any) => string; name: (tracetype: any) => string; colors: string[]; statTitle: (tracetype: any) => "UNMAP Statistics" | "Discard Statistics"; }, pieData: any[]) {
        const title = typeof ioType.title === 'function' ? ioType.title(tracetype) : ioType.title;
        const name = typeof ioType.name === 'function' ? ioType.name(tracetype) : ioType.name;
        
        // 모든 값이 0인지 확인
        const isAllZero = pieData.every((item: { value: number; }) => item.value === 0);
        
        return {
            title: {
                text: title,
                left: 'center',
                top: 10,
                textStyle: {
                    fontSize: 14
                }
            },
            tooltip: {
                trigger: 'item',
                formatter: '{b}: {c} ({d}%)'
            },
            legend: {
                orient: 'horizontal',
                bottom: 10,
                left: 'center',
                data: ['Cont.', 'Discont.']
            },
            series: [
                {
                    name: name,
                    type: 'pie',
                    radius: ['40%', '70%'],
                    center: ['50%', '50%'],
                    avoidLabelOverlap: false,
                    itemStyle: {
                        borderRadius: 4,
                        borderColor: '#fff',
                        borderWidth: 2
                    },
                    label: {
                        show: !isAllZero,
                        position: 'outside',
                        formatter: '{b}: {d}%',
                        fontSize: 12
                    },
                    emphasis: {
                        label: {
                            show: true,
                            fontSize: 14,
                            fontWeight: 'bold'
                        }
                    },
                    data: pieData,
                    ...(isAllZero && {
                        label: {
                            show: true,
                            position: 'center',
                            formatter: 'No Data',
                            fontSize: 14,
                            fontWeight: 'bold',
                            color: '#999'
                        }
                    })
                }
            ]
        };
    }
    
    // 차트 초기화 또는 업데이트
    function initOrUpdateCharts() {
        // 데이터 준비
        chartData = prepareChartData();
        
        // 각 IO 타입 처리
        IO_TYPES.forEach(ioType => {
            const chartElement = chartElements[ioType.id];
            
            if (!chartElement || !chartElement.container) return;
            
            try {
                // 기존 인스턴스 정리
                if (chartElement.instance) {
                    chartElement.instance.dispose();
                }
                
                // 새 인스턴스 생성
                chartElement.instance = echarts.init(chartElement.container);
                
                // 옵션 설정
                const option = createChartOption(ioType, chartData.pieData[ioType.id]);
                chartElement.instance.setOption(option, true);
                
            } catch (error) {
                console.error(`${ioType.id} 차트 초기화 오류:`, error);
            }
        });
    }
    
    // 차트 크기 리사이즈
    function resizeCharts() {
        IO_TYPES.forEach(ioType => {
            const instance = chartElements[ioType.id]?.instance;
            if (instance) {
                instance.resize();
            }
        });
    }
    
    // 차트 인스턴스 정리
    function disposeCharts() {
        IO_TYPES.forEach(ioType => {
            if (chartElements[ioType.id]?.instance) {
                chartElements[ioType.id].instance?.dispose();
                chartElements[ioType.id].instance = null;
            }
        });
    }
    
    // 윈도우 리사이즈 핸들러
    function handleResize() {
        resizeCharts();
        
        // 크기 변경 후 필요시 차트 재초기화
        if (chartInitTimer) clearTimeout(chartInitTimer);
        chartInitTimer = setTimeout(() => {
            if (isrwd && areContainersReady()) {
                initOrUpdateCharts();
            }
        }, 300);
    }
    
    // 컨테이너가 모두 준비됐는지 확인
    function areContainersReady() {
        return IO_TYPES.every(type => chartElements[type.id]?.container);
    }
    
    // 통계 제목 가져오기
    function getStatTitle(ioType: { id: string; key: (tracetype: any) => "0x28" | "R"; title: (tracetype: any) => string; name: string; colors: string[]; statTitle: string; } | { id: string; key: (tracetype: any) => "0x2a" | "W"; title: (tracetype: any) => string; name: string; colors: string[]; statTitle: string; } | { id: string; key: (tracetype: string) => "0x42" | "D"; title: (tracetype: any) => string; name: (tracetype: any) => string; colors: string[]; statTitle: (tracetype: any) => "UNMAP Statistics" | "Discard Statistics"; }) {
        return typeof ioType.statTitle === 'function' 
            ? ioType.statTitle(tracetype) 
            : ioType.statTitle;
    }
    
    // 컴포넌트 마운트
    onMount(() => {
        window.addEventListener('resize', handleResize);
        
        // 컴포넌트 마운트 후 차트 초기화 (지연 시작)
        if (isrwd) {
            if (chartInitTimer) clearTimeout(chartInitTimer);
            chartInitTimer = setTimeout(() => {
                if (areContainersReady()) {
                    initOrUpdateCharts();
                }
            }, 500);
        }
    });
    
    // 컴포넌트 파괴
    onDestroy(() => {
        window.removeEventListener('resize', handleResize);
        
        if (chartInitTimer) {
            clearTimeout(chartInitTimer);
            chartInitTimer = null;
        }
        
        disposeCharts();
    });
    
    // 데이터 변경 감지
    $effect(() => {
        if (!isrwd || !areContainersReady()) return;
        
        // 데이터 변경 확인
        const currentDataHash = JSON.stringify(data);
        if (currentDataHash !== prevDataHash) {
            prevDataHash = currentDataHash;
            
            // 타이머로 처리해서 Svelte의 렌더링 주기와 분리
            setTimeout(() => {
                initOrUpdateCharts();
            }, 10);
        }
    });
</script>

{#if isrwd}
<div class="flex flex-col gap-6">
    <!-- 요약 통계 -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        {#each IO_TYPES as ioType}
            <div class="stat bg-white shadow rounded-lg p-4">
                <div class="stat-title font-bold text-lg mb-2">{getStatTitle(ioType)}</div>
                <div class="stat-desc grid grid-cols-2 gap-2 text-sm">
                    <div class="stat-item">
                        <span class="font-semibold">Total Request:</span> 
                        <span>{chartData?.stats[ioType.id]?.total.toLocaleString() || 0}</span>
                    </div>
                    <div class="stat-item">
                        <span class="font-semibold">Cont. Ratio:</span>
                        <span>{chartData?.stats[ioType.id]?.ratio || '0.00'}%</span>
                    </div>
                    <div class="stat-item">
                        <span class="font-semibold">Total Size:</span>
                        <span>{chartData?.stats[ioType.id]?.bytes || '0 Bytes'}</span>
                    </div>
                    <div class="stat-item">
                        <span class="font-semibold">Cont. Size Ratio:</span>
                        <span>{chartData?.stats[ioType.id]?.bytesRatio || '0.00'}%</span>
                    </div>
                </div>
            </div>
        {/each}
    </div>
    
    <!-- 파이 차트 영역 -->
    <div bind:this={chartsContainer} class="grid grid-cols-1 md:grid-cols-3 gap-6">
        {#each IO_TYPES as ioType}
            <div class="chart-outer-container bg-white shadow rounded-lg p-4">
                <div 
                    bind:this={chartElements[ioType.id].container} 
                    class="chart-container"
                ></div>
            </div>
        {/each}
    </div>
</div>
{/if}

<style>
    .stat-item {
        display: flex;
        justify-content: space-between;
        padding: 4px 0;
        border-bottom: 1px dashed #e2e8f0;
    }
    
    .chart-outer-container {
        position: relative;
        height: 400px; /* 전체 높이 고정 */
        width: 100%;
        overflow: hidden;
    }
    
    .chart-container {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        width: 100% !important;
        height: 100% !important;
    }
    
    /* 모바일용 차트 높이 조정 */
    @media (max-width: 768px) {
        .chart-outer-container {
            height: 300px;
        }
    }
</style>