<script lang="ts">
    import { ScatterCharts } from '$components/detail';
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    
    interface QDTabsProps {
        traceType: string;
        data: any[];
        legendKey: string;
    }

    let { 
        data = [],
        traceType = '', 
        legendKey = 'cpu', 
    }: QDTabsProps = $props();
    
    // 사용 가능한 탭 정의
    const mainTabs = ['d', 'c'];
    const subTabs = ['cpu', 'addr'];
    
    // 현재 선택된 탭 상태
    let selectedMainTab = $state(mainTabs[0]);
    
    // 각 메인탭별 하위탭 상태
    const tabConfig = {
        d: { defaultTab: 'cpu' },
        c: { defaultTab: 'cpu' }
    };
    
    // ScatterCharts 구성 데이터 정의
    const getChartConfig = (mainTab, subTab) => {
        if (subTab === 'cpu') {
            console.log(`CPU 차트 생성 - mainTab: ${mainTab}, Y축 범위 0-7로 고정`);
            return {
                xAxisKey: 'time',
                yAxisKey: 'cpu',
                legendKey: 'cpu',
                yAxisLabel: 'cpu',
                ycolumn: 'cpu',
                yAxisRange: [0, 7],  // CPU는 항상 0-7로 고정
                actionFilter: mainTab  // 'd' 또는 'c' 액션 필터 추가
            };
        } else if (subTab === 'addr') {
            if (traceType === 'ufs') {
                return {
                    xAxisKey: 'time',
                    yAxisKey: 'lba',
                    legendKey: 'cpu',
                    yAxisLabel: '4KB',
                    ycolumn: 'lba',
                    actionFilter: mainTab  // 'd' 또는 'c' 액션 필터 추가
                };
            } else if (traceType === 'block') {
                return {
                    xAxisKey: 'time',
                    yAxisKey: 'sector',
                    legendKey: 'cpu',
                    yAxisLabel: 'sector',
                    ycolumn: 'sector',
                    actionFilter: mainTab  // 'd' 또는 'c' 액션 필터 추가
                };
            }
        }
        return null;
    };
    
    // 메인 탭 선택 처리
    function selectMainTab(tab) {
        selectedMainTab = tab;
    }
</script>

<!-- 메인 탭 (Daisy UI 스타일로 구현) -->
<div role="tablist" class="tabs tabs-lifted mb-4">
    {#each mainTabs as mainTab, i}
        <input 
            type="radio" 
            name="main_tabs" 
            role="tab" 
            class="tab" 
            aria-label={mainTab.toUpperCase()} 
            checked={mainTab === selectedMainTab}
            on:change={() => selectMainTab(mainTab)}
        />
        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
            <!-- 서브탭 (Shadcn으로 구현) -->
            <Tabs.Root value={tabConfig[mainTab].defaultTab} class="w-full">
                <Tabs.List class="mb-4">
                    {#each subTabs as subTab}
                        <Tabs.Trigger value={subTab}>{subTab === 'cpu' ? 'CPU' : 'ADDRESS'}</Tabs.Trigger>
                    {/each}
                </Tabs.List>
                
                {#each subTabs as subTab}
                    <Tabs.Content value={subTab} class="border-none p-0 pt-2 outline-none">
                        {#if getChartConfig(mainTab, subTab)}
                            {@const config = getChartConfig(mainTab, subTab)}
                            <ScatterCharts 
                                data={data}
                                xAxisKey={config.xAxisKey}
                                yAxisKey={config.yAxisKey}
                                legendKey={config.legendKey}
                                yAxisLabel={config.yAxisLabel}
                                ycolumn={config.ycolumn}
                                yAxisRange={config.yAxisRange}
                                actionFilter={config.actionFilter}
                            />
                        {:else}
                            <div class="p-4 text-center text-gray-500">데이터가 없습니다.</div>
                        {/if}
                    </Tabs.Content>
                {/each}
            </Tabs.Root>
        </div>
    {/each}
</div>

<style>
    /* Daisy UI 탭 스타일 커스터마이징 */
    .tabs-lifted .tab {
        @apply font-medium text-sm;
    }
    
    .tab:checked {
        @apply font-bold text-primary;
    }
    
    .tab-content {
        @apply w-full;
        min-height: 550px; /* 컨텐츠 영역 최소 높이 설정 */
    }
    
    /* Shadcn 탭과 간격 조정 */
    :global(.tab-content [role="tablist"]) {
        @apply px-0;
    }
</style>
