<script lang='ts'>
    import { onMount } from 'svelte';
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";
    import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
    import { Download } from "svelte-lucide";

    import { toast } from "svelte-sonner";
    import { Toaster } from "$lib/components/ui/sonner";

    // Tauri v2 API 가져오기
    import { save } from '@tauri-apps/plugin-dialog';
    import { writeTextFile } from '@tauri-apps/plugin-fs';
    import { writeText } from '@tauri-apps/plugin-clipboard-manager';

    // props 정의
    interface LatencyStatsProps {
        tracetype: string;
        latencystat: any;
        threshold: string[];
    }

    let { tracetype, latencystat, threshold }:LatencyStatsProps = $props();

    // 상태 변수
    let grid_columns_summary = $state<Array<{ id: string, header: string, width: number }>>([]);
    let grid_data_summary = $state<any[]>([]);
    let grid_columns = $state<Array<{ id: string, header: string, width: number }>>([]);
    let grid_data = $state<any[]>([]);
    let errorMsg = $state<string|null>(null);
    let isLoading = $state(true);
    let prevLatencystat = $state(null);

    // 컨텍스트 메뉴 상태
    let summaryContextMenuOpen = $state(false);
    let countsContextMenuOpen = $state(false);

    // latencystat 변경 시 데이터 처리
    $effect(() => {
        if (latencystat && JSON.stringify(latencystat) !== JSON.stringify(prevLatencystat)) {
            isLoading = true;
            prevLatencystat = JSON.parse(JSON.stringify(latencystat));
            processData();
        }
    });

    onMount(() => {
        if (latencystat) {
            processData();
        }
    });

    // 데이터 처리 함수
    function processData() {
        try {
            // 데이터 검증
            if (!latencystat || !latencystat.latency_counts || !latencystat.summary) {
                errorMsg = "데이터가 없거나 형식이 잘못되었습니다";
                isLoading = false;
                return;
            }
            
            // 서머리 데이터 처리
            processSummaryData();
            
            // 카운트 데이터 처리
            processCountsData();
            
            errorMsg = null;
            isLoading = false;
        } catch (error) {
            console.error('Error processing data:', error);
            errorMsg = `데이터 처리 오류: ${error.message || '알 수 없는 오류'}`;
            isLoading = false;
        }
    }

    // 서머리 데이터 그리드 생성
    function processSummaryData() {
        const summary = latencystat.summary;
        
        // 기본 컬럼 정의
        const baseColumns = [
            { id: "type", header: "Type", width: 150 },
            { id: "avg", header: "Avg", width: 100 },
            { id: "min", header: "Min", width: 100 },
            { id: "median", header: "Median", width: 100 },
            { id: "max", header: "Max", width: 100 },
            { id: "std_dev", header: "Std", width: 100 }
        ];

        // 퍼센타일 키 수집
        let percentileKeys: string[] = [];
        Object.values(summary).forEach((s: any) => {
            if (s && s.percentiles) {
                Object.keys(s.percentiles).forEach(k => {
                    if (!percentileKeys.includes(k)) percentileKeys.push(k);
                });
            }
        });
        
        // 퍼센타일 정렬
        percentileKeys.sort().reverse();
        
        // 그리드 컬럼 생성
        grid_columns_summary = [...baseColumns];
        percentileKeys.forEach(pk => {
            grid_columns_summary.push({ id: pk, header: pk, width: 100 });
        });
        
        // 그리드 데이터 생성
        grid_data_summary = Object.keys(summary).map(typeKey => {
            const s = summary[typeKey];
            if (!s) return { type: typeKey };
            
            const row: any = {
                type: typeKey,
                avg: formatNumber(s.avg),
                min: formatNumber(s.min),
                median: formatNumber(s.median),
                max: formatNumber(s.max),
                std_dev: formatNumber(s.std_dev)
            };
            
            // 퍼센타일 추가
            if (s.percentiles) {
                percentileKeys.forEach(pk => {
                    row[pk] = formatNumber(s.percentiles[pk]);
                });
            }
            
            return row;
        });
    }

    // 카운트 데이터 그리드 생성
    function processCountsData() {
        const counts = latencystat.latency_counts;
        
        // 타입 키와 접두사 패턴 확인
        const typeKeys = Object.keys(counts);
        if (typeKeys.length === 0) {
            return;
        }
        
        // 첫 번째 타입의 키를 기준으로 패턴 확인
        const firstType = typeKeys[0];
        const sampleKeys = Object.keys(counts[firstType]);
        
        // 패턴이 "숫자_텍스트" 형식인지 확인
        const hasPrefixPattern = sampleKeys.some(k => /^\d+_/.test(k));
        
        // 컬럼 설정
        grid_columns = [{ id: 'range', header: 'Range', width: 200 }];
        typeKeys.forEach(typeKey => {
            grid_columns.push({ id: typeKey, header: typeKey, width: 120 });
        });
        
        // 데이터 행 생성
        if (hasPrefixPattern) {
            // 접두사 있는 경우: 접두사로 정렬
            const firstTypeData = counts[firstType];
            const sortedKeys = Object.keys(firstTypeData)
                .filter(k => /^\d+_/.test(k))
                .sort((a, b) => {
                    const numA = parseInt(a.split('_')[0]);
                    const numB = parseInt(b.split('_')[0]);
                    return numA - numB;
                });
                
            grid_data = sortedKeys.map(key => {
                // 표시용 범위 텍스트 (접두사 제거)
                const displayRange = key.replace(/^\d+_/, '');
                
                const row: any = { range: displayRange };
                
                // 각 타입별 값 추가
                typeKeys.forEach(typeKey => {
                    row[typeKey] = counts[typeKey][key] || 0;
                });
                
                return row;
            });
        } else {
            // 임계값 기반 (threshold 배열 사용)
            grid_data = threshold.map((thresh, index) => {
                let displayRange: string;
                if (index === 0) {
                    displayRange = `≤ ${thresh}`;
                } else if (index === threshold.length - 1) {
                    displayRange = `> ${threshold[index-1]}`;
                } else {
                    displayRange = `${threshold[index-1]} < v ≤ ${thresh}`;
                }
                
                const row: any = { range: displayRange };
                
                typeKeys.forEach(typeKey => {
                    row[typeKey] = counts[typeKey][thresh] || 0;
                });
                
                return row;
            });
        }
    }

    // 소수점 3자리로 포맷팅
    function formatNumber(value: any) {
        if (value === undefined || value === null) return 0;
        if (typeof value === 'number') {
            return Number(value.toFixed(3));
        }
        return value;
    }

    // 요약 통계 CSV 다운로드 함수
    async function downloadSummaryCSV() {
        if (!grid_data_summary || grid_data_summary.length === 0) return;
        
        // CSV 헤더 생성
        let csvContent = grid_columns_summary.map(col => col.header).join(',') + '\n';
        
        // 데이터 행 추가
        grid_data_summary.forEach(row => {
            const rowData = grid_columns_summary.map(col => `"${row[col.id] ?? ''}"`).join(',');
            csvContent += rowData + '\n';
        });
        
        // 파일 저장 다이얼로그 표시
        const filePath = await save({
            filters: [{
                name: 'CSV',
                extensions: ['csv']
            }],
            defaultPath: `latency_summary_stats.csv`
        });
        
        if (filePath) {
            // 파일에 CSV 내용 쓰기
            await writeTextFile(filePath, csvContent);
            console.log(`CSV exported to ${filePath}`);
        }
    }
    
    // 지연 시간 카운트 CSV 다운로드 함수
    async function downloadCountsCSV() {
        if (!grid_data || grid_data.length === 0) return;
        
        // CSV 헤더 생성
        let csvContent = grid_columns.map(col => col.header).join(',') + '\n';
        
        // 데이터 행 추가
        grid_data.forEach(row => {
            const rowData = grid_columns.map(col => `"${row[col.id] ?? ''}"`).join(',');
            csvContent += rowData + '\n';
        });
        
        // 파일 저장 다이얼로그 표시
        const filePath = await save({
            filters: [{
                name: 'CSV',
                extensions: ['csv']
            }],
            defaultPath: `latency_counts.csv`
        });
        
        if (filePath) {
            // 파일에 CSV 내용 쓰기
            await writeTextFile(filePath, csvContent);
            console.log(`CSV exported to ${filePath}`);
        }
    }
    
    // 요약 통계 그리드에서 Ctrl+A 처리
    async function handleSummaryKeyDown(event: KeyboardEvent) {
        // Ctrl+A 또는 Command+A 감지
        if ((event.ctrlKey || event.metaKey) && event.key === 'a') {
            event.preventDefault();
            
            if (!grid_data_summary || grid_data_summary.length === 0) return;
            
            // 헤더 생성 (탭으로 구분)
            let clipboardText = grid_columns_summary.map(col => col.header).join('\t') + '\n';
            
            // 데이터 행 추가
            grid_data_summary.forEach(row => {
                const rowData = grid_columns_summary.map(col => row[col.id] ?? '').join('\t');
                clipboardText += rowData + '\n';
            });
            
            try {
                await writeText(clipboardText);
                console.log("Descriptive Statistics 데이터가 클립보드에 복사되었습니다.");
                toast.success("Descriptive Statistics 데이터가 클립보드에 복사되었습니다", {
                    description: "엑셀에 붙여넣기 가능합니다.",
                    duration: 2000,
                });
            } catch (error) {
                console.error("클립보드 복사 실패:", error);
            }
        }
    }
    
    // latency range count 그리드에서 Ctrl+A 처리
    async function handleCountsKeyDown(event: KeyboardEvent) {
        // Ctrl+A 또는 Command+A 감지
        if ((event.ctrlKey || event.metaKey) && event.key === 'a') {
            event.preventDefault();
            
            if (!grid_data || grid_data.length === 0) return;
            
            // 헤더 생성 (탭으로 구분)
            let clipboardText = grid_columns.map(col => col.header).join('\t') + '\n';
            
            // 데이터 행 추가
            grid_data.forEach(row => {
                const rowData = grid_columns.map(col => row[col.id] ?? '').join('\t');
                clipboardText += rowData + '\n';
            });
            
            try {
                await writeText(clipboardText);
                console.log("latency range count 데이터가 클립보드에 복사되었습니다.");
                toast.success("latency range count 데이터가 클립보드에 복사되었습니다", {
                    description: "엑셀에 붙여넣기 가능합니다.",
                    duration: 2000,
                });
            } catch (error) {
                console.error("클립보드 복사 실패:", error);
            }
        }
    }
