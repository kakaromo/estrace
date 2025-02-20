<script lang='ts'>
    import { Grid } from "wx-svelte-grid";
    import { Willow, Material } from "wx-svelte-grid";
    import * as Tabs from "$lib/components/ui/tabs/index.js";
    import { Root } from "$lib/components/ui/dialog";
    import { helper } from "echarts";

    // props로부터 전달받은 값
    let { tracetype, latencystat, threshold } = $props();

    // 기존 latency_counts: { [latency_type: string]: { [threshold: string]: number } }
    let latency_counts = latencystat.latency_counts;
    let latency_summary = latencystat.summary;
    console.log('latency_summary:', latency_summary);
    
    // 결과를 위한 배열들
    let latency_threshold: string[] = [];
    let latency_type_key: string[] = [];
    let grid_columns: Array<{ id: string, header: string, width: number }> = [];
    let grid_data: any[] = [];

    // threshold 문자열 배열 생성 (예: [ "≤ 0.1ms", "0.1ms < v ≤ 0.5ms", "> 1000s" ])
    function thresholdValue() {
        if (threshold) {
            for (let i = 0; i < threshold.length; i++) {
                let value: string = '';
                if (i === 0) {         
                    value = "≤ " + threshold[i];       
                } else if (i === threshold.length - 1) {
                    value = "> " + threshold[i];
                } else {
                    value = threshold[i - 1] + " < v ≤ " + threshold[i];
                }
                latency_threshold.push(value);
            }
        }
    }  

    // latency_type_key 추출 및 내림차순 정렬
    function latencyTypeKey() {
        Object.keys(latency_counts).forEach((key) => {
            latency_type_key.push(key);
        });
        latency_type_key.sort().reverse();
    }

    // 행과 열을 전치하여 Grid용 데이터를 생성:
    // - grid의 첫 컬럼은 "Range" (threshold 값)
    // - 나머지 컬럼은 각 latency_type_key (opcode)
    // - 각 행은 하나의 threshold에 해당하며, 각 셀은 해당 latency_type의 count
    function buildTransposedGridData() {
        // grid_columns: 첫 번째 컬럼은 Range, 이후 latency_type_key마다 컬럼 추가
        grid_columns = [];
        grid_columns.push({ id: "range", header: "Range", width: 150 });
        latency_type_key.forEach(typeKey => {
            grid_columns.push({ id: typeKey, header: typeKey, width: 150 });
        });
        // grid_data: 각 행은 하나의 threshold (range)
        grid_data = latency_threshold.map(thresh => {
            let row: any = { range: thresh };
            latency_type_key.forEach(typeKey => {
                // latency_counts[typeKey]는 해당 typeKey의 threshold별 값 객체
                // 값이 없으면 0을 할당
                row[typeKey] = latency_counts[typeKey] ? latency_counts[typeKey][thresh] || 0 : 0;
            });
            return row;
        });
    }

    let grid_columns_summary: Array<{ id: string, header: string, width: number }> = [];
    let grid_data_summary: any[] = [];

    function buildSummaryGridData() {
        // 기본 컬럼 정의
        const baseColumns = [
            { id: "type", header: "Type", width: 150 },
            { id: "avg", header: "Avg", width: 150 },
            { id: "min", header: "Min", width: 150 },
            { id: "median", header: "Median", width: 150 },
            { id: "max", header: "Max", width: 150 },
            { id: "std_dev", header: "Std Dev", width: 150 },
            { id: "sum", header: "Sum", width: 150 }
        ];

        // 각 summary 객체의 percentiles 내부 키들을 모두 모아서 union을 만듭니다.
        let percentileKeys: string[] = [];
        Object.values(latency_summary).forEach(summary => {
            if (summary.percentiles) {
                Object.keys(summary.percentiles).forEach(key => {
                    if (!percentileKeys.includes(key)) {
                        percentileKeys.push(key);
                    }
                });
            }
        });
        // 예제에서는 단순 알파벳 순으로 정렬합니다.
        percentileKeys.sort().reverse();

        // grid 컬럼 생성: 기본 컬럼 + percentile 컬럼
        grid_columns_summary = [...baseColumns];
        percentileKeys.forEach(pk => {
            grid_columns_summary.push({ id: pk, header: pk, width: 150 });
        });

        // grid 데이터 생성: 각 행은 하나의 타입(summary의 key)을 나타냄
        grid_data_summary = Object.keys(latency_summary).map(typeKey => {
            const summary = latency_summary[typeKey];
            let row: any = {
                type: typeKey,
                avg: summary.avg || 0,
                min: summary.min || 0,
                median: summary.median || 0,
                max: summary.max || 0,
                std_dev: summary.std_dev || 0,
                sum: summary.sum || 0
            };
            percentileKeys.forEach(pk => {
                row[pk] = summary.percentiles ? summary.percentiles[pk] || 0 : 0;
            });
            return row;
        });
    }

    // 함수 호출 순서: type, threshold, 전치 데이터 생성
    latencyTypeKey();
    thresholdValue();        
    buildTransposedGridData();

    buildSummaryGridData();

    console.log("grid_columns_summary:", grid_columns_summary);
    console.log("grid_data_summary:", grid_data_summary);

    console.log('latency_threshold:', latency_threshold);
    console.log('latency_type_key:', latency_type_key);
    console.log('grid_columns:', grid_columns);
    console.log('grid_data:', grid_data);
</script>

<div class="font-sans">
    <Willow>
        <div class="px-0" style="font-size: 11px;">
            <Grid data={grid_data} columns={grid_columns}/>
        </div>
    </Willow>
    <div class="divider"></div>
    <Willow>
        <div class="px-0" style="font-size: 11px;">
            <Grid data={grid_data_summary} columns={grid_columns_summary}/>
        </div>
    </Willow>
</div>

<style>
    .font-sans {
        font-size: 11px;
    }
</style>