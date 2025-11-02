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

    // UFSCUSTOM의 경우 시간 필드를 동적으로 결정
    // - 기본: start_time (dtoc, ctod)
    // - ctoc: end_time
    // - 일반 trace (ufs, block): time
    const timeField = $derived(
        traceType === 'ufscustom' 
            ? (columnType.toLowerCase() === 'ctoc' ? 'end_time' : 'start_time')
            : 'time'
    );

</script>

<Card.Root>
    <Card.Content>
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Pattern</div>
        <div class="h-full" style="height: 50vh; min-height: 400px; display: flex; flex-direction: column;">
            <ScatterChartsDeck 
                data={data}
                table={table}
                xAxisKey={timeField}
                yAxisKey={columnType.toLowerCase()} 
                {legendKey} 
                yAxisLabel='ms' 
                ycolumn={columnType.toLowerCase()}
                actionFilter='send_req'
            /> 
        </div>
        <div class="font-semibold prose lg:prose-h3 pb-4">{columnType} Statistics</div>
        <LatencyStats 
            tracetype={traceType} 
            {threshold} 
            latencystat={statData} 
        /> 
    </Card.Content>
</Card.Root>
