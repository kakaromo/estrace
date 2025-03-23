<script lang='ts'>
    import { getAllTestInfo, updateReparseResult } from '$api/db';
    import { testinfoid, initialTraceData } from '$stores/trace';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    import { message } from "@tauri-apps/plugin-dialog";

    import { Badge } from "$lib/components/ui/badge";
    import { Circle2 } from 'svelte-loading-spinners';
    import Separator from '$lib/components/ui/separator/separator.svelte';
    import { Button } from "$lib/components/ui/button";
    import { traceStatusStore, Status } from '$stores/file';
    import VirtualList from '@sveltejs/svelte-virtual-list';

    interface TestInfo {
        id: number;
        title: string;
        content: string;
        logtype: string;
        logfolder: string;
        logname: string;
        sourcelog_path: string;
    }
    
    let testData:TestInfo[] = $state([]);
    let isLoading = $state(false);
    let reparsingId = $state<number | null>(null);
    
    // 열 너비 정의
    const columnWidths = {
        id: '80px',
        title: '350px',
        logfolder: '200px',
        logname: '150px',
        actions: '150px'
    };

    // 행 클릭 핸들러 - 상세 정보로 이동
    function handleRowClick(item: TestInfo) {
        initialTraceData();
        $testinfoid = item.id;
        goto('/detail/');
    }
    
    // 재파싱 버튼 클릭 핸들러
    async function handleReparse(event: Event, id: number) {
        // 이벤트 버블링 방지
        event.stopPropagation();
        
        // 이미 처리 중이면 중복 실행 방지
        if (reparsingId !== null) return;
        
        try {
            reparsingId = id;
            const testInfo = testData.find(item => item.id === id);
            
            if (!testInfo) {
                throw new Error(`테스트 정보를 찾을 수 없습니다 (ID: ${id})`);
            }

            // 원본 로그 파일의 전체 경로 구성
            // 로그 파일 이름은 title.log 형태로 저장된다고 가정
            const logFilePath = `${testInfo.logfolder}/${testInfo.title}.log`;
            
            console.log(`Reparsing trace: ${testInfo.title} (ID: ${id})`);
            
            // 백엔드 호출하여 재파싱 실행
            const result = await invoke<string>('reparse_trace', {
                id,
                logfilePath: testInfo.sourcelog_path,
                logfolder: testInfo.logfolder,
            });
            
            // 파싱 결과 확인 및 DB 업데이트
            const parsedResult = JSON.parse(result);
            
            // 파싱 결과의 파일 경로들을 합쳐서 logname으로 저장
            let logname = parsedResult.ufs_parquet_filename;
            if (parsedResult.block_parquet_filename) {
                logname = logname + "," + parsedResult.block_parquet_filename;
            }
            
            // DB 업데이트
            await updateReparseResult(id, logname);
            
            await message(`트레이스 ${id}번이 성공적으로 재파싱되었습니다.`);
            
            // 테이블 데이터 새로고침
            testData = await getAllTestInfo();
        } catch (error) {
            console.error('Reparse failed:', error);
            await message(`재파싱 실패: ${error.message || error}`);
        } finally {
            reparsingId = null;
        }
    }

    // trace 성공하면 table update
    $effect(() => {
        if ($traceStatusStore === Status.Success) {
            isLoading = true;
            void (async () => {
                try {
                    testData = await getAllTestInfo();
                } catch (error) {
                    console.error('Failed to update test data:', error);
                } finally {
                    isLoading = false;
                }
            })();
        }
    });
    
    onMount(async () => {
        isLoading = true;
        try {
            testData = await getAllTestInfo();
        } catch (error) {
            console.error('Error loading test data:', error);
        } finally {
            isLoading = false;
        }
    });
</script>

