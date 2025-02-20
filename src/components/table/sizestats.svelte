<script lang="ts">
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";
    import * as Tabs from "$lib/components/ui/tabs/index.js";

    // 예시 props 구조:
    // {
    //   "0x2a": {1: 37876, 2: 3422, 3: 706, ...},
    //   "0x28": {1: 69776, 2: 217, 3: 17, ...},
    //   "0x42": {1: 1, 160: 2, 161: 1, ...}
    // }
    let { opcode_size_counts } = $props();

    // 각 opcode별로 size, count 데이터를 배열 형태로 변환합니다.
    let normalized_size_counts: Record<string, { size: number, count: number }[]> = {};

    Object.keys(opcode_size_counts).forEach(opcode => {
        const sizesObj = opcode_size_counts[opcode];
        let arr: { size: number, count: number }[] = [];
        Object.keys(sizesObj).forEach(key => {
            arr.push({ size: parseFloat(key), count: sizesObj[key] });
        });
        // size 기준 오름차순 정렬 (필요에 따라 내림차순으로 변경 가능)
        arr.sort((a, b) => a.size - b.size);
        normalized_size_counts[opcode] = arr;
    });
</script>


<div role="tablist" class="tabs tabs-lifted">
    {#each Object.keys(normalized_size_counts) as opcode, i}           
        <input type="radio" name="my_tabs_1" role="tab" class="tab" aria-label="{opcode}" checked={i===0?'checked':''}/>
        <div role="tabpanel" class="tab-content bg-base-100 border-base-300 rounded-box p-6">
            <Willow>
                <div class="px-0" style="font-size: 11px;">
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