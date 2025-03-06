<script lang="ts">
    import * as Select from "$lib/components/ui/select/index.js";
    import { selectedTrace, filtertrace } from "$stores/trace.js";

    interface SelectTypeProps {
        tracedata: any;
        tracetype: string[];
    }

    let { tracedata, tracetype } : SelectTypeProps = $props();       

    function initfiltertrace() {
       
    }
    
</script>
<Select.Root onSelectedChange={(v) => {
     $filtertrace = {
            zoom_column: $selectedTrace === 'ufs'? 'lba': 'sector',
            from_time: 0.0,
            to_time: 0.0,
            from_lba: 0.0,
            to_lba: 0.0,
        };
    v && (selectedTrace.set(v.value));
  }}>
<Select.Trigger class="w-[240px]" h-12>
    <Select.Value placeholder="Select a type(UFS, Block)" />
</Select.Trigger>
<Select.Content>
    <Select.Group>
    <Select.Label>Type</Select.Label>
    {#each tracetype as value}
        {#if tracedata[value].total_count > 0}
        <Select.Item value={value} label={value}>{value}</Select.Item>
        {/if}
    {/each}             
    </Select.Group>
</Select.Content>
<Select.Input name="selectTraceType" />
</Select.Root>