<script lang="ts">
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
    import { Download } from "svelte-lucide";

    import { toast } from "svelte-sonner";
    import { Toaster } from "$lib/components/ui/sonner";

    // Tauri v2 API 가져오기
    import { save } from '@tauri-apps/plugin-dialog';
    import { writeTextFile } from '@tauri-apps/plugin-fs';
    import { writeText } from '@tauri-apps/plugin-clipboard-manager';

    interface SizeStatsProps {
        opcode_size_counts: Record<string, Record<string, number>>;
    }

    let { opcode_size_counts }:SizeStatsProps = $props();

    // 상태 변수 선언
    let normalized_size_counts = $state<Record<string, { size: number, count: number }[]>>({});
    let prevOpcodeSizeCounts = $state(null);

    // 컨텍스트 메뉴 상태
    let contextMenuOpen = $state(false);
    let currentOpcode = $state('');
    
    // 현재 선택된 탭의 opcode 추적
    let selectedOpcode = $state('');

    // opcode_size_counts가 변경될 때마다 데이터 재처리
    $effect(() => {
        if (opcode_size_counts && 
            JSON.stringify(opcode_size_counts) !== JSON.stringify(prevOpcodeSizeCounts)) {
            
            // 현재 값을 이전 값으로 저장
            prevOpcodeSizeCounts = JSON.parse(JSON.stringify(opcode_size_counts));
            
            // 데이터 재처리
            normalized_size_counts = {};
            Object.keys(opcode_size_counts).forEach(opcode => {
                const sizesObj = opcode_size_counts[opcode];
                let arr: { size: number, count: number }[] = [];
                Object.keys(sizesObj).forEach(key => {
                    arr.push({ size: parseFloat(key), count: sizesObj[key] });
                });
                arr.sort((a, b) => a.size - b.size);
                normalized_size_counts[opcode] = arr;
            });
            
            // 첫 번째 opcode를 기본 선택
            if (Object.keys(normalized_size_counts).length > 0) {
                selectedOpcode = Object.keys(normalized_size_counts)[0];
            }
        }
    });

    // CSV 다운로드 함수
    async function downloadCSV(opcode: string) {
        const data = normalized_size_counts[opcode];
        if (!data || data.length === 0) return;
        
        // CSV 헤더 생성
        let csvContent = "Size,Count\n";
        
        // 데이터 행 추가
        data.forEach(row => {
            csvContent += `${row.size},${row.count}\n`;
        });
        
         // 파일 저장 다이얼로그 표시
         const filePath = await save({
            filters: [{
                name: 'CSV',
                extensions: ['csv']
            }],
            defaultPath: `${opcode}_size_stats.csv`
        });
        
        if (filePath) {
            // 파일에 CSV 내용 쓰기
            await writeTextFile(filePath, csvContent);
            console.log(`CSV exported to ${filePath}`);
        }
    }
    
    // 컨텍스트 메뉴를 표시할 opcode 설정
    function handleContextMenu(e: MouseEvent & { currentTarget: EventTarget & HTMLDivElement; } ,opcode: string) {
        e.preventDefault();
        currentOpcode = opcode;
    }
    
    // 탭 변경 시 선택된 opcode 업데이트
    function handleTabChange(opcode: string) {
        selectedOpcode = opcode;
    }
    
    // Ctrl+A 키 이벤트 처리 및 클립보드에 복사
    async function handleKeyDown(event: KeyboardEvent, opcode: string) {
        // Ctrl+A 감지 (Mac에서는 Command+A)
        if ((event.ctrlKey || event.metaKey) && event.key === 'a') {
            event.preventDefault();
            
            const data = normalized_size_counts[opcode];
            if (!data || data.length === 0) return;
            
            // 헤더 포함 탭으로 구분된 텍스트 생성 (엑셀에 붙여넣기 용)
            let clipboardText = "Size\tCount\n";
            
            // 데이터 행 추가
            data.forEach(row => {
                clipboardText += `${row.size}\t${row.count}\n`;
            });

            console.log("클립보드에 복사할 데이터:", clipboardText);
            
            // 클립보드에 복사
            try {
                await writeText(clipboardText);
                toast.success("데이터가 클립보드에 복사되었습니다.", {
                    description: "엑셀에 붙여넣기 가능합니다.",
                    duration: 2000,
                });
                console.log("데이터가 클립보드에 복사되었습니다.");
            } catch (error) {
                console.error("클립보드 복사 실패:", error);
            }
        }
    }
</script>
<Toaster/>

<ContextMenu.Root bind:open={contextMenuOpen}>
    <div role="tablist" class="tabs tabs-lifted">
    {#each Object.keys(normalized_size_counts) as opcode, i}           
        <input 
            type="radio" 
            name="sizecounts" 
            role="tab" 
            class="tab" 
            aria-label="{opcode}" 
            checked={i===0} 
            onchange={() => handleTabChange(opcode)}
        />
        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
            <Willow>
                <ContextMenu.Trigger>
                    <!-- svelte-ignore a11y_interactive_supports_focus -->
                    <div 
                        class="px-0" 
                        style="font-size: 12px;" 
                        role="grid" 
                        aria-label="{opcode} 크기 통계" 
                        oncontextmenu={(e) => handleContextMenu(e, opcode)}
                        onkeydown={(e) => handleKeyDown(e, opcode)}
                        tabindex="0"
                    >
                        <Grid 
                            bind:data={normalized_size_counts[opcode]} 
                            columns={[
                                { id: "size", header: "Size", width: 150 },
                                { id: "count", header: "Count", width: 150 }
                            ]}
                        />
                    </div>
                </ContextMenu.Trigger>
            </Willow>
        </div>
    {/each}
    </div>
    <ContextMenu.Content class="w-48">
        <ContextMenu.Item onclick={() => downloadCSV(currentOpcode)}>
            <Download class="mr-2 h-4 w-4" />
            <span>CSV로 다운로드</span>
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item>
            <span>행: {normalized_size_counts[currentOpcode]?.length || 0}개</span>
        </ContextMenu.Item>
        <ContextMenu.Item onclick={() => handleKeyDown({ctrlKey: true, key: 'a', preventDefault: () => {}}, currentOpcode)}>
            <span>모든 데이터 복사 (Ctrl+A)</span>
        </ContextMenu.Item>
    </ContextMenu.Content>
</ContextMenu.Root>

<style>
    /* 그리드가 포커스를 받을 수 있도록 스타일 추가 */
    div[role="grid"] {
        outline: none;
    }
    
    div[role="grid"]:focus {
        outline: 1px solid rgba(66, 153, 225, 0.5);
    }
</style>