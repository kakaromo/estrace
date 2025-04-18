<script lang="ts">
    // import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    
    import { getTestInfo, getBufferSize } from '$api/db';
    import { trace, 
        filtertrace, prevFilterTrace, filtertraceChanged,
        selectedTrace,  prevselectedTrace, filterselectedTraceChanged, testinfoid
     } from '$stores/trace';

    import type { TestInfo } from '$stores/trace';

    import { Circle2 } from 'svelte-loading-spinners';
    import { StepBack, FileDown, RefreshCw } from 'svelte-lucide';
    import { Button } from "$lib/components/ui/button";

    import { get, set } from 'idb-keyval';  // IndexedDB 사용 위한 import

    import { Separator } from '$lib/components/ui/separator';
    import * as Card from '$lib/components/ui/card/index.js';   
    import * as Dialog from "$lib/components/ui/dialog/index.js";
    import * as Tooltip from "$lib/components/ui/tooltip/index.js";
    import { message } from "@tauri-apps/plugin-dialog";

    import { 
        SelectType,
        SizeStats,
        ScatterCharts, 
        VisualItem, 
        RWDStats,
        LatencyTabs,
        CPUTabs 
    } from '$components/detail';
    
    import { 
        fetchUfsStats, 
        fetchBlockStats, 
        filterTraceData, 
        THRESHOLDS as thresholds 
    } from '$utils/trace-helper';

    // 페이지 ID 및 기본 상태
    // const id = page.params.id;
    const id = $testinfoid;
    let data:TestInfo = $state({});
    let tracedata:any[] = $state([]);
    let filteredData = $state({});
    let tracetype:string[] = $state([]);
    let isLoading:boolean = $state(false);

    // Retry 관련 상태 추가
    let loadError:string = $state('');
    let retryCount:number = $state(0);
    let maxRetries:number = 3;
    let showRetryDialog:boolean = $state(false);
    
    // 시각화 항목 상태
    let ispattern = $state(true);
    let isrwd = $state(false);
    let isqd = $state(false);
    let iscpu = $state(false);
    let islatency = $state(false);
    let issizestats = $state(false);
    
    // UFS 통계 데이터
    let ufsStats = $state({
        dtocStat: null,
        ctodStat: null,
        ctocStat: null,
        sizeCounts: null,
        continuous: null,
    });
    
    // Block 통계 데이터
    let blockStats = $state({
        dtocStat: null,
        ctodStat: null,
        ctocStat: null,
        sizeCounts: null,
        continuous: null,
    });

    // 파일 내보내기 상태
    let isExporting = $state(false);
    let showExportDialog = $state(false);
    let exportResult = $state('');
    let parquetFiles = $state({
        ufs: '',
        block: ''
    });

    let fileNames = $state({
        ufs: '',
        block: ''
    });

    let buffersize = $state(0);

    // 필터가 변경될 때 데이터 업데이트
    $effect(() => {
        (async () => {
            if ($filtertraceChanged) {
                isLoading = true;
                
                // 이전 필터 값 업데이트
                $prevFilterTrace = {...$filtertrace};
                
                // 필터링된 데이터 설정
                await updateFilteredData();
                
                // 선택된 유형에 따라 통계 데이터 다시 로드
                await loadStatsData();
                
                isLoading = false;
            }
        })();
    })

    
    // selectedTrace가 변경될 때 통계 데이터 업데이트
    $effect(() => {
        (async () => {
            if ($selectedTrace && $filterselectedTraceChanged) {
                isLoading = true;
                
                $prevselectedTrace = $selectedTrace;

                // 선택된 trace에 대한 필터링된 데이터 업데이트
                await updateFilteredData();
                
                // 통계 데이터 로드
                await loadStatsData();
                
                isLoading = false;
            }
        })();
    })

    // 필터링된 데이터 설정
    async function updateFilteredData() {
        if ($selectedTrace) {
            const result = await filterTraceData(fileNames[$selectedTrace], tracedata, $selectedTrace, $filtertrace);
            if (result !== null) {
                filteredData[$selectedTrace] = result;  
            }
        }
    }

    // 선택된 유형에 따라 통계 데이터 로드
    async function loadStatsData() {
        if ($selectedTrace === 'ufs') {
            const stats = await fetchUfsStats(fileNames.ufs, $filtertrace);
            ufsStats = stats;
        } else if ($selectedTrace === 'block') {
            const stats = await fetchBlockStats(fileNames.block, $filtertrace);
            blockStats = stats;
        }
    }

    // CSV 내보내기 함수
    async function exportToCSV() {
        const currentType = $selectedTrace;
        if (!currentType || !parquetFiles[currentType]) {
            await message('내보낼 파일이 지정되지 않았습니다.');
            return;
        }
        
        try {
            isExporting = true;
            
            const result = await invoke<string>("export_to_csv", { 
                parquetPath: parquetFiles[currentType], 
                fileType: currentType
            });
            
            exportResult = result;
            showExportDialog = true;
            
        } catch (error) {
            console.error('CSV 내보내기 오류:', error);
            await message(`내보내기 실패: ${error}`);
        } finally {
            isExporting = false;
        }
    }
    
    // parquet 파일 경로 설정
    function setParquetFilePaths() {
        if (data && data.logname) {
            const names = data.logname.split(',');
            
            if (names.length > 0) {
                fileNames.ufs = names[0];
                parquetFiles.ufs = names[0];
            }
            
            if (names.length > 1) {
                fileNames.block = names[1];
                parquetFiles.block = names[1];
            }
        }
    }

    // 트레이스 데이터 로딩 함수 추출
    async function loadTraceData() {
        try {
            isLoading = true;
            loadError = '';
            
            // 테스트 정보 가져오기
            data = await getTestInfo(id);
            buffersize = await getBufferSize();
            console.log('buffersize:', buffersize);
            
            // 캐시 키 구성
            const cacheKey = `traceData_${id}_${data.logfolder}_${data.logname}`;
            
            // IndexedDB에서 캐시된 데이터 불러오기
            let cached = await get(cacheKey);
            if (cached) {
                tracedata = JSON.parse(cached);
            } else {
                // 캐시된 데이터가 없으면 서버에서 가져오기
                let traceStr = await invoke<string>('readtrace', { 
                    logfolder: data.logfolder, 
                    logname: data.logname,
                    maxrecords: buffersize 
                });
                
                tracedata = JSON.parse(traceStr);
                
                // IndexedDB에 데이터 저장
                await set(cacheKey, traceStr);
            }
            
            // 데이터 저장 및 초기화
            $trace = tracedata;
            filteredData = tracedata;
            tracetype = Object.keys(tracedata);

            // 파일 경로 설정
            setParquetFilePaths();

            // 초기 통계 데이터 로드
            await loadStatsData();
            
            retryCount = 0; // 성공했으므로, 재시도 카운트 초기화
            return true;
        } catch (error) {
            let errorMessage = '데이터 로딩 실패';
            if (error instanceof Error) {
                errorMessage = `Error: ${error.message}`;
                console.error('Error during data loading:', error.message);
                console.error('Stack trace:', error.stack);
            } else {
                console.error('Unknown error:', error);
                errorMessage = `Unknown error: ${error}`;
            }
            
            loadError = errorMessage;
            retryCount++;
            
            if (retryCount >= maxRetries) {
                showRetryDialog = true;
            } else {
                // 자동 재시도
                console.log(`자동 재시도 중... (${retryCount}/${maxRetries})`);
                await new Promise(resolve => setTimeout(resolve, 1000));
                return loadTraceData();
            }
            
            return false;
        } finally {
            isLoading = false;
        }
    }
    
    // 수동 재시도 함수
    async function retryLoading() {
        showRetryDialog = false;
        retryCount = 0; // 수동 재시도시 카운트 초기화
        const success = await loadTraceData();
        if (!success && retryCount >= maxRetries) {
            // 최대 재시도 횟수 초과하면 홈으로 이동
            goto('/');
        }
    }

    onMount(async () => {
        try {
            isLoading = true;
            const startTotal = performance.now();
            
            // 테스트 정보 가져오기
            data = await getTestInfo(id);
            buffersize = await getBufferSize();
            console.log('buffersize:', buffersize);
            // 캐시 키 구성
            const cacheKey = `traceData_${id}_${data.logfolder}_${data.logname}`;
            
            // IndexedDB에서 캐시된 데이터 불러오기
            let cached = await get(cacheKey);
            if (cached) {
                tracedata = JSON.parse(cached);
            } else {
                // 캐시된 데이터가 없으면 서버에서 가져오기
                let traceStr = await invoke<string>('readtrace', { 
                    logfolder: data.logfolder, 
                    logname: data.logname,
                    maxrecords: buffersize 
                });
                
                tracedata = JSON.parse(traceStr);
                
                // IndexedDB에 데이터 저장
                await set(cacheKey, traceStr);
            }
            
            // 데이터 저장 및 초기화
            $trace = tracedata;
            filteredData = tracedata;
            // filteredData['ufs'] = tracedata['ufs']['data'];
            // filteredData['block'] = tracedata['block']['data'];
            tracetype = Object.keys(tracedata);

            // 파일 경로 설정
            setParquetFilePaths();

            // 초기 필터링된 데이터 설정
            // await updateFilteredData();
            
            // 초기 통계 데이터 로드
            await loadStatsData();

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
        <div class="fixed top-4 left-4 flex items-center gap-2">
            <SelectType tracetype={tracetype} tracedata={filteredData} class="h-12"/>
            
            <!-- Retry 버튼 추가 -->
            <Tooltip.Root>
                <Tooltip.Trigger asChild>
                    <Button 
                        variant="outline" 
                        size="icon"
                        class="h-12 w-12"
                        onclick={retryLoading}
                    >
                        <RefreshCw size="20"></RefreshCw>
                    </Button>
                </Tooltip.Trigger>
                <Tooltip.Content>
                    <p>데이터 다시 불러오기</p>
                </Tooltip.Content>
            </Tooltip.Root>

            <Tooltip.Root>
                <Tooltip.Trigger asChild>
                    <Button 
                        variant="outline" 
                        size="icon"
                        class="h-12 w-12"
                        onclick={exportToCSV}
                        disabled={isExporting || !$selectedTrace || !parquetFiles[$selectedTrace]}
                    >
                        {#if isExporting}
                            <div class="animate-spin h-5 w-5 border-2 border-current border-t-transparent rounded-full"></div>
                        {:else}
                            <FileDown size="20"></FileDown>
                        {/if}
                    </Button>
                </Tooltip.Trigger>
                <Tooltip.Content>
                    <p>현재 데이터를 CSV로 내보내기</p>
                </Tooltip.Content>
            </Tooltip.Root>
            {#if $selectedTrace !== '' && tracedata[$selectedTrace].total_count !== tracedata[$selectedTrace].sampled_count}
            <div class="text-sm font-medium mb-2">total : {tracedata[$selectedTrace].total_count}</div>
            <div class="text-sm font-medium mb-2">sampling : {tracedata[$selectedTrace].sampled_count}</div>
            <div class="text-sm font-medium mb-2">sample ratio : {Number(tracedata[$selectedTrace].sampling_ratio.toFixed(2))} %</div>
            {/if}
            <div class="text-sm font-medium mb-2">{data.title}</div>
        </div>
        {/if}        
    </header>    
    <main class="mx-auto p-6">
        {#if $selectedTrace != ''}
        <VisualItem bind:ispattern bind:isrwd bind:isqd bind:iscpu bind:islatency bind:issizestats />                 
        <div class="grid grid-cols-2 gap-4">
            <div class="col-span-2">
                <Card.Root class={ispattern ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Pattern</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <ScatterCharts data={filteredData.ufs.data} xAxisKey='time' yAxisKey='lba' legendKey='opcode' yAxisLabel='4KB' ycolumn='lba'/>
                        {:else if $selectedTrace === 'block'}
                        <ScatterCharts data={filteredData.block.data} xAxisKey='time' yAxisKey='sector' legendKey='io_type' yAxisLabel='sector' ycolumn='sector'/>
                        {/if}
                    </Card.Content>
                </Card.Root>
                <Separator class="my-4 {isqd ? 'block' : 'hidden'}" />
                <Card.Root class={isqd ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} QueueDepth</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <ScatterCharts data={filteredData.ufs.data} xAxisKey='time' yAxisKey='qd' legendKey='opcode' yAxisLabel='count' ycolumn='qd'/>
                        {:else if $selectedTrace === 'block'}
                        <ScatterCharts data={filteredData.block.data} xAxisKey='time' yAxisKey='qd' legendKey='io_type' yAxisLabel='count' ycolumn='qd'/>
                        {/if}
                    </Card.Content>
                </Card.Root>
                <Separator class="my-4 {iscpu ? 'block' : 'hidden'}" />
                <Card.Root class={iscpu ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} CPU</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <CPUTabs traceType={$selectedTrace} data={filteredData.ufs.data} legendKey='cpu' />
                        {:else if $selectedTrace === 'block'}
                        <CPUTabs traceType={$selectedTrace} data={filteredData.block.data} legendKey='cpu' />
                        {/if}                        
                    </Card.Content>
                </Card.Root>
                <Separator class="my-4 {isrwd ? 'block' : 'hidden'}" />
                <Card.Root class={isrwd ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Read/Write/Discard Statistics</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <RWDStats data={ufsStats.continuous} tracetype={$selectedTrace} {isrwd} />
                        {:else if $selectedTrace === 'block'}
                        <RWDStats data={blockStats.continuous} tracetype={$selectedTrace} {isrwd} />
                        {/if}
                    </Card.Content>
                </Card.Root>                
                <Separator class="my-4 {islatency ? 'block' : 'hidden'}" />
                <Card.Root class={islatency ? 'block' : 'hidden'}>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Latency</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <LatencyTabs 
                            traceType={$selectedTrace} 
                            filteredData={filteredData.ufs.data}
                            legendKey="opcode"
                            thresholds={thresholds}
                            dtocStat={ufsStats.dtocStat}
                            ctodStat={ufsStats.ctodStat}
                            ctocStat={ufsStats.ctocStat}
                        />
                        {:else if $selectedTrace === 'block'}         
                        <LatencyTabs 
                            traceType={$selectedTrace} 
                            filteredData={filteredData.block.data}
                            legendKey="io_type"
                            thresholds={thresholds}
                            dtocStat={blockStats.dtocStat}
                            ctodStat={blockStats.ctodStat}
                            ctocStat={blockStats.ctocStat}
                        />
                        {/if}
                    </Card.Content>
                </Card.Root>                                
            </div>
            <div class="col-span-2 {issizestats ? 'block' : 'hidden'}">
                <Separator class="my-4" />          
                <Card.Root>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Size</Card.Title>
                        <Card.Description>Size별 Count</Card.Description>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs' && ufsStats.sizeCounts?.opcode_stats} 
                        <SizeStats opcode_size_counts={ufsStats.sizeCounts.opcode_stats} />
                        {:else if $selectedTrace === 'block' && blockStats.sizeCounts?.opcode_stats}
                        <SizeStats opcode_size_counts={blockStats.sizeCounts.opcode_stats} />
                        {/if}
                    </Card.Content>
                </Card.Root> 
            </div>
        </div> 
        {/if} 
    </main>
</div>

<Dialog.Root bind:open={showExportDialog}>
    <Dialog.Content>
        <Dialog.Header>
            <Dialog.Title>내보내기 결과</Dialog.Title>
            <Dialog.Description>
                CSV 파일이 생성되었습니다.
            </Dialog.Description>
        </Dialog.Header>
        <div class="p-4 bg-slate-100 rounded">
            <p class="text-sm break-all">{exportResult}</p>
        </div>
        <Dialog.Footer>
            <Button onclick={() => showExportDialog = false}>확인</Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>

<style>
    .spinner-overlay {
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
    }
</style>