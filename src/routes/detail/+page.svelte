<script lang="ts">
    // import { page } from '$app/state';
    import { onMount, tick } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    import { tableFromIPC } from 'apache-arrow';
    
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
        ScatterChartsDeck, 
        VisualItem, 
        RWDStats,
        LatencyTabs,
        CPUTabs 
    } from '$components/detail';
    
    import { 
        fetchUfsStats, 
        fetchBlockStats, 
        filterTraceData, 
        THRESHOLDS as thresholds,
        fetchTraceLengths
    } from '$utils/trace-helper';
    
    import { arrowToWebGLData } from '$utils/webgl-optimizer';
    
    // 페이지 ID 및 기본 상태
    // const id = page.params.id;
    const id = $testinfoid;
    let data:TestInfo = $state({});
    let tracedata:any[] = $state([]);
    let filteredData = $state({});
    let tracetype:string[] = $state([]);
    let traceLengths:any = $state({});

    // 선택된 타입의 필터된 데이터를 접근하기 위한 반응형 변수
    // ⚡ 성능 최적화: Arrow Table 직접 사용 (.data 제거)
    let currentFilteredTable = $derived(filteredData[$selectedTrace]?.table ?? null);
    let currentFiltered:Array = $derived(filteredData[$selectedTrace]?.data ?? []); // 호환성용 (CPUTabs, RWDStats 등)
    let legendKey:string = $derived($selectedTrace === 'ufs' ? 'opcode' : 'io_type');
    let patternAxis:Object = $derived($selectedTrace === 'ufs'
        ? { key: 'lba', label: '4KB', column: 'lba' }
        : { key: 'sector', label: 'sector', column: 'sector' });
    let currentStats:Object = $derived($selectedTrace === 'ufs' ? ufsStats : blockStats);
    let isLoading:boolean = $state(false);

    // Retry 관련 상태 추가
    let loadError:string = $state('');
    let retryCount:number = $state(0);
    let maxRetries:number = 3;
    let showRetryDialog:boolean = $state(false);
    // 차트 리렌더링을 위한 키 추가
    let chartKey:number = $state(0);
    
    // 시각화 항목 상태
    let ispattern = $state(true);
    let isrwd = $state(false);
    let isqd = $state(false);
    let iscpu = $state(false);
    let islatency = $state(false);
    let issizestats = $state(false);
    
    // 각 차트별 로딩 상태
    let loadingStates = $state({
        pattern: false,
        rwd: false,
        qd: false,
        cpu: false,
        latency: false,
        sizestats: false
    });
    
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
            console.log('[Trace] 필터 변경 감지');
            // 이전 필터 값 업데이트
            $prevFilterTrace = {...$filtertrace};
            
            try {
                if (!tracedata[$selectedTrace]) {
                    await loadTraceData();
                }
                
                // 필터링된 데이터 설정
                await updateFilteredData();
                
                // 선택된 유형에 따라 통계 데이터 다시 로드
                await loadStatsData();
                
                // 추가 지연으로 모든 차트 렌더링 완료 보장
                await delay(300);
            } catch (error) {
                console.error('[Trace] 데이터 처리 오류:', error);
            } finally {
                console.log('[Trace] 모든 처리 완료, 로딩 상태 해제');
                isLoading = false;
            }
        }
        })();
    });

    
    // selectedTrace가 변경될 때 통계 데이터 업데이트
    $effect(() => {
        // selectedTrace가 변경될 때만 filtertrace 초기화
        if ($selectedTrace) {
            $filtertrace = {
                zoom_column: $selectedTrace === 'ufs' ? 'lba' : 'sector',
                from_time: 0.0,
                to_time: 0.0,
                from_lba: 0.0,
                to_lba: 0.0,
            };
        }
    })
    
    // RWD 차트 enable 시 통계 데이터 로드
    $effect(() => {
        (async () => {
            if (isrwd && !loadingStates.rwd && !currentStats.dtocStat) {
                loadingStates.rwd = true;
                try {
                    await loadStatsData();
                } finally {
                    loadingStates.rwd = false;
                }
            }
        })();
    });
    
    // Size Stats enable 시 통계 데이터 로드
    $effect(() => {
        (async () => {
            if (issizestats && !loadingStates.sizestats && !currentStats.sizeCounts) {
                loadingStates.sizestats = true;
                try {
                    await loadStatsData();
                } finally {
                    loadingStates.sizestats = false;
                }
            }
        })();
    });
    
    // Latency enable 시 통계 데이터 로드
    $effect(() => {
        (async () => {
            if (islatency && !loadingStates.latency && !currentStats.dtocStat) {
                loadingStates.latency = true;
                try {
                    await loadStatsData();
                } finally {
                    loadingStates.latency = false;
                }
            }
        })();
    });

    // BigInt 직렬화 처리를 위한 함수
    function serializeBigInt(data) {
        return JSON.stringify(data, (key, value) => 
            typeof value === 'bigint' ? value.toString() + 'n' : value
        );
    }

    // BigInt 역직렬화 처리를 위한 함수
    function deserializeBigInt(jsonString) {
        return JSON.parse(jsonString, (key, value) => {
            if (typeof value === 'string' && /^\d+n$/.test(value)) {
                return BigInt(value.slice(0, -1));
            }
            return value;
        });
    }
    function delay(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }
    // 필터링된 데이터 설정
    async function updateFilteredData() {
        if ($selectedTrace) {
            isLoading = true;
            console.log('[Trace] 필터링된 데이터 요청 중...');
            const filterStart = performance.now();
            
            try {
                const result = await filterTraceData(fileNames[$selectedTrace], tracedata, $selectedTrace, $filtertrace);
                if (result !== null) {
                    const filterEnd = performance.now();
                    console.log(`[Performance] filterTraceData 완료: ${(filterEnd - filterStart).toFixed(2)}ms`);
                    
                    filteredData[$selectedTrace] = result[$selectedTrace];
                    
                    // ⚡ 성능 최적화: tick만 대기하고 인위적 delay 제거
                    await tick();
                    
                    const totalEnd = performance.now();
                    console.log(`[Performance] 전체 필터링+렌더링: ${(totalEnd - filterStart).toFixed(2)}ms`);
                }
                return true;
            } catch (error) {
                console.error('[Trace] 데이터 필터링 오류:', error);
                return false;
            } finally {
                // 작업이 성공하든 실패하든 로딩 상태 해제
                isLoading = false;
            }
        }
        return false;
    }

    // 선택된 유형에 따라 통계 데이터 로드
    async function loadStatsData() {
        try {
            if ($selectedTrace === 'ufs') {
                const stats = await fetchUfsStats(fileNames.ufs, $filtertrace);
                ufsStats = stats;
            } else if ($selectedTrace === 'block') {
                const stats = await fetchBlockStats(fileNames.block, $filtertrace);
                blockStats = stats;
            }
        } catch (error) {
            console.error('[Trace] 통계 데이터 로드 중 오류 발생:', error);
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
            
            const result = await invoke<string[]>("export_to_csv", { 
                parquetPath: parquetFiles[currentType], 
                fileType: currentType
            });
            
            // 여러 파일이 생성된 경우 메시지 표시
            if (result.length > 1) {
                exportResult = `CSV 파일이 엑셀 행 제한으로 인해 ${result.length}개 파일로 분할되었습니다:\n${result.map((path, index) => `${index + 1}. ${path}`).join('\n')}`;
            } else {
                exportResult = result[0];
            }
            
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
            console.log('setParquetFilePaths - data.logname:', data.logname);
            console.log('setParquetFilePaths - data.logtype:', data.logtype);
            
            const names = data.logname.split(',');
            console.log('setParquetFilePaths - names:', names);
            
            if (data.logtype === 'block') {
                // block만 있는 경우
                fileNames.block = names[0];
                parquetFiles.block = names[0];
                fileNames.ufs = '';
                parquetFiles.ufs = '';
            } else if (data.logtype === 'ufs') {
                // ufs만 있는 경우
                fileNames.ufs = names[0];
                parquetFiles.ufs = names[0];
                fileNames.block = '';
                parquetFiles.block = '';
            } else if (data.logtype === 'both' || names.length > 1) {
                // 둘 다 있는 경우
                console.log('Processing both UFS and Block files');
                if (names.length > 0) {
                    fileNames.ufs = names[0];
                    parquetFiles.ufs = names[0];
                }
                
                if (names.length > 1) {
                    fileNames.block = names[1];
                    parquetFiles.block = names[1];
                }
            } else {
                // 기타 경우
                console.log('Unknown logtype, using first name for both');
                fileNames.ufs = names[0] || '';
                parquetFiles.ufs = names[0] || '';
                fileNames.block = names[0] || '';
                parquetFiles.block = names[0] || '';
            }
            
            console.log('setParquetFilePaths - final fileNames:', fileNames);
        }
    }

    // 트레이스 데이터 로딩 함수 추출
    async function loadTraceData() {
        try {
            isLoading = true;
            loadError = '';
                        
            // 캐시 키 구성
            const cacheKey = `traceData_${id}_${data.logfolder}_${data.logname}`;
            
            // IndexedDB에서 캐시된 데이터 불러오기
            let cached = null;
            try {
                cached = await get(cacheKey);
            } catch (cacheError) {
                console.warn('[Performance] 캐시 읽기 실패, 원본 데이터 로드:', cacheError);
            }
            
            if (cached && cached.ufs && cached.block) {
                try {
                    console.log('[Performance] 캐시된 데이터 발견, Arrow Table 복원 중...');
                    const restoreStart = performance.now();
                    
                    // Arrow IPC 바이너리에서 Table 복원
                    const ufsBytes = cached.ufs.bytes instanceof Uint8Array 
                        ? cached.ufs.bytes 
                        : new Uint8Array(cached.ufs.bytes);
                    const blockBytes = cached.block.bytes instanceof Uint8Array
                        ? cached.block.bytes
                        : new Uint8Array(cached.block.bytes);
                    
                    const ufsTable = tableFromIPC(ufsBytes);
                    const blockTable = tableFromIPC(blockBytes);
                    
                    tracedata = {
                        ufs: {
                            table: ufsTable,
                            total_count: cached.ufs.total_count,
                            sampled_count: cached.ufs.sampled_count,
                            sampling_ratio: cached.ufs.sampling_ratio
                        },
                        block: {
                            table: blockTable,
                            total_count: cached.block.total_count,
                            sampled_count: cached.block.sampled_count,
                            sampling_ratio: cached.block.sampling_ratio
                        }
                    };
                    
                    const restoreEnd = performance.now();
                    console.log(`[Performance] 캐시 복원 완료: ${(restoreEnd - restoreStart).toFixed(2)}ms`);
                } catch (restoreError) {
                    console.warn('[Performance] 캐시 복원 실패, 원본 데이터 로드:', restoreError);
                    cached = null; // 복원 실패 시 원본 데이터 로드하도록
                }
            }
            
            if (!cached) {
                const readtraceStart = performance.now();
                const result: any = await invoke('readtrace', {
                    logfolder: data.logfolder,
                    logname: data.logname,
                    maxrecords: buffersize
                });
                const readtraceEnd = performance.now();
                console.log(`[Performance] readtrace 완료: ${(readtraceEnd - readtraceStart).toFixed(2)}ms`);
                
                const parseStart = performance.now();
                // Arrow IPC 데이터 직접 변환 (압축 없음)
                const ufsData = new Uint8Array(result.ufs.bytes);
                const blockData = new Uint8Array(result.block.bytes);
                const parseEnd = performance.now();
                console.log(`[Performance] Arrow IPC 데이터 변환 완료: ${(parseEnd - parseStart).toFixed(2)}ms`);
                
                const tableStart = performance.now();                
                const ufsTable = tableFromIPC(ufsData);
                const blockTable = tableFromIPC(blockData);
                const tableEnd = performance.now();
                console.log(`[Performance] Arrow Table 생성 시간: ${(tableEnd - tableStart).toFixed(2)}ms`);                
                console.log('[Performance] Arrow Table 생성 완료');
                
                // ⚡ 성능 최적화: Arrow Table 직접 사용, toArray() 제거
                tracedata = {
                    ufs: {
                        table: ufsTable,  // Table 객체 저장
                        total_count: result.ufs.total_count,
                        sampled_count: result.ufs.sampled_count,
                        sampling_ratio: result.ufs.sampling_ratio
                    },
                    block: {
                        table: blockTable,  // Table 객체 저장
                        total_count: result.block.total_count,
                        sampled_count: result.block.sampled_count,
                        sampling_ratio: result.block.sampling_ratio
                    }
                };
                
                // ⚡ 최적화: Arrow IPC 바이너리를 직접 캐싱 (직렬화 불필요)
                const cacheStart = performance.now();
                try {
                    await set(cacheKey, {
                        ufs: {
                            bytes: ufsData,  // Uint8Array 직접 저장 (IndexedDB는 TypedArray 지원)
                            total_count: result.ufs.total_count,
                            sampled_count: result.ufs.sampled_count,
                            sampling_ratio: result.ufs.sampling_ratio
                        },
                        block: {
                            bytes: blockData,  // Uint8Array 직접 저장
                            total_count: result.block.total_count,
                            sampled_count: result.block.sampled_count,
                            sampling_ratio: result.block.sampling_ratio
                        }
                    });
                    const cacheEnd = performance.now();
                    console.log(`[Performance] Arrow IPC 바이너리 캐싱 완료: ${(cacheEnd - cacheStart).toFixed(2)}ms`);
                } catch (cacheError) {
                    console.warn('[Performance] 캐싱 실패 (무시하고 계속):', cacheError);
                    // 캐싱 실패해도 계속 진행
                }
            }
            
            // 데이터 저장 및 초기화
            $trace = tracedata;
            filteredData = tracedata;

            // 파일 경로 설정
            setParquetFilePaths();

            // // 초기 통계 데이터 로드
            // await loadStatsData();
            
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
                // 재귀 호출 시 isLoading이 중첩 설정될 수 있으므로 일시적으로 false로 설정
                isLoading = false;
                return loadTraceData();
            }
            
            return false;
        } finally {
            isLoading = false;
        }
    }
    
    // 수동 재시도 함수
    async function retryLoading() {
        try {
            isLoading = true;
            showRetryDialog = false;
            retryCount = 0; // 수동 재시도시 카운트 초기화
            
            // 차트 키 변경으로 강제 재렌더링
            chartKey++;
            console.log('[Trace] 차트 리렌더링 키 변경:', chartKey);
            
            const success = await loadTraceData();
            
            if (success) {
                // 필터링된 데이터 설정 및 통계 데이터 로드
                await updateFilteredData();
                await loadStatsData();
                
                // 차트 렌더링을 위한 추가 지연
                await delay(300);
            } else if (retryCount >= maxRetries) {
                // 최대 재시도 횟수 초과하면 홈으로 이동
                goto('/');
            }
        } catch (error) {
            console.error('[Trace] 재시도 중 오류 발생:', error);
        } finally {
            console.log('[Trace] 재시도 작업 완료, 로딩 상태 해제');
            isLoading = false;
        }
    }

    onMount(async () => {
        try {
            isLoading = true;
            // 테스트 정보 가져오기
            data = await getTestInfo(id);
            buffersize = await getBufferSize();
            
            // 파일 경로 설정
            setParquetFilePaths();

            traceLengths = await fetchTraceLengths(data.logname);
            tracetype = Object.keys(traceLengths).filter((key) => traceLengths[key] > 0);

            // if (tracetype.length > 0) {
            //     selectedTrace.set(tracetype[0]);
            // }
        } catch (error) {
            if (error instanceof Error) {
                console.error('Error during onMount:', error.message);
                console.error('Stack trace:', error.stack);
            } else {
                console.error('Unknown error:', error);
            }
            goto('/');
        } finally {
            isLoading = false;
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
        <div class="fixed top-4 left-4">
            <div class="flex items-center gap-2">
                <SelectType tracetype={tracetype} class="h-12"/>
                
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
                
                <div class="text-sm font-medium">{data.title}</div>
                
                {#if $selectedTrace !== '' && filteredData[$selectedTrace]?.total_count && filteredData[$selectedTrace].total_count !== filteredData[$selectedTrace].sampled_count}
                <div class="flex gap-2 text-xs text-gray-400 items-center ml-auto">
                    <span>total: {filteredData[$selectedTrace].total_count}</span>
                    <span>sampling: {filteredData[$selectedTrace].sampled_count}</span>
                    <span>sample ratio: {filteredData[$selectedTrace].sampling_ratio.toFixed(2)}%</span>
                </div>
                {/if}
            </div>
        </div>
        {:else}
        {/if}
        {#if loadError}
        {/if}        
    </header>    
    <main class="mx-auto p-6">
        {#if $selectedTrace != '' && filteredData}
        <VisualItem bind:ispattern bind:isrwd bind:isqd bind:iscpu bind:islatency bind:issizestats />                 
        <div class="grid grid-cols-2 gap-4">
            <div class="col-span-2">
                {#if ispattern}
                <Card.Root class={ispattern ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Pattern</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        <ScatterChartsDeck
                            key={chartKey}
                            table={currentFilteredTable}
                            data={currentFiltered}
                            xAxisKey='time'
                            yAxisKey={patternAxis.key}
                            legendKey={legendKey}
                            yAxisLabel={patternAxis.label}
                            ycolumn={patternAxis.column}
                        />
                    </Card.Content>
                </Card.Root>
                {/if}                
                {#if isqd}
                <Separator class="my-4 {isqd ? 'block' : 'hidden'}" />
                <Card.Root class={isqd ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} QueueDepth</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        <ScatterChartsDeck
                            key={chartKey}
                            table={currentFilteredTable}
                            data={currentFiltered}
                            xAxisKey='time'
                            yAxisKey='qd'
                            legendKey={legendKey}
                            yAxisLabel='qd'
                            ycolumn='qd'
                        />
                    </Card.Content>
                </Card.Root>
                {/if}
                {#if iscpu}
                <Separator class="my-4 {iscpu ? 'block' : 'hidden'}" />
                <Card.Root class={iscpu ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} CPU</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if $selectedTrace === 'ufs'} 
                        <CPUTabs key={chartKey} traceType={$selectedTrace} table={filteredData.ufs?.table} data={filteredData.ufs?.data} legendKey='cpu' />
                        {:else if $selectedTrace === 'block'}
                        <CPUTabs key={chartKey} traceType={$selectedTrace} table={filteredData.block?.table} data={filteredData.block?.data} legendKey='cpu' />
                        {/if}                        
                    </Card.Content>
                </Card.Root>
                {/if}
                {#if isrwd}
                <Separator class="my-4 {isrwd ? 'block' : 'hidden'}" />
                <Card.Root class={isrwd ? 'block' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Read/Write/Discard Statistics</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if loadingStates.rwd}
                        <div class="flex justify-center items-center h-64">
                            <Circle2 color="#FF3E00" size="60" unit="px" />
                        </div>
                        {:else if $selectedTrace === 'ufs'} 
                        <RWDStats key={chartKey} data={ufsStats.continuous} tracetype={$selectedTrace} {isrwd} />
                        {:else if $selectedTrace === 'block'}
                        <RWDStats key={chartKey} data={blockStats.continuous} tracetype={$selectedTrace} {isrwd} />
                        {/if}
                    </Card.Content>
                </Card.Root>                
                {/if}
                {#if islatency}
                <Separator class="my-4 {islatency ? 'block' : 'hidden'}" />
                <Card.Root class={islatency ? 'block' : 'hidden'}>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Latency</Card.Title>
                    </Card.Header>
                    <Card.Content>
                        {#if loadingStates.latency || !currentStats.dtocStat}
                        <div class="flex justify-center items-center h-64">
                            <Circle2 color="#FF3E00" size="60" unit="px" />
                        </div>
                        {:else}
                        <LatencyTabs
                            key={chartKey}
                            traceType={$selectedTrace}
                            filteredData={currentFiltered}
                            filteredTable={currentFilteredTable}
                            legendKey={legendKey}
                            thresholds={thresholds}
                            dtocStat={currentStats.dtocStat}
                            ctodStat={currentStats.ctodStat}
                            ctocStat={currentStats.ctocStat}
                        />
                        {/if}
                    </Card.Content>
                </Card.Root>                                
                {/if}
            </div>
            {#if issizestats}
            <div class="col-span-2 {issizestats ? 'block' : 'hidden'}">
                <Separator class="my-4" />          
                <Card.Root>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Size</Card.Title>
                        <Card.Description>Size별 Count</Card.Description>
                    </Card.Header>
                    <Card.Content>
                        {#if loadingStates.sizestats}
                        <div class="flex justify-center items-center h-64">
                            <Circle2 color="#FF3E00" size="60" unit="px" />
                        </div>
                        {:else if currentStats.sizeCounts?.opcode_stats}
                        <SizeStats key={chartKey} opcode_size_counts={currentStats.sizeCounts.opcode_stats} />
                        {/if}
                    </Card.Content>
                </Card.Root> 
            </div>
            {/if}
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