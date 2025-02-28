<script lang="ts">
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import TabContent from './TabContent.svelte';
    
    interface LatencyTabsProps {
        traceType: string;
        filteredData: any[];
        legendKey: string;
        thresholds: string[];
        dtocStat: any;
        ctodStat: any;
        ctocStat: any;
    }

    let { 
        traceType = '', 
        filteredData = [], 
        legendKey = 'opcode', 
        thresholds = [], 
        dtocStat = {}, 
        ctodStat = {}, 
        ctocStat = {} 
    }: LatencyTabsProps = $props();
</script>

<Tabs.Root value="dtoc" class="h-full space-y-6">
    <div class="space-between flex items-center">
        <Tabs.List>
            <Tabs.Trigger value="dtoc">DtoC</Tabs.Trigger>
            <Tabs.Trigger value="ctod">CtoD</Tabs.Trigger>
            <Tabs.Trigger value="ctoc">CtoC</Tabs.Trigger>
        </Tabs.List>
    </div>
    <Tabs.Content value="dtoc" class="border-none p-0 outline-none">
        <TabContent 
            data={filteredData} 
            traceType={traceType}
            columnType="dtoc" 
            {legendKey}
            threshold={thresholds}
            statData={dtocStat}
        />
    </Tabs.Content>
    <Tabs.Content value="ctod" class="border-none p-0 outline-none">
        <TabContent 
            data={filteredData} 
            traceType={traceType}
            columnType="ctod" 
            {legendKey}
            threshold={thresholds}
            statData={ctodStat}
        />
    </Tabs.Content>
    <Tabs.Content value="ctoc" class="border-none p-0 outline-none">
        <TabContent 
            data={filteredData} 
            traceType={traceType}
            columnType="ctoc" 
            {legendKey}
            threshold={thresholds}
            statData={ctocStat}
        />
    </Tabs.Content>
</Tabs.Root>
