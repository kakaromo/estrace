<script lang='ts'>    
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";

    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import { Root } from "$lib/components/ui/dialog";
    import { helper } from "echarts";

    let { tracetype, latencystat, threshold } = $props();

    let columns = [];
    let latency_counts = latencystat.latency_counts;
    let latency_summary = latencystat.summary;
    let latency_threshold = $state<String[]>([]);
    let latency_type_key = $state<string[]>([]);
    let latency_threshold_columns = $state<Array<{id: string, width: number}>>([]);
    let latency_counts_grid = $state([]);

    function latencyTypeKey() {
        Object.keys(latency_counts).forEach((key) => {
            latency_type_key.push(key);
        });
        latency_type_key.sort().reverse();
    }

    function thresholdValue() {
        if (threshold) {
            for (let i = 0; i < threshold.length; i++) {
                let value:String = '';
                if (i === 0) {         
                    value = "≤ " + threshold[i];       
                    latency_threshold.push(value);
                } else if (i === threshold.length - 1) {
                    value = "> " + threshold[i];
                    latency_threshold.push(value);
                } else {
                    value = threshold[i - 1] + " < v ≤ " + threshold[i];
                    latency_threshold.push(value);
                }
            }
        }
    }  

    function latencyThresholdColumn() {
        for (let i = 0; i < latency_threshold.length; i++) {
            const key = latency_threshold[i] as string;
            let column = {
                id: key,
                header: key,
                width: 150
            };
            latency_threshold_columns.push(column);
        }
    }

    function test() {
        console.log("test")
    }

    function latencyCountGird() {
        for (let i = 0; i < latency_type_key.length; i++) {
            const key:string = latency_type_key[i];
            latency_counts_grid[key] = [];
            let data = {};
            for (let j = 0; j < latency_threshold.length; j++) {
                const subKey:String = latency_threshold[j];
                data[subKey] = latency_counts[key][subKey];
                
            }
            latency_counts_grid[key].push(data);      
        }
    }

    latencyTypeKey();
    
    thresholdValue();        
    latencyThresholdColumn();
    latencyCountGird();
    
    console.log('latency_type_key:', latency_type_key);    
    console.log('latency_threshold:', latency_threshold);
    console.log('latency_threshold_column:', latency_threshold_columns);
    console.log('latency_counts_grid:', latency_counts_grid);

      
</script>
<div class="font-sans tabs">
<Tabs.Root value={latency_type_key[0]}  class="w-full">
    <Tabs.List class="grid w-[{latency_type_key.length *100}px] grid-cols-{latency_type_key.length} gap-1.5">
        {#each latency_type_key as key}
        <Tabs.Trigger value={key}>{key}</Tabs.Trigger>
        {/each}    
    </Tabs.List>
    {#each latency_type_key as key}
        <Tabs.Content value={key}>
            <Willow>
            <Grid data={latency_counts_grid[key]} columns={latency_threshold_columns}/>
            </Willow>
        </Tabs.Content>
    {/each}
</Tabs.Root>
</div>

<style>
    .tabs {
        font-size: 11px;
    }
</style>