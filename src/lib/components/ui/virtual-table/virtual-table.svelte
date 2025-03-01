<script lang="ts">
    import VirtualList from '@sveltejs/svelte-virtual-list';
    import { createEventDispatcher } from 'svelte';
    import { onMount, onDestroy } from 'svelte';
    
    // Tauri v2 API 가져오기
    import { save } from '@tauri-apps/plugin-dialog';
    import { writeTextFile } from '@tauri-apps/plugin-fs';

    import { Download } from 'lucide-svelte';
    import { Button } from "$lib/components/ui/button";

    let {
        items = [],
        columns = [],
        // 선택적 props
        itemHeight = 36, // 각 행의 높이
        rowClass = "", // 행에 적용할 추가 클래스
        headerClass = "bg-gray-200 font-bold border-b border-gray-300", // 헤더 클래스
        stripedRows = false, // 줄무늬 행 패턴 활성화
        hoverable = true, // 행에 호버 효과 활성화
        selectable = true, // 행 선택 가능 여부
        fileName = "table-export" // CSV 파일 이름 기본값
    } = $props();

    // 상태 변수
    let selectedItem = $state(null); // 선택된 항목
    let start = $state(0);
    let end = $state(0);
    let showContextMenu = $state(false);
    let contextMenuX = $state(0);
    let contextMenuY = $state(0);
    let isExporting = $state(false); // 내보내기 진행 중 상태
    
    // 이벤트 디스패처 생성
    const dispatch = createEventDispatcher<{
        rowClick: { item: any };
        rowSelect: { item: any };
        sort: { column: any, direction: 'asc' | 'desc' };
        exportCsv: { data: string, path: string | null };
    }>();

    // 행 너비를 CSS grid-template-columns 형식으로 변환
    let gridTemplateColumns = $derived(columns.map(col => col.width).join(' '));

    // 행 클릭 핸들러
    function handleRowClick(item: any) {
        if (selectable) {
            selectedItem = item;
            dispatch('rowSelect', { item });
        }
        dispatch('rowClick', { item });
    }

    // 행 키보드 핸들러 (Enter 또는 Space 키)
    function handleRowKeyDown(event, item) {
        // Enter 또는 Space 키 눌렀을 때 클릭과 동일한 동작 수행
        if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault(); // 기본 동작 방지 (스크롤 등)
            handleRowClick(item);
        }
    }

    // 테이블 키보드 핸들러
    function handleTableKeyDown(event) {
        // 테이블 레벨의 키보드 탐색 핸들링 (옵션)
        // 현재는 기본만 구현
    }

    // 셀 내용 가져오기
    function getCellContent(item: any, column: any) {
        if (column.cell) {
            const result = column.cell(item);
            if (typeof result === 'string') {
                return result;
            } else {
                // 컴포넌트 반환 처리는 상태 관리가 필요하므로 일단 생략
                return '';
            }
        }

        if (typeof column.accessor === 'function') {
            return column.accessor(item);
        }
        
        return item[column.accessor];
    }

    // 원시 데이터 가져오기 (HTML 태그 없이)
    function getRawCellContent(item: any, column: any) {
        if (typeof column.accessor === 'function') {
            return column.accessor(item);
        }
        return item[column.accessor];
    }
    
    // 행에 적용할 클래스 계산
    function getRowClass(item: any, index: number) {
        let classes = rowClass + ' grid border-b border-gray-300';
        
        if (hoverable) classes += ' hover:bg-gray-100 cursor-pointer';
        if (stripedRows && index % 2 === 1) classes += ' bg-gray-50';
        if (selectedItem === item) classes += ' bg-blue-100';
        
        return classes;
    }

    // CSV로 내보내기
    async function exportToCsv() {
        if (isExporting) return; // 이미 진행 중이면 중복 실행 방지
        isExporting = true;
        let filePath = null;
        try {
            // CSV 헤더 생성
            const headers = columns.map(col => `"${col.header}"`).join(',');
            
            // 데이터 행 생성
            const rows = items.map(item => {
                return columns.map(col => {
                    let value = getRawCellContent(item, col);
                    // null 또는 undefined 처리
                    if (value === null || value === undefined) {
                        value = '';
                    }
                    // 문자열로 변환 및 쌍따옴표 처리 (CSV 형식 규칙)
                    const valueStr = String(value).replace(/"/g, '""');
                    return `"${valueStr}"`;
                }).join(',');
            }).join('\n');
            
            // 최종 CSV 문자열 (UTF-8 BOM 추가)
            const BOM = "\uFEFF";
            const csv = BOM + `${headers}\n${rows}`;
            
            // 파일 저장 대화상자 열기
            filePath = await save({
                filters: [{
                    name: 'CSV',
                    extensions: ['csv']
                }],
                defaultPath: `${fileName}.csv`
            });
            
            // 사용자가 저장 위치를 선택한 경우
            if (filePath) {                                
                await writeTextFile(filePath, csv);
                dispatch('exportCsv', { data: csv, path: filePath });      
            } else {
                // 사용자가 취소한 경우
                dispatch('exportCsv', { data: csv, path: null });
            }
        } catch (error) {
            console.error('CSV 내보내기 오류:', error);
            console.error('오류 세부 정보:', JSON.stringify(error, null, 2));
            dispatch('exportCsv', { data: '', path: null });
        } finally {
            // 컨텍스트 메뉴 닫기
            showContextMenu = false;
            isExporting = false;
        }
    }
    
    // 우클릭 이벤트 핸들러 (컨텍스트 메뉴 표시)
    function handleContextMenu(e) {
        e.preventDefault(); // 기본 컨텍스트 메뉴 방지
        
        // 마우스 위치 계산 - 페이지 내 위치 사용
        contextMenuX = e.pageX || e.clientX;
        contextMenuY = e.pageY || e.clientY;
        
        // 화면 경계 확인 - 컨텍스트 메뉴가 화면을 벗어나지 않도록
        const menuWidth = 180; // 컨텍스트 메뉴 예상 너비
        const menuHeight = 40; // 컨텍스트 메뉴 예상 높이
        
        // 화면 너비와 높이
        const windowWidth = window.innerWidth;
        const windowHeight = window.innerHeight;
        
        // 오른쪽 경계 확인
        if (contextMenuX + menuWidth > windowWidth) {
            contextMenuX = windowWidth - menuWidth;
        }
        
        // 아래쪽 경계 확인
        if (contextMenuY + menuHeight > windowHeight) {
            contextMenuY = windowHeight - menuHeight;
        }
        
        showContextMenu = true; // 컨텍스트 메뉴 표시
    }
    
    // 컨텍스트 메뉴 닫기
    function closeContextMenu() {
        showContextMenu = false;
    }
    
    // 문서 클릭시 컨텍스트 메뉴 닫기
    function handleDocumentClick(e) {
        // 컨텍스트 메뉴 외부 클릭 시에만 닫기
        if (showContextMenu) {
            const contextMenu = document.querySelector('.context-menu');
            // 클릭된 요소가 컨텍스트 메뉴나 그 자식이 아닌 경우에만 닫기
            if (contextMenu && !contextMenu.contains(e.target)) {
                closeContextMenu();
            }
        }
    }
    
    // 내보내기 버튼 클릭 핸들러 (이벤트 전파 중지)
    function handleExportClick(event) {
        event.stopPropagation();
        exportToCsv();
    }
    
    // 컴포넌트 마운트시 이벤트 리스너 추가
    onMount(() => {
        document.addEventListener('click', handleDocumentClick);
        // 스크롤 시에도 메뉴 닫기
        document.addEventListener('scroll', closeContextMenu);
        // 창 크기 변경 시 메뉴 닫기
        window.addEventListener('resize', closeContextMenu);
    });
    
    onDestroy(() => {
        document.removeEventListener('click', handleDocumentClick);
        document.removeEventListener('scroll', closeContextMenu);
        window.removeEventListener('resize', closeContextMenu);
    });
</script>

<!-- 테이블 컨테이너 -->
<div 
    class="virtual-table-container w-full font-sans" 
    oncontextmenu={handleContextMenu}
    onkeydown={handleTableKeyDown}
    tabindex="0"
    role="grid" 
    aria-rowcount={items.length + 1} 
    aria-colcount={columns.length}
>
    <!-- 테이블 헤더 -->
    <div 
        class="header grid {headerClass}" 
        style="grid-template-columns: {gridTemplateColumns};"
        role="row"
        aria-rowindex="1"
    >
        {#each columns as column, colIndex}
            <div 
                class="p-2 {column.className || ''}" 
                class:cursor-pointer={column.sortable}
                role="columnheader"
                aria-colindex={colIndex + 1}
            >
                {column.header}
            </div>
        {/each}
    </div>
    
    <!-- 테이블 본문 -->
    <div class="list-wrapper" role="rowgroup">
        <VirtualList {items} {itemHeight} bind:start bind:end let:item let:index>
            <div 
                class={getRowClass(item, index)}
                style="grid-template-columns: {gridTemplateColumns};"
                onclick={() => handleRowClick(item)}
                onkeydown={(e) => handleRowKeyDown(e, item)}
                role="row"
                aria-rowindex={index + 2} 
                tabindex="0" 
            >
                {#each columns as column, colIndex}
                    <div 
                        class="py-1 px-2 content-item overflow-hidden text-ellipsis {column.className || ''}"
                        role="gridcell"
                        aria-colindex={colIndex + 1}
                    >
                        {#if typeof column.cell === 'function'}
                            {@html getCellContent(item, column)}
                        {:else}
                            {getCellContent(item, column)}
                        {/if}
                    </div>
                {/each}
            </div>
        </VirtualList>
    </div>

    <!-- 테이블 푸터 -->
    <div class="footer p-2 text-sm text-gray-500">
        {#if items.length > 0}
            showing {start + 1}-{Math.min(end, items.length)} of {items.length} rows
        {:else}
            No data available
        {/if}
    </div>

    <!-- 커스텀 컨텍스트 메뉴 - 위치 조정 -->
    {#if showContextMenu}
        <div 
            class="context-menu absolute bg-white shadow-lg rounded-md py-1 z-50"
            style="left: {contextMenuX}px; top: {contextMenuY}px;"
            role="menu"
        >
            <button 
                class="menu-item flex items-center gap-2 w-full text-left px-4 py-2 hover:bg-gray-100"
                onclick={handleExportClick}
                disabled={isExporting}
                role="menuitem"
            >
                {#if isExporting}
                    <div class="animate-spin h-4 w-4 border-2 border-current border-t-transparent rounded-full mr-2" aria-hidden="true"></div>
                    <span>내보내는 중...</span>
                {:else}
                    <Download size={16} aria-hidden="true" />
                    <span>CSV로 내보내기</span>
                {/if}
            </button>
        </div>
    {/if}
</div>

<style>
    .virtual-table-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        overflow: hidden;
        position: relative; /* 컨텍스트 메뉴 위치 기준점 */
        outline: none; /* 포커스 아웃라인 제거 (필요에 따라 커스텀 스타일 추가 가능) */
    }
    
    .virtual-table-container:focus-visible {
        outline: 2px solid #4f46e5; /* 키보드 포커스 시 아웃라인 표시 */
        outline-offset: -2px;
    }
    
    .header {
        flex-shrink: 0;
        font-size: 0.8rem;
    }
    
    .list-wrapper {
        flex-grow: 1;
        overflow: auto;
        min-height: 200px;
    }
    
    .content-item {
        font-size: 0.8rem;
    }
    
    .footer {
        flex-shrink: 0;
        font-size: 0.75rem;
    }
    
    /* 컨텍스트 메뉴 스타일 */
    .context-menu {
        position: fixed; /* 페이지 스크롤에 영향 받지 않도록 고정 */
        min-width: 180px;
        box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
        border: 1px solid #e2e8f0;
        z-index: 1000; /* 다른 요소들 위에 표시 */
        font-size: 13px;
    }
    
    .menu-item {
        transition: background-color 0.2s;
    }
    
    .menu-item:disabled {
        opacity: 0.6;
        cursor: not-allowed;
    }
    
    /* 로딩 애니메이션 */
    .animate-spin {
        animation: spin 1s linear infinite;
    }
    
    @keyframes spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
    }
</style>