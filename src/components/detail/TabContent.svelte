<script lang="ts">
    import * as Card from '$lib/components/ui/card/index.js';
    import { ScatterChartsDeck } from '$components/detail';
    import { LatencyStats } from '$components/detail';
    
    interface TabContentProps {
        data?: any[];      // 호환성용
        table?: any;       // Arrow Table 객체
        traceType: string;
        columnType: string;
        legendKey: string;
        threshold: string[];
        statData: any;
    }

    let { 
        data = [], 
        table = null,
        traceType = '', 
        columnType = '', 
        legendKey = 'opcode', 
        threshold = [], 
        statData = {}
     } : TabContentProps = $props();

</script>

<Card.Root>
    <Card.Content>
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Pattern</div>
        <ScatterChartsDeck 
            data={data}
            table={table}
            xAxisKey='time' 
            yAxisKey={columnType.toLowerCase()} 
            {legendKey} 
            yAxisLabel='ms' 
            ycolumn={columnType.toLowerCase()}
            actionFilter='send_req'
        /> 
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Statistics</div>
        <LatencyStats 
            tracetype={traceType} 
            {threshold} 
            latencystat={statData} 
        /> 
    </Card.Content>
</Card.Root>
