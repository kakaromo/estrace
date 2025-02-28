<script lang="ts">
    import * as Card from '$lib/components/ui/card/index.js';
    import { ScatterCharts } from '$components/detail';
    import { LatencyStats } from '$components/detail';
    
    export let data: any[] = [];
    export let traceType: string = '';
    export let columnType: string = '';
    export let legendKey: string = 'opcode';
    export let threshold: string[] = [];
    export let statData: any = {};
</script>

<Card.Root>
    <Card.Content>
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Pattern</div>
        <ScatterCharts 
            data={data} 
            xAxisKey='time' 
            yAxisKey={columnType.toLowerCase()} 
            {legendKey} 
            yAxisLabel='ms' 
            ycolumn={columnType.toLowerCase()}
        /> 
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Statistics</div>
        <LatencyStats 
            tracetype={traceType} 
            {threshold} 
            latencystat={statData} 
        /> 
    </Card.Content>
</Card.Root>
