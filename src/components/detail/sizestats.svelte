<script lang="ts">
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";
    import * as Tabs from "$lib/components/ui/tabs/index.js";

    interface SizeStatsProps {
        opcode_size_counts: Record<string, Record<string, number>>;
    }

    let { opcode_size_counts }:SizeStatsProps = $props();

    // 상태 변수 선언
    let normalized_size_counts = $state<Record<string, { size: number, count: number }[]>>({});
    let prevOpcodeSizeCounts = $state(null);

    // opcode_size_counts가 변경될 때마다 데이터 재처리
    $effect(() => {
        if (opcode_size_counts && 
            JSON.stringify(opcode_size_counts) !== JSON.stringify(prevOpcodeSizeCounts)) {
            console.log('opcode_size_counts changed:', opcode_size_counts);
            
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
        }
    });
</script>


<div role="tablist" class="tabs tabs-lifted">
    {#each Object.keys(normalized_size_counts) as opcode, i}           
        <input type="radio" name="sizecounts" role="tab" class="tab" aria-label="{opcode}" checked={i===0?'checked':''}/>
        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
            <Willow>
                <div class="px-0" style="font-size: 12px;">
                    <Grid 
                        data={normalized_size_counts[opcode]} 
                        columns={[
                            { id: "size", header: "Size", width: 150 },
                            { id: "count", header: "Count", width: 150 }
                        ]}/>
                </div>
            </Willow>
        </div>
    {/each}
  </div>

<style>
    /* 추가 스타일은 필요에 따라 조정 */
</style>