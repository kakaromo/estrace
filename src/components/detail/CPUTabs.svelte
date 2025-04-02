<script lang="ts">
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import { ScatterCharts } from '$components/detail';
    
    interface QDTabsProps {
        traceType: string;
        data: any[];
        legendKey: string;
    }

    let { 
        data = [],
        traceType = '', 
        legendKey = 'cpu', 
    }: QDTabsProps = $props();
</script>

<Tabs.Root value="cpu" class="h-full space-y-6">
    <div class="space-between flex items-center">
        <Tabs.List>
            <Tabs.Trigger value="cpu">CPU</Tabs.Trigger>
            <Tabs.Trigger value="addr">ADDRESS</Tabs.Trigger>
        </Tabs.List>
    </div>
    <Tabs.Content value="cpu" class="border-none p-0 outline-none">
        <ScatterCharts data={data} xAxisKey='time' yAxisKey='cpu' legendKey='cpu' yAxisLabel='cpu' ycolumn='cpu'/>
    </Tabs.Content>
    <Tabs.Content value="addr" class="border-none p-0 outline-none">
        {#if traceType === 'ufs'} 
        <ScatterCharts data={data} xAxisKey='time' yAxisKey='lba' legendKey='cpu' yAxisLabel='4KB' ycolumn='lba'/>
        {:else if traceType === 'block'}
        <ScatterCharts data={data} xAxisKey='time' yAxisKey='sector' legendKey='cpu' yAxisLabel='sector' ycolumn='sector'/>
        {/if}
    </Tabs.Content>
</Tabs.Root>
