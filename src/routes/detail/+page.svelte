<script lang="ts">
    // import { page } from '$app/state';
    import { onMount, tick } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    import { readFile, remove } from "@tauri-apps/plugin-fs";
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
    import { Toaster } from "$lib/components/ui/sonner";
    import { toast } from "svelte-sonner";

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
        fetchUfscustomStats,
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
    let legendKey:string = $derived($selectedTrace === 'ufs' || $selectedTrace === 'ufscustom' ? 'opcode' : 'io_type');
    let patternAxis:Object = $derived($selectedTrace === 'ufs' || $selectedTrace === 'ufscustom'
        ? { key: 'lba', label: '4KB', column: 'lba' }
        : { key: 'sector', label: 'sector', column: 'sector' });
    // UFSCUSTOM은 start_time 사용, 일반 trace는 time 사용
    let timeField:string = $derived($selectedTrace === 'ufscustom' ? 'start_time' : 'time');
    let currentStats:Object = $derived(
        $selectedTrace === 'ufs' ? ufsStats : 
        $selectedTrace === 'ufscustom' ? ufscustomStats : 
        blockStats
    );
    let isLoading:boolean = $state(false);

    // Retry 관련 상태 추가
    let loadError:string = $state('');
    let retryCount:number = $state(0);
    let maxRetries:number = 3;
    let showRetryDialog:boolean = $state(false);
    // 차트 리렌더링을 위한 키 추가
    let chartKey:number = $state(0);
    
    // 시각화 항목 상태
    let ispattern = $state(false);
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

    // UFSCUSTOM 통계 데이터
    let ufscustomStats = $state({
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
        block: '',
        ufscustom: ''
    });

    let fileNames = $state({
        ufs: '',
        block: '',
        ufscustom: ''
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
                zoom_column: ($selectedTrace === 'ufs' || $selectedTrace === 'ufscustom') ? 'lba' : 'sector',
                from_time: 0.0,
                to_time: 0.0,
                from_lba: 0.0,
                to_lba: 0.0,
            };
            
            // UFSCUSTOM 선택 시 CPU 차트 비활성화 (CPU 정보 없음)
            if ($selectedTrace === 'ufscustom' && iscpu) {
                iscpu = false;
            }
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
            } else if ($selectedTrace === 'ufscustom') {
                const stats = await fetchUfscustomStats(fileNames.ufscustom, $filtertrace);
                ufscustomStats = stats;
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
                fileNames.ufscustom = '';
                parquetFiles.ufscustom = '';
            } else if (data.logtype === 'ufs') {
                // ufs만 있는 경우
                fileNames.ufs = names[0];
                parquetFiles.ufs = names[0];
                fileNames.block = '';
                parquetFiles.block = '';
                fileNames.ufscustom = '';
                parquetFiles.ufscustom = '';
            } else if (data.logtype === 'ufscustom') {
                // ufscustom만 있는 경우
                fileNames.ufscustom = names[0];
                parquetFiles.ufscustom = names[0];
                fileNames.ufs = '';
                parquetFiles.ufs = '';
                fileNames.block = '';
                parquetFiles.block = '';
            } else if (data.logtype === 'both' || names.length > 1) {
                // 여러 개 있는 경우
                console.log('Processing multiple trace files');
                if (names.length > 0) {
                    fileNames.ufs = names[0];
                    parquetFiles.ufs = names[0];
                }
                
                if (names.length > 1) {
                    fileNames.block = names[1];
                    parquetFiles.block = names[1];
                }
                
                if (names.length > 2) {
                    fileNames.ufscustom = names[2];
                    parquetFiles.ufscustom = names[2];
                }
            } else {
                // 기타 경우
                console.log('Unknown logtype, using first name for all');
                fileNames.ufs = names[0] || '';
                parquetFiles.ufs = names[0] || '';
                fileNames.block = names[0] || '';
                parquetFiles.block = names[0] || '';
                fileNames.ufscustom = names[0] || '';
                parquetFiles.ufscustom = names[0] || '';
            }
            
            console.log('setParquetFilePaths - final fileNames:', fileNames);
        }
    }

    // 트레이스 데이터 로딩 함수 추출
    async function loadTraceData() {
        try {
            isLoading = true;
            loadError = '';
            
            // 로딩 시작 알림
            toast.info('데이터 로딩 중...', {
                description: `${$selectedTrace.toUpperCase()} 트레이스 데이터를 불러오고 있습니다.`,
                duration: 2000,
            });
                        
            // 캐시 키 구성
            const cacheKey = `traceData_${id}_${data.logfolder}_${data.logname}`;
            
            // IndexedDB에서 캐시된 데이터 불러오기
            let cached = null;
            try {
                cached = await get(cacheKey);
            } catch (cacheError) {
                console.warn('[Performance] 캐시 읽기 실패, 원본 데이터 로드:', cacheError);
            }
            
            if (cached && cached.ufs && cached.block && cached.ufscustom) {
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
                    const ufscustomBytes = cached.ufscustom.bytes instanceof Uint8Array
                        ? cached.ufscustom.bytes
                        : new Uint8Array(cached.ufscustom.bytes);
                    
                    const ufsTable = tableFromIPC(ufsBytes);
                    const blockTable = tableFromIPC(blockBytes);
                    const ufscustomTable = tableFromIPC(ufscustomBytes);
                    
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
                        },
                        ufscustom: {
                            table: ufscustomTable,
                            total_count: cached.ufscustom.total_count,
                            sampled_count: cached.ufscustom.sampled_count,
                            sampling_ratio: cached.ufscustom.sampling_ratio
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
                // 파일 기반 전송 사용 - 53s → 15s (73% 성능 개선)
                const result: any = await invoke('readtrace_to_files', {
                    logfolder: data.logfolder,
                    logname: data.logname,
                    maxrecords: buffersize
                });
                const readtraceEnd = performance.now();
                console.log(`[Performance] readtrace_to_files 완료: ${(readtraceEnd - readtraceStart).toFixed(2)}ms`);
                
                const readFileStart = performance.now();
                // 파일에서 바이너리 데이터 읽기
                const ufsData = await readFile(result.ufs_path);
                const blockData = await readFile(result.block_path);
                const ufscustomData = await readFile(result.ufscustom_path);
                const readFileEnd = performance.now();
                console.log(`[Performance] 파일 읽기 완료: ${(readFileEnd - readFileStart).toFixed(2)}ms`);
                
                // 파일 읽기 완료 후 즉시 삭제
                let ufsRemoved = false, blockRemoved = false, ufscustomRemoved = false;
                try {
                    await remove(result.ufs_path);
                    ufsRemoved = true;
                } catch (ufsRemoveError) {
                    console.warn(
                        `⚠️  임시 파일 삭제 실패 (ufs): ${result.ufs_path}\n` +
                        `오류: ${ufsRemoveError}\n` +
                        `가능한 원인: 파일이 이미 삭제되었거나, 권한이 없거나, 다른 프로세스에서 사용 중일 수 있습니다.\n` +
                        `해결 방법: 파일이 존재하는지, 권한이 충분한지, 다른 프로그램에서 사용 중인지 확인하세요.`
                    );
                }
                try {
                    await remove(result.block_path);
                    blockRemoved = true;
                } catch (blockRemoveError) {
                    console.warn(
                        `⚠️  임시 파일 삭제 실패 (block): ${result.block_path}\n` +
                        `오류: ${blockRemoveError}\n` +
                        `가능한 원인: 파일이 이미 삭제되었거나, 권한이 없거나, 다른 프로세스에서 사용 중일 수 있습니다.\n` +
                        `해결 방법: 파일이 존재하는지, 권한이 충분한지, 다른 프로그램에서 사용 중인지 확인하세요.`
                    );
                }
                try {
                    await remove(result.ufscustom_path);
                    ufscustomRemoved = true;
                } catch (ufscustomRemoveError) {
                    console.warn(
                        `⚠️  임시 파일 삭제 실패 (ufscustom): ${result.ufscustom_path}\n` +
                        `오류: ${ufscustomRemoveError}\n` +
                        `가능한 원인: 파일이 이미 삭제되었거나, 권한이 없거나, 다른 프로세스에서 사용 중일 수 있습니다.\n` +
                        `해결 방법: 파일이 존재하는지, 권한이 충분한지, 다른 프로그램에서 사용 중인지 확인하세요.`
                    );
                }
                if (ufsRemoved && blockRemoved && ufscustomRemoved) {
                    console.log('✅ 임시 파일 삭제 완료');
                }
                
                const tableStart = performance.now();                
                const ufsTable = tableFromIPC(ufsData);
                const blockTable = tableFromIPC(blockData);
                const ufscustomTable = tableFromIPC(ufscustomData);
                const tableEnd = performance.now();
                console.log(`[Performance] Arrow Table 생성 시간: ${(tableEnd - tableStart).toFixed(2)}ms`);                
                console.log('[Performance] Arrow Table 생성 완료');
                
                // ⚡ 성능 최적화: Arrow Table 직접 사용, toArray() 제거
                tracedata = {
                    ufs: {
                        table: ufsTable,  // Table 객체 저장
                        total_count: result.ufs_total_count,
                        sampled_count: result.ufs_sampled_count,
                        sampling_ratio: result.ufs_sampling_ratio
                    },
                    block: {
                        table: blockTable,  // Table 객체 저장
                        total_count: result.block_total_count,
                        sampled_count: result.block_sampled_count,
                        sampling_ratio: result.block_sampling_ratio
                    },
                    ufscustom: {
                        table: ufscustomTable,  // Table 객체 저장
                        total_count: result.ufscustom_total_count,
                        sampled_count: result.ufscustom_sampled_count,
                        sampling_ratio: result.ufscustom_sampling_ratio
                    }
                };
                
                // ⚡ 최적화: Arrow IPC 바이너리를 직접 캐싱 (직렬화 불필요)
                const cacheStart = performance.now();
                try {
                    await set(cacheKey, {
                        ufs: {
                            bytes: ufsData,  // Uint8Array 직접 저장 (IndexedDB는 TypedArray 지원)
                            total_count: result.ufs_total_count,
                            sampled_count: result.ufs_sampled_count,
                            sampling_ratio: result.ufs_sampling_ratio
                        },
                        block: {
                            bytes: blockData,  // Uint8Array 직접 저장
                            total_count: result.block_total_count,
                            sampled_count: result.block_sampled_count,
                            sampling_ratio: result.block_sampling_ratio
                        },
                        ufscustom: {
                            bytes: ufscustomData,  // Uint8Array 직접 저장
                            total_count: result.ufscustom_total_count,
                            sampled_count: result.ufscustom_sampled_count,
                            sampling_ratio: result.ufscustom_sampling_ratio
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
            
            // 🎉 로딩 완료 알림 (데이터 포인트 개수 포함)
            const totalPoints = tracedata[$selectedTrace]?.total_count || 0;
            const sampledPoints = tracedata[$selectedTrace]?.sampled_count || 0;
            
            toast.success('🎉 데이터 로딩 완료!', {
                description: `${$selectedTrace.toUpperCase()} 트레이스: ${sampledPoints.toLocaleString()}개 포인트가 준비되었습니다.`,
                duration: 3000,
            });
            
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
                toast.error('데이터 로딩 실패', {
                    description: '데이터를 불러올 수 없습니다. 다시 시도해주세요.',
                    duration: 4000,
                });
            } else {
                // 자동 재시도
                console.log(`자동 재시도 중... (${retryCount}/${maxRetries})`);
                toast.warning(`재시도 중... (${retryCount}/${maxRetries})`, {
                    description: '잠시 후 다시 시도합니다.',
                    duration: 1500,
                });
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
            
            // 🔧 UFSCUSTOM 업데이트로 인한 스키마 변경 - 오래된 캐시 자동 삭제
            // Cache version: v2 (2025-10-16) - 올바른 스키마의 빈 RecordBatch 포함
            const CACHE_VERSION = 'v2';
            const CACHE_VERSION_KEY = 'traceDataCacheVersion';
            
            try {
                const currentVersion = localStorage.getItem(CACHE_VERSION_KEY);
                if (currentVersion !== CACHE_VERSION) {
                    console.log(`[Cache] 캐시 버전 불일치 (현재: ${currentVersion}, 필요: ${CACHE_VERSION}) - 전체 캐시 삭제`);
                    
                    // IndexedDB 전체 삭제
                    const databases = await indexedDB.databases();
                    for (const db of databases) {
                        if (db.name === 'traceDataCache') {
                            console.log('[Cache] IndexedDB 삭제:', db.name);
                            indexedDB.deleteDatabase(db.name);
                        }
                    }
                    
                    // 버전 업데이트
                    localStorage.setItem(CACHE_VERSION_KEY, CACHE_VERSION);
                    console.log('[Cache] 캐시 버전 업데이트 완료');
                }
            } catch (cacheError) {
                console.warn('[Cache] 캐시 정리 중 오류:', cacheError);
            }
            
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
        <VisualItem 
            bind:ispattern 
            bind:isrwd 
            bind:isqd 
            bind:iscpu 
            bind:islatency 
            bind:issizestats 
            traceType={$selectedTrace}
        />                 
        <div class="grid grid-cols-2 gap-4">
            <div class="col-span-2">
                {#if ispattern}
                <Card.Root class={ispattern ? 'block overflow-visible' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Pattern</Card.Title>
                    </Card.Header>
                    <Card.Content class="overflow-visible">
                        <ScatterChartsDeck
                            key={chartKey}
                            table={currentFilteredTable}
                            data={currentFiltered}
                            xAxisKey={timeField}
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
                <Card.Root class={isqd ? 'block overflow-visible' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} QueueDepth</Card.Title>
                    </Card.Header>
                    <Card.Content class="overflow-visible">
                        <ScatterChartsDeck
                            key={chartKey}
                            table={currentFilteredTable}
                            data={currentFiltered}
                            xAxisKey={timeField}
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
                <Card.Root class={iscpu ? 'block overflow-visible' : 'hidden'} >
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} CPU</Card.Title>
                    </Card.Header>
                    <Card.Content class="overflow-visible">
                        {#if $selectedTrace === 'ufs'} 
                        <CPUTabs key={chartKey} traceType={$selectedTrace} table={filteredData.ufs?.table} data={filteredData.ufs?.data} legendKey='cpu' />
                        {:else if $selectedTrace === 'block'}
                        <CPUTabs key={chartKey} traceType={$selectedTrace} table={filteredData.block?.table} data={filteredData.block?.data} legendKey='cpu' />
                        {:else if $selectedTrace === 'ufscustom'}
                        <CPUTabs key={chartKey} traceType={$selectedTrace} table={filteredData.ufscustom?.table} data={filteredData.ufscustom?.data} legendKey='cpu' />
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
                        {:else if $selectedTrace === 'ufscustom'}
                        <RWDStats key={chartKey} data={ufscustomStats.continuous} tracetype={$selectedTrace} {isrwd} />
                        {/if}
                    </Card.Content>
                </Card.Root>                
                {/if}
                {#if islatency}
                <Separator class="my-4 {islatency ? 'block' : 'hidden'}" />
                <Card.Root class={islatency ? 'block overflow-visible' : 'hidden'}>
                    <Card.Header>
                        <Card.Title>{$selectedTrace.toUpperCase()} Latency</Card.Title>
                    </Card.Header>
                    <Card.Content class="overflow-visible">
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

<!-- Toast Notifications -->
<Toaster position="top-right" />

<style>
    .spinner-overlay {
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
    }
</style>