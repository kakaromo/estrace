<script lang="ts">
    import * as Select from "$lib/components/ui/select/index.js";
    import { selectedTrace, filtertrace } from "$stores/trace.js";

    interface SelectTypeProps {
        tracetype: string[];
    }

    let { tracetype } : SelectTypeProps = $props();  
</script>
<Select.Root onSelectedChange={(v) => {
    if (v) {
        selectedTrace.set(v.value);
        // filtertrace 설정은 부모 컴포넌트로 이동
    }
}}>
<Select.Trigger class="w-[240px]" h-12>
    <Select.Value placeholder="Select a type(UFS, Block)" />
</Select.Trigger>
<Select.Content>
    <Select.Group>
    <Select.Label>Type</Select.Label>
    {#each tracetype as value}
    <Select.Item value={value} label={value}>{value}</Select.Item>
    {/each}             
    </Select.Group>
</Select.Content>
<Select.Input name="selectTraceType" />
</Select.Root>