</script>

<Toaster />

<div class="p-2">
    {#if errorMsg}
        <div class="alert alert-warning mb-4">
            <p>{errorMsg}</p>
        </div>
    {:else if isLoading}
        <div class="flex justify-center items-center p-4">
            <div class="animate-spin h-5 w-5 border-2 border-current border-t-transparent rounded-full mr-2"></div>
            <span>데이터 로딩 중...</span>
        </div>
    {:else}
        <!-- 서머리 그리드 -->
        {#if grid_data_summary.length > 0}
            <div class="mb-6">
                <div class="text-sm font-medium mb-2">Descriptive Statistics</div>
                <ContextMenu.Root bind:open={summaryContextMenuOpen}>
                    <ContextMenu.Trigger>
                        <Willow>
                            <!-- svelte-ignore a11y_interactive_supports_focus -->
                            <div 
                                style="font-size: 12px;"
                                role="grid"
                                aria-label="요약 통계"
                                tabindex="0"
                                onkeydown={handleSummaryKeyDown}
                                class="grid-container"
                            >
                                <Grid bind:data={grid_data_summary} bind:columns={grid_columns_summary}/>
                            </div>
                        </Willow>
                    </ContextMenu.Trigger>
                    <ContextMenu.Content class="w-48">
                        <ContextMenu.Item onclick={downloadSummaryCSV}>
                            <Download class="mr-2 h-4 w-4" />
                            <span>CSV로 다운로드</span>
                        </ContextMenu.Item>
                        <ContextMenu.Separator />
                        <ContextMenu.Item>
                            <span>행: {grid_data_summary.length}개</span>
                        </ContextMenu.Item>
                        <ContextMenu.Item onclick={() => handleSummaryKeyDown({ctrlKey: true, key: 'a', preventDefault: () => {}})}>
                            <span>모든 데이터 복사 (Ctrl+A)</span>
                        </ContextMenu.Item>
                    </ContextMenu.Content>
                </ContextMenu.Root>
            </div>
        {/if}
        
        <!-- 카운트 그리드 -->
        {#if grid_data.length > 0}
            <div>
                <div class="text-sm font-medium mb-2">latency count</div>
                <ContextMenu.Root bind:open={countsContextMenuOpen}>
                    <ContextMenu.Trigger>
                        <Willow>
                            <!-- svelte-ignore a11y_interactive_supports_focus -->
                            <div 
                                style="font-size: 12px;"
                                role="grid"
                                aria-label="지연 시간 카운트"
                                tabindex="0"
                                onkeydown={handleCountsKeyDown}
                                class="grid-container"
                            >
                                <Grid bind:data={grid_data} bind:columns={grid_columns}/>
                            </div>
                        </Willow>
                    </ContextMenu.Trigger>
                    <ContextMenu.Content class="w-48">
                        <ContextMenu.Item onclick={downloadCountsCSV}>
                            <Download class="mr-2 h-4 w-4" />
                            <span>CSV로 다운로드</span>
                        </ContextMenu.Item>
                        <ContextMenu.Separator />
                        <ContextMenu.Item>
                            <span>행: {grid_data.length}개</span>
                        </ContextMenu.Item>
                        <ContextMenu.Item onclick={() => handleCountsKeyDown({ctrlKey: true, key: 'a', preventDefault: () => {}})}>
                            <span>모든 데이터 복사 (Ctrl+A)</span>
                        </ContextMenu.Item>
                    </ContextMenu.Content>
                </ContextMenu.Root>
            </div>
        {/if}
    {/if}
</div>

<style>
    .alert {
        padding: 0.75rem 1rem;
        border-radius: 0.375rem;
    }
    .alert-warning {
        background-color: #fff3cd;
        color: #856404;
        border: 1px solid #ffeeba;
    }
    
    /* 그리드 포커스 스타일 */
    .grid-container {
        outline: none;
    }
    
    .grid-container:focus {
        outline: 1px solid rgba(66, 153, 225, 0.5);
    }
</style>