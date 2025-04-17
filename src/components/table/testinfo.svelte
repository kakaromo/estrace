<script lang='ts'>
    import { getAllTestInfo, updateReparseResult, deleteTestInfo, deleteMultipleTestInfo } from '$api/db';
    import { testinfoid, initialTraceData } from '$stores/trace';
    import { onMount, onDestroy } from 'svelte';
    import { goto } from '$app/navigation';
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { message, confirm } from "@tauri-apps/plugin-dialog";
    import { open } from "@tauri-apps/plugin-shell";
    import { revealItemInDir } from '@tauri-apps/plugin-opener';
    import { platform } from '@tauri-apps/plugin-os';

    import { Badge } from "$lib/components/ui/badge";
    import { Circle2 } from 'svelte-loading-spinners';
    import Separator from '$lib/components/ui/separator/separator.svelte';
    import { Button } from "$lib/components/ui/button";
    import { traceStatusStore, Status } from '$stores/file';
    import VirtualList from '@sveltejs/svelte-virtual-list';
    import { Trash2, RefreshCw, Loader2, FolderOpen } from 'lucide-svelte';

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
    let selectedItems = $state<Set<number>>(new Set());
    let selectAll = $state(false);
    
    // 재파싱 진행 상태 추적 변수
    let reParseProgressValue = $state(0);
    let reParseProgressStage = $state('');
    let reParseProgressMessage = $state('');
    let reParseIsCancelled = $state(false);
    
    // 이벤트 리스너 정리 함수
    let unlisten = null;

    let start = $state(0);
    let end = $state(0);
    
    // 열 너비 정의 - 반응형 상태로 변경
    let columnWidths = $state({
        checkbox: '50px',
        id: '70px',
        title: '330px',
        logfolder: '200px',
        logname: '150px',
        actions: '210px'
    });
    
    // 컬럼 리사이징 관련 상태 변수
    let isResizing = $state(false);
    let currentResizingColumn = $state<string | null>(null);
    let startX = $state(0);
    let startWidth = $state(0);
    
    // 운영체제 구분 변수 추가
    let currentPlatform = $state<string>('');
    
    // 컬럼 리사이징 시작 핸들러
    function startResize(event: MouseEvent, columnKey: string) {
        event.preventDefault();
        isResizing = true;
        currentResizingColumn = columnKey;
        startX = event.clientX;
        
        // 현재 컬럼 너비를 숫자로 파싱
        startWidth = parseInt(columnWidths[columnKey], 10);
        
        // 전역 이벤트 리스너 추가
        document.addEventListener('mousemove', handleMouseMove);
        document.addEventListener('mouseup', stopResize);
    }
    
    // 마우스 이동 시 리사이징 처리
    function handleMouseMove(event: MouseEvent) {
        if (!isResizing || !currentResizingColumn) return;
        
        const delta = event.clientX - startX;
        const newWidth = Math.max(50, startWidth + delta); // 최소 너비 50px
        
        // 새 너비로 업데이트
        columnWidths = {
            ...columnWidths,
            [currentResizingColumn]: `${newWidth}px`
        };
    }
    
    // 리사이징 종료 핸들러
    function stopResize() {
        isResizing = false;
        currentResizingColumn = null;
        
        // 전역 이벤트 리스너 제거
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', stopResize);
    }
    
    // 행 클릭 핸들러 - 상세 정보로 이동
    function handleRowClick(item: TestInfo) {
        initialTraceData();
        $testinfoid = item.id;
        goto('/detail/');
    }
    
    // 체크박스 클릭 시 이벤트 버블링 방지
    function handleCheckboxClick(event: Event) {
        event.stopPropagation();
    }
    
    // 항목 선택 핸들러
    function toggleItemSelection(event: Event, id: number) {
        event.stopPropagation();
        
        // 새 Set 객체를 생성하여 반응성 트리거
        const newSelectedItems = new Set(selectedItems);
        
        if (newSelectedItems.has(id)) {
            newSelectedItems.delete(id);
        } else {
            newSelectedItems.add(id);
        }
        
        // 새 객체로 할당하여 반응성 보장
        selectedItems = newSelectedItems;
        
        // 전체 선택 상태 업데이트
        selectAll = testData.length > 0 && selectedItems.size === testData.length;
    }
    
    // 전체 선택/해제 핸들러
    function toggleSelectAll() {
        if (selectAll) {
            // 모두 해제 - 새 Set 객체 생성으로 반응성 트리거
            selectedItems = new Set();
        } else {
            // 모두 선택
            selectedItems = new Set(testData.map(item => item.id));
        }
        selectAll = !selectAll;
    }
    
    // 폴더 경로 추출 함수 개선 - 더 정확한 test 폴더 식별
    function extractFolderPaths(lognames: string[]): string[] {
        const folderPaths = new Set<string>();
        
        lognames.forEach(logname => {
            if (!logname) return;
            
            const path = logname.trim();
            if (path === '') return;
            
            console.log('Processing path:', path);
            
            // 경로에서 파일명을 제외한 디렉토리 경로 추출
            // 윈도우, 리눅스 모두 지원하기 위해 '/'와 '\\' 모두 확인
            const normalizedPath = path.replace(/\\/g, '/'); // 모든 백슬래시를 슬래시로 변환
            const lastSlashIndex = normalizedPath.lastIndexOf('/');
            
            if (lastSlashIndex > 0) {
                // 디렉토리 경로 추출 (파일명 제외)
                const folderPath = path.substring(0, lastSlashIndex);
                console.log('Extracted folder path:', folderPath);
                
                // test 폴더를 찾는 로직 개선
                // 다양한 패턴의 test 폴더를 지원
                const normalizedFolderPath = folderPath.replace(/\\/g, '/').toLowerCase();
                
                // 여러 패턴 체크
                const testPatterns = ['/test', '/tests', '/test_', '_test'];
                let foundTestPath = null;
                
                // 여러 패턴 중 가장 마지막에 나오는 test 경로를 찾음
                for (const pattern of testPatterns) {
                    const testDirIndex = normalizedFolderPath.lastIndexOf(pattern);
                    if (testDirIndex >= 0) {
                        // test 폴더 경로 추출 (pattern을 포함하는 위치까지)
                        const testPathCandidate = folderPath.substring(0, 
                            folderPath.length - (normalizedFolderPath.length - testDirIndex) + pattern.length);
                        
                        // 가장 마지막에 나오는 패턴을 사용
                        if (!foundTestPath || testDirIndex > normalizedFolderPath.lastIndexOf(foundTestPath)) {
                            foundTestPath = testPathCandidate;
                        }
                    }
                }
                
                if (foundTestPath) {
                    console.log('Found test folder path:', foundTestPath);
                    folderPaths.add(foundTestPath);
                } else {
                    // test 폴더를 못 찾은 경우 원래 폴더 경로 추가
                    console.log('No test folder found, using original path:', folderPath);
                    folderPaths.add(folderPath);
                }
            }
        });
        
        console.log('All folders to delete:', [...folderPaths]);
        return Array.from(folderPaths);
    }
    
    // 폴더 삭제 함수 개선 - 더 나은 오류 처리 및 재시도 로직
    async function deleteFolder(folderPath: string): Promise<boolean> {
        if (!folderPath || folderPath.trim() === '') {
            console.log('Empty folder path, skipping');
            return true;
        }
        
        try {
            console.log('Attempting to delete folder:', folderPath);
            await invoke('delete_folder', { folderPath });
            console.log('Successfully deleted folder:', folderPath);
            return true;
        } catch (error) {
            console.warn(`Failed to delete folder ${folderPath}:`, error);
            
            // 폴더가 이미 삭제되었는지 확인하는 로직을 추가할 수 있지만,
            // 현재는 단순히 실패로 처리
            return false;
        }
    }
    
    // 선택한 항목 삭제 핸들러 개선
    async function handleDeleteSelected() {
        if (selectedItems.size === 0) {
            await message('삭제할 항목을 선택해주세요.');
            return;
        }
        
        const confirmed = await confirm(
            `선택한 ${selectedItems.size}개 항목을 삭제하시겠습니까?`,
            { title: '삭제 확인', type: 'warning' }
        );
        
        if (confirmed) {
            try {
                isLoading = true;
                
                // 선택된 항목의 parquet 파일 삭제
                const selectedTestData = testData.filter(item => selectedItems.has(item.id));
                const filesToDelete = selectedTestData
                    .flatMap(item => item.logname ? item.logname.split(',') : [])
                    .filter(Boolean);
                
                if (filesToDelete.length > 0) {
                    try {
                        // 1. 먼저 파일 삭제
                        console.log('Files to delete:', filesToDelete);
                        await invoke('delete_parquet_files', { filePaths: filesToDelete });
                        console.log('Successfully deleted parquet files');
                        
                        // 2. 모든 관련 폴더 삭제 (test 폴더 찾기)
                        const folderPaths = extractFolderPaths(filesToDelete);
                        
                        // 더 명확한 로깅
                        console.log(`Attempting to delete ${folderPaths.length} folders`);
                        
                        for (const folderPath of folderPaths) {
                            // 각 폴더 삭제 시도
                            const success = await deleteFolder(folderPath);
                            
                            if (!success) {
                                console.warn(`Could not delete folder: ${folderPath}. Will continue with other operations.`);
                                
                                // 실패한 경우에도 계속 진행
                                // 추가 오류 정보 로깅
                                try {
                                    // 재시도 - 일부 파일 시스템은 지연 후 삭제가 성공하는 경우도 있음
                                    console.log('Retrying folder deletion after delay...');
                                    await new Promise(resolve => setTimeout(resolve, 500));
                                    await deleteFolder(folderPath);
                                } catch (retryError) {
                                    console.warn('Retry also failed:', retryError);
                                }
                            }
                        }
                    } catch (error) {
                        console.warn('Error during file/folder deletion:', error);
                        // 파일/폴더 삭제에 실패해도 DB에서는 삭제 진행
                    }
                }
                
                // DB에서 삭제
                await deleteMultipleTestInfo(Array.from(selectedItems));
                
                // 테이블 데이터 새로고침
                testData = await getAllTestInfo();
                
                // 선택 항목 초기화
                selectedItems = new Set(); // clear()를 사용하지 않고 새 Set 객체 할당
                selectAll = false;
                
                await message(`선택한 항목이 삭제되었습니다.`);
            } catch (error) {
                console.error('Delete failed:', error);
                await message(`삭제 실패: ${error.message || error}`);
            } finally {
                isLoading = false;
            }
        }
    }
    
    // 단일 항목 삭제 핸들러 개선
    async function handleDeleteItem(event: Event, id: number) {
        event.stopPropagation();
        
        const testInfo = testData.find(item => item.id === id);
        if (!testInfo) return;
        
        const confirmed = await confirm(
            `"${testInfo.title}" 항목을 삭제하시겠습니까?`,
            { title: '삭제 확인', type: 'warning' }
        );
        
        if (confirmed) {
            try {
                isLoading = true;
                
                // parquet 파일 삭제
                const filesToDelete = testInfo.logname ? testInfo.logname.split(',') : [];
                if (filesToDelete.length > 0) {
                    try {
                        // 1. 먼저 파일 삭제
                        await invoke('delete_parquet_files', { filePaths: filesToDelete });
                        console.log('Successfully deleted parquet files');
                        
                        // 2. 모든 관련 폴더 삭제 (test 폴더 찾기)
                        const folderPaths = extractFolderPaths(filesToDelete);
                        console.log(`Attempting to delete ${folderPaths.length} folders`);
                        
                        for (const folderPath of folderPaths) {
                            // 각 폴더 삭제 시도
                            const success = await deleteFolder(folderPath);
                            
                            if (!success) {
                                console.warn(`Could not delete folder: ${folderPath}. Will continue with other operations.`);
                                
                                // 실패한 경우에도 계속 진행
                                // 추가 오류 정보 로깅
                                try {
                                    // 재시도 - 일부 파일 시스템은 지연 후 삭제가 성공하는 경우도 있음
                                    console.log('Retrying folder deletion after delay...');
                                    await new Promise(resolve => setTimeout(resolve, 500));
                                    await deleteFolder(folderPath);
                                } catch (retryError) {
                                    console.warn('Retry also failed:', retryError);
                                }
                            }
                        }
                    } catch (error) {
                        console.warn('Error during file/folder deletion:', error);
                        // 파일/폴더 삭제에 실패해도 DB에서는 삭제 진행
                    }
                }
                
                // DB에서 삭제
                await deleteTestInfo(id);
                
                // selectedItems에서도 제거
                if (selectedItems.has(id)) {
                    // 새 Set 객체를 생성하여 반응성 트리거
                    const newSelectedItems = new Set(selectedItems);
                    newSelectedItems.delete(id);
                    selectedItems = newSelectedItems;
                }
                
                // 테이블 데이터 새로고침
                testData = await getAllTestInfo();
                selectAll = testData.length > 0 && selectedItems.size === testData.length;
                
                await message('항목이 삭제되었습니다.');
            } catch (error) {
                console.error('Delete failed:', error);
                await message(`삭제 실패: ${error.message || error}`);
            } finally {
                isLoading = false;
            }
        }
    }
    
    // 재파싱 취소 함수
    async function cancelReparse() {
        try {
            reParseIsCancelled = true;
            await invoke('cancel_trace_process');
            console.log('재파싱 취소 요청 전송됨');
        } catch (error) {
            console.error('재파싱 취소 실패:', error);
        }
    }
    
    // 재파싱 재시작 함수
    async function restartReparse(id: number) {
        try {
            // 취소 신호 초기화
            await invoke('reset_cancel_signal');
            reParseIsCancelled = false;
            
            // 재파싱 시작
            handleReparse(new Event('click'), id);
        } catch (error) {
            console.error('재파싱 재시작 실패:', error);
            await message('재파싱 재시작에 실패했습니다.');
        }
    }
    
    // 재파싱 버튼 클릭 핸들러
    async function handleReparse(event: Event, id: number) {
        // 이벤트 버블링 방지
        event.stopPropagation();
        
        // 이미 처리 중이면 중복 실행 방지
        if (reparsingId !== null) return;
        
        try {
            reparsingId = id;
            reParseIsCancelled = false;
            reParseProgressValue = 0;
            reParseProgressStage = 'init';
            reParseProgressMessage = '재파싱 준비 중...';
            
            const testInfo = testData.find(item => item.id === id);
            
            if (!testInfo) {
                throw new Error(`테스트 정보를 찾을 수 없습니다 (ID: ${id})`);
            }

            // 기존 parquet 파일들의 경로 확인
            const existingFiles = testInfo.logname ? testInfo.logname.split(',') : [];
            
            console.log(`Reparsing trace: ${testInfo.title} (ID: ${id})`);
            console.log(`Old parquet files: ${existingFiles.join(', ')}`);
            
            // 기존 파일 삭제 - 백엔드에서 처리
            if (existingFiles.length > 0) {
                try {
                    await invoke('delete_parquet_files', { filePaths: existingFiles });
                    console.log('Successfully deleted old parquet files');
                } catch (error) {
                    // 파일 삭제 실패해도 계속 진행
                    console.warn('Error deleting old files:', error);
                }
            }
            
            // 백엔드 호출하여 재파싱 실행
            const result = await invoke<string>('reparse_trace', {
                id,
                logfilePath: testInfo.sourcelog_path,
                logfolder: testInfo.logfolder,
                window: null // Tauri가 window 객체를 자동으로 처리
            });
            
            // 사용자가 취소한 경우 메시지 표시하고 종료
            if (reParseIsCancelled) {
                console.log('User cancelled the reparse operation');
                return;
            }
            
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
            
            // 사용자가 취소한 경우가 아니면 오류 메시지 표시
            if (!reParseIsCancelled) {
                await message(`재파싱 실패: ${error.message || error}`);
            }
        } finally {
            // 취소 여부에 관계없이 재파싱 ID 초기화
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
            // 운영체제 정보 가져오기
            currentPlatform = await platform();
            console.log('현재 운영체제:', currentPlatform);
            
            // 진행 상태 이벤트 리스너 설정
            unlisten = await listen('trace-progress', (event) => {
                const progress = event.payload;
                
                reParseProgressStage = progress.stage;
                reParseProgressValue = progress.progress;
                reParseProgressMessage = progress.message;
                
                // 진행 상태가 완료되면 타이머 중지
                if (reParseProgressStage === 'complete') {
                    console.log('재파싱 완료됨');
                }
                
                // 상태 업데이트 로깅
                console.log(`진행 상태 업데이트: ${reParseProgressStage} - ${reParseProgressValue}% - ${reParseProgressMessage}`);
            });
        } catch (error) {
            console.error('Error loading test data:', error);
        } finally {
            isLoading = false;
        }
    });
    
    // 컴포넌트 소멸 시 이벤트 리스너 정리
    onDestroy(() => {
        if (unlisten) {
            unlisten();
        }
    });

    // 경로에서 폴더 부분 추출하는 함수
    function getFolderPath(path: string): string {
        if (!path) return '';
        
        // 콤마로 구분된 경우 첫 번째 경로만 사용
        const actualPath = path.includes(',') ? path.split(',')[0] : path;
        
        // 경로에서 폴더 부분 추출
        const normalizedPath = actualPath.replace(/\\/g, '/');
        const lastSlashIndex = normalizedPath.lastIndexOf('/');
        
        if (lastSlashIndex > 0) {
            return actualPath.substring(0, lastSlashIndex);
        }
        
        return '';
    }
    
    // 파일 탐색기에서 경로 열기 - OS별 로직 추가
    async function openInExplorer(event: Event, path: string) {
        event.stopPropagation(); // 이벤트 버블링 방지
        
        if (!path || path.trim() === '') {
            await message('열 수 있는 경로가 없습니다.');
            return;
        }
        
        try {
            console.log('파일 탐색기에서 열기:', path);
            
            // 운영체제별 처리
            if (currentPlatform === 'win32') {
                // Windows - explorer 명령어 사용
                // 백슬래시로 변환
                const winPath = path.replace(/\//g, '\\');
                console.log('Windows 경로:', winPath);
                
                // explorer 명령어로 폴더 열기
                await open(`explorer "${winPath}"`);
            } else {
                // macOS, Linux 등의 경우 기존 함수 사용
                await revealItemInDir(path);
            }
        } catch (error) {
            console.error('파일 탐색기 열기 실패:', error);
            await message(`파일 탐색기를 열 수 없습니다: ${error.message || '경로가 유효하지 않습니다'}`);
            
            // 대체 방법 시도
            try {
                console.log('대체 방법으로 파일 탐색기 열기 시도');
                await open(getFolderPath(path));
            } catch (fallbackError) {
                console.error('대체 방법도 실패:', fallbackError);
                await message(`대체 방법도 실패: ${fallbackError.message || '알 수 없는 오류'}`);
            }
        }
    }
    
    // 파일 경로에서 파일명만 추출
    function getFileName(path: string): string {
        if (!path) return '';
        
        const normalizedPath = path.replace(/\\/g, '/');
        const lastSlashIndex = normalizedPath.lastIndexOf('/');
        
        if (lastSlashIndex >= 0 && lastSlashIndex < normalizedPath.length - 1) {
            return path.substring(lastSlashIndex + 1);
        }
        
        return path;
    }

    // 진행 상태 단계 설명 반환 함수
    function getStageDescription(stage: string): string {
        const stageDescriptions: Record<string, string> = {
            init: '초기화 중',
            processing: '처리 중',
            complete: '완료됨',
        };
        return stageDescriptions[stage] || stage;
    }
</script>

<div class="container font-sans">
    <div class="space-y-1">
        <div class="flex justify-between items-center">
            <div>
                <h3 class="text-lg font-medium">
                    Trace Information
                </h3>
                <p class="text-muted-foreground text-sm">테스트 트레이스 정보 목록입니다. 항목을 클릭하여 상세 정보를 확인하거나 재파싱할 수 있습니다.</p>
            </div>
            
            <div>
                <Button 
                    variant="destructive" 
                    size="sm" 
                    disabled={selectedItems.size === 0}
                    onclick={handleDeleteSelected}
                >
                    <Trash2 class="mr-1" size={14} />
                    선택 항목 삭제 ({selectedItems.size})
                </Button>
            </div>
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
                <div class="header-cell" style="width: {columnWidths.checkbox}">
                    <input 
                        type="checkbox" 
                        checked={selectAll} 
                        onchange={toggleSelectAll}
                        disabled={testData.length === 0}
                    />
                    <div class="resize-handle" role="separator" aria-orientation="vertical" onmousedown={(e) => startResize(e, 'checkbox')}></div>
                </div>
                <div class="header-cell" style="width: {columnWidths.id}">
                    ID
                    <div class="resize-handle" role="separator" aria-orientation="vertical" onmousedown={(e) => startResize(e, 'id')}></div>
                </div>
                <div class="header-cell" style="width: {columnWidths.title}">
                    Title
                    <div class="resize-handle" role="separator" aria-orientation="vertical" onmousedown={(e) => startResize(e, 'title')}></div>
                </div>
                <div class="header-cell" style="width: {columnWidths.logfolder}">
                    Log Folder
                    <div class="resize-handle" role="separator" aria-orientation="vertical" onmousedown={(e) => startResize(e, 'logfolder')}></div>
                </div>
                <div class="header-cell" style="width: {columnWidths.logname}">
                    Log File
                    <div class="resize-handle" role="separator" aria-orientation="vertical" onmousedown={(e) => startResize(e, 'logname')}></div>
                </div>
                <div class="header-cell" style="width: {columnWidths.actions}">
                    Actions
                </div>
            </div>
            
            <!-- 테이블 바디 (VirtualList 사용) -->
            <div class="table-body">
                <VirtualList items={testData} bind:start bind:end let:item height="calc(100vh - 200px)" itemHeight={36}>
                    <div 
                        class="table-row hover:bg-gray-100"
                        role="button"
                        tabindex="0" 
                        onclick={() => handleRowClick(item)}
                        onkeydown={(e) => e.key === 'Enter' && handleRowClick(item)}
                    >
                        <div class="cell" style="width: {columnWidths.checkbox}">
                            <input 
                                type="checkbox" 
                                checked={selectedItems.has(item.id)} 
                                onclick={handleCheckboxClick}
                                onchange={(e) => toggleItemSelection(e, item.id)}
                            />
                        </div>
                        <div class="cell" style="width: {columnWidths.id}">{item.id}</div>
                        <div class="cell" style="width: {columnWidths.title}">
                            <span class="badge-container">
                                <Badge variant="outline">{item.logtype}</Badge>
                                {item.title}
                            </span>
                        </div>
                        <div 
                            class="cell clickable-cell" 
                            style="width: {columnWidths.logfolder}"
                            onclick={(e) => openInExplorer(e, getFolderPath(item.sourcelog_path))}
                            title="클릭하여 소스 로그 폴더 열기"
                        >
                            <div class="folder-path">
                                <span>{getFolderPath(item.sourcelog_path)}</span>
                                <FolderOpen size={14} class="folder-icon" />
                            </div>
                        </div>
                        <div 
                            class="cell clickable-cell" 
                            style="width: {columnWidths.logname}"
                            onclick={(e) => item.logname && openInExplorer(e, getFolderPath(item.logname))}
                            title="클릭하여 파싱된 파일 폴더 열기"
                        >
                            <div class="folder-path">
                                <span>{getFolderPath(item.logname)}</span>
                                <FolderOpen size={14} class="folder-icon" />
                            </div>
                        </div>
                        {#if reparsingId === item.id}
                            <!-- 재파싱 진행 상태 표시 UI -->
                            <div class="cell reparse-progress-cell" style="width: {columnWidths.actions}">
                                <div class="reparse-progress">
                                    <div class="progress-header">
                                        <span class="progress-stage">{reParseProgressStage ? getStageDescription(reParseProgressStage) : '준비 중'}</span>
                                        <span class="progress-percent">{reParseProgressValue.toFixed(1)}%</span>
                                    </div>
                                    
                                    <div class="progress-bar-container">
                                        <div class="progress-bar" style="width: {reParseProgressValue}%"></div>
                                    </div>
                                    
                                    <div class="progress-message">{reParseProgressMessage}</div>
                                    
                                    {#if reParseIsCancelled}
                                        <div class="reparse-controls">
                                            <button class="restart-button" onclick={(e) => { e.stopPropagation(); restartReparse(item.id); }}>
                                                <RefreshCw size={12} />
                                                재시작
                                            </button>
                                        </div>
                                    {:else}
                                        <div class="reparse-controls">
                                            <button class="cancel-button" onclick={(e) => { e.stopPropagation(); cancelReparse(); }}>
                                                <span>취소</span>
                                            </button>
                                        </div>
                                    {/if}
                                </div>
                            </div>
                        {:else}
                            <div class="cell" style="width: {columnWidths.actions}">
                                <div class="action-buttons">
                                    <button 
                                        class="reparse-button" 
                                        onclick={(e) => handleReparse(e, item.id)}
                                    >
                                        <RefreshCw size={12} />
                                        <span>Reparse</span>
                                    </button>
                                    
                                    <button 
                                        class="delete-button" 
                                        onclick={(e) => handleDeleteItem(e, item.id)}
                                    >
                                        <Trash2 size={12} />
                                        <span>Delete</span>
                                    </button>
                                </div>
                            </div>
                        {/if}
                    </div>
                </VirtualList>
            </div>
            <p>showing {start}-{end} of {testData.length} rows</p>
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
        user-select: none; /* 드래그 중 텍스트 선택 방지 */
    }
    
    .header-cell {
        padding: 0.75rem 1rem;
        text-align: left;
        position: relative; /* 리사이징 핸들 포지셔닝을 위해 */
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }
    
    /* 리사이징 핸들 스타일 */
    .resize-handle {
        position: absolute;
        right: 0;
        top: 0;
        bottom: 0;
        width: 5px;
        cursor: col-resize;
        background-color: transparent;
    }
    
    .resize-handle:hover, .resize-handle:active {
        background-color: rgba(0, 0, 0, 0.1);
    }
    
    /* 리사이징 중 커서 스타일 */
    :global(body.resizing) {
        cursor: col-resize !important;
        user-select: none;
    }
    
    /* 드래그 중 커서 스타일 적용을 위한 전역 클래스 */
    :global(.resizing *) {
        cursor: col-resize !important;
    }
    
    /* 현재 리사이징 중인 컬럼 강조 */
    .header-cell.resizing .resize-handle {
        background-color: rgba(0, 0, 0, 0.2);
    }
    
    /* 테이블 행 스타일 조정 */
    .table-row {
        display: flex;
        align-items: center;
        border-bottom: 1px solid #e5e7eb;
        cursor: pointer;
    }
    
    /* 셀 스타일 - 리사이즈 반영을 위해 overflow 처리 */
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
    
    .reparse-button, .delete-button {
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
    
    .delete-button {
        background-color: #fee2e2;
        border-color: #fecaca;
    }
    
    .delete-button:hover {
        background-color: #fecaca;
    }
    
    .reparse-button.reparsing {
        background-color: #dbeafe;
        border-color: #93c5fd;
        cursor: not-allowed;
    }
    
    /* Checkbox styling */
    input[type="checkbox"] {
        width: 16px;
        height: 16px;
        cursor: pointer;
    }

    /* 클릭 가능한 셀 스타일 */
    .clickable-cell {
        cursor: pointer;
        color: #2563eb; /* 파란색으로 강조 */
        text-decoration: underline;
    }
    
    .clickable-cell:hover {
        color: #1d4ed8; /* 호버 시 더 진한 파란색 */
        background-color: #f0f9ff; /* 밝은 파란색 배경 */
    }
    
    /* 폴더 경로 컨테이너 */
    .folder-path {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }
    
    .folder-path span {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    
    .folder-icon {
        opacity: 0;
        margin-left: 4px;
        flex-shrink: 0;
    }
    
    .clickable-cell:hover .folder-icon {
        opacity: 1;
    }

    /* 재파싱 진행 상태 UI 스타일 */
    .reparse-progress-cell {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: 4px;
    }

    .reparse-progress {
        width: 100%;
    }

    .progress-header {
        display: flex;
        justify-content: space-between;
        font-size: 12px;
        font-weight: 500;
    }

    .progress-bar-container {
        width: 100%;
        height: 8px;
        background-color: #e5e7eb;
        border-radius: 4px;
        overflow: hidden;
        position: relative;
    }

    .progress-bar {
        height: 100%;
        background-color: #2563eb;
        transition: width 0.2s ease-in-out;
    }

    .progress-message {
        font-size: 12px;
        color: #6b7280;
    }

    .reparse-controls {
        display: flex;
        gap: 8px;
    }

    .restart-button, .cancel-button {
        padding: 4px 8px;
        font-size: 12px;
        border: none;
        border-radius: 4px;
        cursor: pointer;
        transition: background-color 0.2s ease-in-out;
    }

    .restart-button {
        background-color: #dbeafe;
        color: #2563eb;
    }

    .restart-button:hover {
        background-color: #bfdbfe;
    }

    .cancel-button {
        background-color: #fee2e2;
        color: #b91c1c;
    }

    .cancel-button:hover {
        background-color: #fecaca;
    }
</style>