<div class="container font-sans">
    <div class="space-y-1">
        <div>
            <h3 class="text-lg font-medium">
                Trace Information
            </h3>
            <p class="text-muted-foreground text-sm">테스트 트레이스 정보 목록입니다. 항목을 클릭하여 상세 정보를 확인하거나 재파싱할 수 있습니다.</p>
        </div>
        <Separator class="my-4" />
    </div>
    
    {#if isLoading}
    <div class="spinner-overlay">
        <Circle2 color="#FF3E00" size="60" unit="px" />
    </div>
    {/if}
    
    <div class="table-container">
        <div class="virtual-table">
            <!-- 테이블 헤더 -->
            <div class="table-header">
                <div class="header-cell" style="width: {columnWidths.id}">ID</div>
                <div class="header-cell" style="width: {columnWidths.title}">Title</div>
                <div class="header-cell" style="width: {columnWidths.logfolder}">Log Folder</div>
                <div class="header-cell" style="width: {columnWidths.logname}">Log File</div>
                <div class="header-cell" style="width: {columnWidths.actions}">Actions</div>
            </div>
            
            <!-- 테이블 바디 (VirtualList 사용) -->
            <div class="table-body">
                <VirtualList items={testData} let:item height="calc(100vh - 200px)" itemHeight={36}>
                    <div 
                        class="table-row hover:bg-gray-100" 
                        onclick={() => handleRowClick(item)}
                    >
                        <div class="cell" style="width: {columnWidths.id}">{item.id}</div>
                        <div class="cell" style="width: {columnWidths.title}">
                            <span class="badge-container">
                                <Badge variant="outline">{item.logtype}</Badge>
                                {item.title}
                            </span>
                        </div>
                        <div class="cell" style="width: {columnWidths.logfolder}">{item.logfolder}</div>
                        <div class="cell" style="width: {columnWidths.logname}">{item.logname}</div>
                        <div class="cell" style="width: {columnWidths.actions}">
                            <div class="action-buttons">
                                <button 
                                    class="reparse-button{reparsingId === item.id ? ' reparsing' : ''}" 
                                    disabled={reparsingId === item.id}
                                    onclick={(e) => handleReparse(e, item.id)}
                                >
                                    {#if reparsingId === item.id}
                                        <svg class="animate-spin" xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                                        </svg>
                                        <span>Processing...</span>
                                    {:else}
                                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M21 2v6h-6"/>
                                            <path d="M3 12a9 9 0 0 1 15-6.7L21 8"/>
                                            <path d="M3 22v-6h6"/>
                                            <path d="M21 12a9 9 0 0 1-15 6.7L3 16"/>
                                        </svg>
                                        <span>Reparse</span>
                                    {/if}
                                </button>
                            </div>
                        </div>
                    </div>
                </VirtualList>
            </div>
        </div>
    </div>
</div>

<style>
    .container {
      display: flex;
      flex-direction: column;      
      width: 100%;
      height: 100vh;
      overflow: hidden;      
      font-size: 12px;
    }
    
    .table-container {
        flex-grow: 1;
        overflow: hidden;
    }

    /* 가상 테이블 스타일링 */
    .virtual-table {
        width: 100%;
        height: 100%;
        display: flex;
        flex-direction: column;
    }
    
    .table-header {
        display: flex;
        background-color: #f9fafb;
        border-bottom: 1px solid #e5e7eb;
        font-weight: 600;
        font-size: 0.875rem;
    }
    
    .header-cell {
        padding: 0.75rem 1rem;
        text-align: left;
    }
    
    .table-body {
        flex: 1;
        overflow-y: auto;
    }
    
    .table-row {
        display: flex;
        align-items: center;
        border-bottom: 1px solid #e5e7eb;
        cursor: pointer;
    }
    
    .cell {
        padding: 0.5rem 1rem;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    /* Spinner overlay styling */
    .spinner-overlay {
        position: absolute;
        top: 0;
        left: 0;
        width: 100%;
        height: 100%;
        background: rgba(255, 255, 255, 0.8);
        display: flex;
        flex-direction: column;
        justify-content: center;
        align-items: center;
        z-index: 10;
    }
    
    /* Badge styling */
    :global(.badge-container .badge) {
        display: inline-block;
        padding: 0.125rem 0.375rem;
        font-size: 0.75rem;
        font-weight: 500;
        line-height: 1;
        border-radius: 0.375rem;
        background-color: #f3f4f6;
        border: 1px solid #e5e7eb;
        margin-right: 0.5rem;
    }
    
    /* Action buttons styling */
    .action-buttons {
        display: flex;
        gap: 8px;
        justify-content: center;
    }
    
    .reparse-button {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 4px 8px;
        background-color: #f3f4f6;
        border: 1px solid #e5e7eb;
        border-radius: 4px;
        font-size: 12px;
        cursor: pointer;
        transition: all 0.2s;
    }
    
    .reparse-button:hover {
        background-color: #e5e7eb;
    }
    
    .reparse-button.reparsing {
        background-color: #dbeafe;
        border-color: #93c5fd;
        cursor: not-allowed;
    }
</style>