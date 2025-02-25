<script lang="ts">
    import { page } from '$app/stores';
    import { onMount, onDestroy } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    
    import { getTestInfo } from '$api/db';
    import { trace, selectedTrace, filtertrace, prevFilterTrace, filtertraceChanged } from '$stores/trace';

    import { Circle2 } from 'svelte-loading-spinners';
    import { StepBack } from 'svelte-lucide';
    import { Button } from "$lib/components/ui/button";

    import pick  from 'lodash/pick';
    import { get, set } from 'idb-keyval';  // IndexedDB 사용 위한 import

    import { ContextMenu } from '$lib/components/ui/context-menu';
    import * as Select from "$lib/components/ui/select/index.js";
    import { Separator } from '$lib/components/ui/separator';
    import * as Card from '$lib/components/ui/card/index.js';    

    import { SelectType,LatencyStats, SizeStats, ScatterCharts } from '$components/detail';

    let id: number;
    let data = $state({});
    let tracedata = [];
    let filteredData = $state([]);
    let tracetype = $state([]);
    
    let isLoading = $state(false);
    let lbachartdata = $state([]);
    // 개별 변수로 상태 선언
    let ufsdtocstat = $state(null);
    let ufsctocstat = $state(null);
    let ufsctodstat = $state(null);
    
    // 개별 변수로 분리
    let ufssizecounts = $state(null);
    let ufstotal_counts = $state(null);
    let ufsopcode_stats = $state(null);

    // 개별 변수로 상태 선언
    let blockdtocstat = $state(null);
    let blockctocstat = $state(null);
    let blockctodstat = $state(null);
    
    // 개별 변수로 분리
    let blocksizecounts = $state(null);
    let blocktotal_counts = $state(null);
    let blockopcode_stats = $state(null);

    let thresholds = ['0.1ms', '0.5ms', '1ms', '5ms', '10ms', '50ms', '100ms', '500ms', '1s', '5s', '10s', '50s', '100s', '500s', '1000s'];


    // $page.params를 통해 동적 파라미터 id 값을 가져옵니다.
    id = $page.params.id;

    let fname = '';
    let unsubscribe;

    // currentValue가 변경될 때마다 호출되는 함수
    $effect(async () => {
        if ($filtertraceChanged) {
            console.log('filtertrace changed from:', $prevFilterTrace);
            console.log('to:', $filtertrace);
            
            // 현재 값을 이전 값으로 업데이트
            $prevFilterTrace = {...$filtertrace};
            await ufslatencystats(fname);
            await blocklatencystats(fname);
            setFilterData();
        }
    });

    function setFilterData() {
        if ($filtertrace.from_time === 0 && $filtertrace.to_time === 0) {
            filteredData[$selectedTrace] = tracedata[$selectedTrace];
            console.log('filteredData:', filteredData[$selectedTrace].length);  
            console.log('tracedata:', tracedata[$selectedTrace].length);
        } 
        else {
            console.log('filtertrace:', $filtertrace);
            const filter = tracedata[$selectedTrace].filter((item) => {
                return item.time >= $filtertrace.from_time && item.time <= $filtertrace.to_time && 
                    item[$filtertrace.zoom_column] >= $filtertrace.from_lba && item[$filtertrace.zoom_column] <= $filtertrace.to_lba;
            });
            console.log('filter length:', filter.length);
            filteredData[$selectedTrace] = filter;
        }
    }

    onMount(async () => {
        try {
            isLoading = true;
            const startTotal = performance.now();
            
            const startTestInfo = performance.now();
            data = await getTestInfo(id);
            const endTestInfo = performance.now();
            console.log("getTestInfo time:", endTestInfo - startTestInfo, "ms");

            const startInvoke = performance.now();
            // 캐시 키 구성: id와 logfolder, logname을 이용
            const cacheKey = `traceData_${id}_${data.logfolder}_${data.logname}`;
            fname = data.logname.split(',')[0];
            
            // IndexedDB에서 cached 데이터를 불러오기
            let cached = await get(cacheKey);
            if (cached) {
                tracedata = JSON.parse(cached);
                console.log('Loaded trace data from sessionStorage.');
            } else {
                const startInvoke = performance.now();
                let traceStr = await invoke<string>('readtrace', { logfolder: data.logfolder, logname: data.logname });
                const endInvoke = performance.now();
                console.log("invoke('readtrace') time:", endInvoke - startInvoke, "ms");
                
                const startParse = performance.now();
                tracedata = JSON.parse(traceStr);
                const endParse = performance.now();
                console.log("JSON.parse time:", endParse - startParse, "ms");
                
                // IndexedDB에 trace 데이터를 저장 (큰 용량을 저장 가능합니다)
                await set(cacheKey, traceStr);
                console.log('Saved trace data to IndexedDB.');
            }
            const endInvoke = performance.now();
            console.log("invoke('readtrace') time:", endInvoke - startInvoke, "ms");
            // 전역 trace store에 데이터 저장
            $trace = tracedata;
            filteredData = tracedata;
            
            tracetype = Object.keys(tracedata);
            console.log('tracetype:', tracetype);

            const startLbaChart = performance.now();
            // lbachartdata = await invoke('chart', { logfolder: data.logfolder, logname: data.logname, column: 'lba', time_from: 0, time_to: 0, col_from: 0, col_to: 0 });
            lbachartdata = pick(tracedata, ['time', 'lba']);
            const endLbaChart = performance.now();
            console.log("lbachartdata time:", endLbaChart - startLbaChart, "ms");
            
            console.log('tracedata:', Object.keys(tracedata));
                        
            await ufslatencystats(fname);
            await blocklatencystats(fname);
            
            console.log("Total onMount time:", performance.now() - startTotal, "ms");
            isLoading = false;
        } catch (error) {
            if (error instanceof Error) {
                console.error('Error during onMount:', error.message);
                console.error('Stack trace:', error.stack);
            } else {
                console.error('Unknown error:', error);
            }
            goto('/');
        }
    });

    async function ufslatencystats(ufsfname: string) {
        if(tracedata && tracedata.ufs.length > 0) {
            console.log("data", data);
            console.log("ufsfname", ufsfname);
            const startdtocstat = performance.now();        
            const ufsdtocstatResult:string = await invoke('ufs_latencystats', { logname: ufsfname, column: 'dtoc', thresholds:thresholds, 
                timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            ufsdtocstat = JSON.parse(ufsdtocstatResult);                    
            const enddtocstat = performance.now();        
            console.log("statusdata time:", enddtocstat - startdtocstat, "ms");

            const startctodstat = performance.now();
            const ufsctodstatResult:string = await invoke('ufs_latencystats', { logname: ufsfname, column: 'ctod', thresholds:thresholds, 
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            ufsctodstat = JSON.parse(ufsctodstatResult);                                 
            const endctodstat = performance.now();
            console.log("statusdata time:", endctodstat - startctodstat, "ms");

            const startctocstat = performance.now();        
            const ufsctocstatResult:string = await invoke('ufs_latencystats', { logname: ufsfname, column: 'ctoc', thresholds:thresholds, 
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            ufsctocstat = JSON.parse(ufsctocstatResult);
            const endctocstat = performance.now();
            console.log("statusdata time:", endctocstat - startctocstat, "ms");

            const startSizeCounts = performance.now();
            const ufssizecountsResult = await invoke('ufs_sizestats', { logname: ufsfname, column: 'dtoc', 
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            ufssizecounts = JSON.parse(ufssizecountsResult);                 
            ufstotal_counts = ufssizecounts.total_counts;
            ufsopcode_stats = ufssizecounts.opcode_stats;
            const endSizeCounts = performance.now();
            console.log("sizestats time:", endSizeCounts - startSizeCounts, "ms");
        } 
    }

    async function blocklatencystats(blockfname: string) {
        if (tracedata && tracedata.block.length > 0) {
            let blockfname = data.logname.split(',')[1];
            const startdtocstat = performance.now();
            blockdtocstat = await invoke('block_latencystats', { logname: blockfname, column: 'dtoc', thresholds:thresholds, group: true,
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            blockdtocstat = JSON.parse(blockdtocstat);
            const enddtocstat = performance.now();
            console.log("statusdata time:", enddtocstat - startdtocstat, "ms");
            const startctodstat = performance.now();
            blockctodstat = await invoke('block_latencystats', { logname: blockfname, column: 'ctod', thresholds:thresholds, group: true,
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            blockctodstat = JSON.parse(blockctodstat);
            const endctodstat = performance.now();
            console.log("statusdata time:", endctodstat - startctodstat, "ms");
            const startctocstat = performance.now();
            blockctocstat = await invoke('block_latencystats', { logname: blockfname, column: 'ctoc', thresholds:thresholds, group: true, 
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            blockctocstat = JSON.parse(blockctocstat);
            const endctocstat = performance.now();
            console.log("statusdata time:", endctocstat - startctocstat, "ms");
            const startSizeCounts = performance.now();                
            blocksizecounts = await invoke('block_sizestats', { logname: blockfname, column: 'dtoc', group: true, 
            timeFrom: $filtertrace.from_time, timeTo: $filtertrace.to_time, colFrom: $filtertrace.from_lba, colTo: $filtertrace.to_lba, zoomColumn: $filtertrace.zoom_column });                     
            blocksizecounts = JSON.parse(blocksizecounts);
            blocktotal_counts = blocksizecounts.total_counts;
            blockopcode_stats = blocksizecounts.opcode_stats;
            const endSizeCounts = performance.now();
            console.log("sizestats time:", endSizeCounts - startSizeCounts, "ms");
        }
    }
</script>

    {#if isLoading}
        <div class="spinner-overlay">
            <Circle2 color="#FF3E00" size="60" unit="px" />
        </div>
    {/if}
<div class="font-sans">
    <header class="py-4 px-6">
        <Button href="/" variant="primary"  class="fixed top-4 right-4 h-12">
            <StepBack size="24" />
            Back
        </Button>
        {#if tracetype.length > 0}
        <SelectType bind:tracetype bind:tracedata class="fixed top-4 left-4 h-12"/>
        <p>선택된 타입: {$selectedTrace}</p>
        {/if}
        <Separator class="my-4" />
    </header>    
    <main class="mx-auto p-6">
        {#if $selectedTrace != ''}                     
        <div class="grid grid-cols-2 gap-4">
            <div class="col-span-2">
                <h3 class="text-lg font-medium bg-blue-50">LBA Pattern</h3>  
                <div class="divider"></div>   
                <Card.Root>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Pattern</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <ScatterCharts data={filteredData.ufs} xAxisKey='time' yAxisKey='lba' legendKey='opcode' yAxisLabel='4KB' ycolumn='lba'/>
                        {:else if $selectedTrace === 'block'}
                        <ScatterCharts data={filteredData.block} xAxisKey='time' yAxisKey='sector' legendKey='io_type' yAxisLabel='sector' ycolumn='sector'/>
                        {/if}
                    </Card.Content>
                </Card.Root>
                <Separator class="my-4" />
                <!-- <Card.Header>
                    <Card.Title>{$selectedTrace.toUpperCase()} Latency</Card.Title>
                </Card.Header>
                <Card.Content>
                    {#if $selectedTrace === 'ufs'} 
                    <div role="tablist" class="tabs tabs-lifted">
                        <input type="radio" name="ufslatencychart" role="tab" class="tab" aria-label="DtoC" checked="checked"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.ufs} xAxisKey='time' yAxisKey='dtoc' legendKey='opcode' yAxisLabel='ms' ycolumn='dtoc'/> 
                        </div>
                        <input type="radio" name="ufslatencychart" role="tab" class="tab" aria-label="CtoD"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.ufs} xAxisKey='time' yAxisKey='ctod' legendKey='opcode' yAxisLabel='ms' ycolumn='ctod'/> 
                        </div>
                        <input type="radio" name="ufslatencychart" role="tab" class="tab" aria-label="CtoC"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.ufs} xAxisKey='time' yAxisKey='ctoc' legendKey='opcode' yAxisLabel='ms' ycolumn='ctoc'/> 
                        </div>
                    </div>
                             
                    {:else if $selectedTrace === 'block'}
                    <div role="tablist" class="tabs tabs-lifted">
                        <input type="radio" name="blocklatencychart" role="tab" class="tab" aria-label="DtoC" checked="checked"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.block} xAxisKey='time' yAxisKey='dtoc' legendKey='io_type' yAxisLabel='ms' ycolumn='dtoc'/> 
                        </div>
                        <input type="radio" name="blocklatencychart" role="tab" class="tab" aria-label="CtoD"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.block} xAxisKey='time' yAxisKey='ctod' legendKey='io_type' yAxisLabel='ms' ycolumn='ctod'/> 
                        </div>
                        <input type="radio" name="blocklatencychart" role="tab" class="tab" aria-label="CtoC"/>
                        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                            <ScatterCharts data={filteredData.block} xAxisKey='time' yAxisKey='ctoc' legendKey='io_type' yAxisLabel='ms' ycolumn='ctoc'/> 
                        </div>
                    </div>
                    
                    {/if}
                </Card.Content> -->
                <Separator class="my-4" />
                <Card.Root>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Latency</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <div role="tablist" class="tabs tabs-lifted">
                            <input type="radio" name="ufslatency" role="tab" class="tab" aria-label="DtoC" checked="checked"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={ufsdtocstat} /> 
                            </div>
                            <input type="radio" name="ufslatency" role="tab" class="tab" aria-label="CtoD"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={ufsctodstat} /> 
                            </div>
                            <input type="radio" name="ufslatency" role="tab" class="tab" aria-label="CtoC"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={ufsctocstat} /> 
                            </div>
                        </div>
                                 
                        {:else if $selectedTrace === 'block'}
                        <div role="tablist" class="tabs tabs-lifted">
                            <input type="radio" name="blocklatency" role="tab" class="tab" aria-label="DTOC" checked="checked"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={blockdtocstat} />
                            </div>
                            <input type="radio" name="blocklatency" role="tab" class="tab" aria-label="CtoD"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={blockctodstat} /> 
                            </div>
                            <input type="radio" name="blocklatency" role="tab" class="tab" aria-label="CtoC"/>
                            <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
                                <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={blockctocstat} /> 
                            </div>
                        </div>
                        
                        {/if}
                    </Card.Content>
                </Card.Root>       
            </div>
            <div class="col-span-2">
            <div class="divider"></div>            
            <Card.Root>
                <Card.Header>
                    <Card.Title>{$selectedTrace.toUpperCase()} Size</Card.Title>
                    <Card.Description>Sice별 Count</Card.Description>
                </Card.Header>
                <Card.Content>
                    {#if $selectedTrace === 'ufs' && ufssizecounts?.opcode_stats} 
                    <SizeStats opcode_size_counts={ufssizecounts.opcode_stats} />
                    {:else if $selectedTrace === 'block' &&  blocksizecounts?.opcode_stats}
                    <SizeStats opcode_size_counts={blocksizecounts.opcode_stats} />
                    {/if}
                </Card.Content>
            </Card.Root> 
            </div>
            
            

        </div> 
        {/if} 
    </main>
    

    
</div>

<style>
    .spinner-overlay {
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
    }
</style>