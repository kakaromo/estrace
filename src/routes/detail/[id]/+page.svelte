<script lang="ts">
    import { page } from '$app/stores';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    
    import { getTestInfo } from '../../../api/db';
    import { trace, selectedTrace } from '../../../stores/trace';

    import { Circle2 } from 'svelte-loading-spinners';
    import { StepBack } from 'svelte-lucide';
    import { Button } from "$lib/components/ui/button";

    import pick  from 'lodash/pick';
    import { get, set } from 'idb-keyval';  // IndexedDB 사용 위한 import

    import { ContextMenu } from '$lib/components/ui/context-menu';
    import * as Select from "$lib/components/ui/select/index.js";
    import { Separator } from '$lib/components/ui/separator';
    import * as Card from '$lib/components/ui/card/index.js';

    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";

    import { SelectType } from '$components/detail';
    import { LatencyStats, SizeStats } from '$components/table';

    let id: number;
    let data = $state({});
    let tracedata = $state([]);
    let tracetype = $state([]);
    
    let isLoading = $state(false);
    let lbachartdata = $state([]);
    let { ufsdtocstat, ufsctocstat, ufsctodstat } = $state([]);
    let { ufssizecounts, ufstotal_counts, ufsopcode_stats } = $state({});

    let { blockdtocstat, blockctocstat, blockctodstat } = $state([]);
    let { blocksizecounts, blocktotal_counts, blockopcode_stats } = $state({});

    let thresholds = ['0.1ms', '0.5ms', '1ms', '5ms', '10ms', '50ms', '100ms', '500ms', '1s', '5s', '10s', '50s', '100s', '500s', '1000s'];


    // $page.params를 통해 동적 파라미터 id 값을 가져옵니다.
    id = $page.params.id;

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
            
            tracetype = Object.keys(tracedata);
            console.log('tracetype:', tracetype);

            const startLbaChart = performance.now();
            // lbachartdata = await invoke('chart', { logfolder: data.logfolder, logname: data.logname, column: 'lba', time_from: 0, time_to: 0, col_from: 0, col_to: 0 });
            lbachartdata = pick(tracedata, ['time', 'lba']);
            const endLbaChart = performance.now();
            console.log("lbachartdata time:", endLbaChart - startLbaChart, "ms");
            
            console.log('tracedata:', Object.keys(tracedata));
            if(tracedata && tracedata.ufs.length > 0) {
                let ufsfname = data.logname.split(',')[0];
                console.log("data", data);
                console.log("ufsfname", ufsfname);
                const startdtocstat = performance.now();        
                ufsdtocstat = await invoke('latencystats', { logname: ufsfname, column: 'dtoc', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0 });        
                ufsdtocstat = JSON.parse(ufsdtocstat);
                const enddtocstat = performance.now();        
                console.log("statusdata time:", enddtocstat - startdtocstat, "ms");

                const startctodstat = performance.now();
                ufsctodstat = await invoke('latencystats', { logname: ufsfname, column: 'ctod', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0 });
                ufsctodstat = JSON.parse(ufsctodstat);
                const endctodstat = performance.now();
                console.log("statusdata time:", endctodstat - startctodstat, "ms");

                const startctocstat = performance.now();        
                ufsctocstat = await invoke('latencystats', { logname: ufsfname, column: 'ctoc', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0 });
                ufsctocstat = JSON.parse(ufsctocstat);
                const endctocstat = performance.now();
                console.log("statusdata time:", endctocstat - startctocstat, "ms");

                const startSizeCounts = performance.now();
                ufssizecounts = await invoke('sizestats', { logname: ufsfname, column: 'dtoc', time_from: 0, time_to: 0, col_from: 0, col_to: 0 });
                ufssizecounts = JSON.parse(ufssizecounts);
                ufstotal_counts = ufssizecounts.total_counts;
                ufsopcode_stats = ufssizecounts.opcode_stats;
                const endSizeCounts = performance.now();
                console.log("sizestats time:", endSizeCounts - startSizeCounts, "ms");
            } 
            if (tracedata && tracedata.block.length > 0) {
                let blockfname = data.logname.split(',')[1];
                const startdtocstat = performance.now();
                blockdtocstat = await invoke('block_latencystats', { logname: blockfname, column: 'dtoc', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0, group:true });
                blockdtocstat = JSON.parse(blockdtocstat);
                const enddtocstat = performance.now();
                console.log("statusdata time:", enddtocstat - startdtocstat, "ms");
                const startctodstat = performance.now();
                blockctodstat = await invoke('block_latencystats', { logname: blockfname, column: 'ctod', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0, group:true });
                blockctodstat = JSON.parse(blockctodstat);
                const endctodstat = performance.now();
                console.log("statusdata time:", endctodstat - startctodstat, "ms");
                const startctocstat = performance.now();
                blockctocstat = await invoke('block_latencystats', { logname: blockfname, column: 'ctoc', thresholds:thresholds,  time_from: 0, time_to: 0, col_from: 0, col_to: 0, group:true });
                blockctocstat = JSON.parse(blockctocstat);
                const endctocstat = performance.now();
                console.log("statusdata time:", endctocstat - startctocstat, "ms");
                const startSizeCounts = performance.now();                
                blocksizecounts = await invoke('block_sizestats', { logname: blockfname, column: 'dtoc', time_from: 0, time_to: 0, col_from: 0, col_to: 0, group:true });
                blocksizecounts = JSON.parse(blocksizecounts);
                blocktotal_counts = blocksizecounts.total_counts;
                blockopcode_stats = blocksizecounts.opcode_stats;
                const endSizeCounts = performance.now();
                console.log("sizestats time:", endSizeCounts - startSizeCounts, "ms");
            }
            
            
            console.log("Total onMount time:", performance.now() - startTotal, "ms");
            console.log('ufsdtocstat:', ufsdtocstat);
            console.log('ufsctodstat:', ufsctodstat);
            console.log('ufsctodstat:', ufsctodstat);            
            console.log('ufssizecounts:', ufssizecounts);
            // console.log('tracedata:', tracedata);

            console.log('blockdtocstat:', blockdtocstat);
            console.log('blockctodstat:', blockctocstat);
            console.log('blockctodstat:', blockctodstat);
            console.log('blocksizecounts:', blocksizecounts);
            isLoading = false;
        } catch (error) {
            console.error('Error:', error);
            goto('/');
        }
        
    });
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
        <div class="grid grid-cols-2 gap-4">
            <div class="col-span-2">
                <h3 class="text-lg font-medium bg-blue-50">LBA Pattern</h3>  
                <div class="divider"></div>  
                {#if $selectedTrace === 'ufs'}                     
                <Card.Root>
                    <Card.Header>
                        <Card.Title>UFS Latency</Card.Title>
                        <Card.Description>Range별 Latency Count & Stats</Card.Description>
                    </Card.Header>
                    <Card.Content>
                        <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={ufsdtocstat} />
                    </Card.Content>
                </Card.Root>   
                <div class="divider"></div>  
                <Card.Root>
                    <Card.Header>
                        <Card.Title>UFS Size</Card.Title>
                        <Card.Description>Sice별 Count</Card.Description>
                    </Card.Header>
                    <Card.Content>
                        <SizeStats opcode_size_counts={ufssizecounts.opcode_stats} />
                    </Card.Content>
                </Card.Root>       
                {:else if $selectedTrace === 'block'}                
                <Card.Root>
                    <Card.Header>
                        <Card.Title>Block Latency</Card.Title>
                        <Card.Description>Range별 Latency Count & Stats</Card.Description>
                    </Card.Header>
                    <Card.Content>                        
                        <LatencyStats tracetype={$selectedTrace} threshold={thresholds} latencystat={blockdtocstat} />
                    </Card.Content>
                </Card.Root>      
                <div class="divider"></div>  
                <Card.Root>
                    <Card.Header>
                        <Card.Title>UFS Size</Card.Title>
                        <Card.Description>Sice별 Count</Card.Description>
                    </Card.Header>
                    <Card.Content>
                        <SizeStats opcode_size_counts={blocksizecounts.opcode_stats} />
                    </Card.Content>
                </Card.Root> 
                {/if}           
            </div>

        </div>  
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