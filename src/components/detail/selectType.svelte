<script lang="ts">
    import * as Select from "$lib/components/ui/select/index.js";
    import { selectedTrace } from "../../stores/trace.js";

    let { tracedata, tracetype } = $props();       
    
</script>
<Select.Root onSelectedChange={(v) => {
    v && (selectedTrace.set(v.value));
    console.log('selectedTrace:', $selectedTrace);
  }}>
<Select.Trigger class="w-[240px]" h-12>
    <Select.Value placeholder="Select a type(UFS, Block)" />
</Select.Trigger>
<Select.Content>
    <Select.Group>
    <Select.Label>Type</Select.Label>
    {#each tracetype as value}
        {#if tracedata[value].length > 0}
        <Select.Item value={value} label={value}>{value}</Select.Item>
        {/if}
    {/each}             
    </Select.Group>
</Select.Content>
<Select.Input name="selectTraceType" />
</Select.Root>