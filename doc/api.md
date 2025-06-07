# ESTrace API 참조

이 문서는 ESTrace의 API를 설명합니다. API를 통해 ESTrace의 기능을 프로그래밍 방식으로 활용하거나 외부 도구와 통합할 수 있습니다.

## API 구조

ESTrace API는 다음과 같은 구성 요소로 이루어져 있습니다:

1. **Rust 백엔드 API**: 성능 데이터 수집 및 처리
2. **JavaScript 프론트엔드 API**: UI 및 데이터 시각화
3. **명령줄 인터페이스 (CLI)**: 자동화 및 배치 처리

## Rust 백엔드 API

ESTrace의 코어 기능은 Rust로 작성되어 있으며, Tauri를 통해 JavaScript 프론트엔드와 통신합니다.

### 트레이스 모듈 (`trace` 모듈)

트레이스 데이터를 수집하고 처리하는 API입니다.

#### `collect_trace`

지정된 유형의 트레이스 데이터를 수집합니다.

```rust
#[tauri::command]
pub fn collect_trace(
    trace_type: String,
    duration: u32,
    device_id: Option<String>
) -> Result<TraceResult, String>
```

**매개변수:**
- `trace_type`: 수집할 트레이스 유형 (`"block"`, `"ufs"`, `"cpu"`, `"memory"`)
- `duration`: 트레이스 수집 기간(초)
- `device_id`: (선택 사항) 대상 디바이스 ID

**반환값:**
- 성공 시: `TraceResult` 객체
- 실패 시: 오류 메시지

#### `analyze_trace`

수집된 트레이스 데이터를 분석합니다.

```rust
#[tauri::command]
pub fn analyze_trace(
    trace_data: TraceData,
    analysis_type: String,
    filters: Option<HashMap<String, String>>
) -> Result<AnalysisResult, String>
```

**매개변수:**
- `trace_data`: 분석할 트레이스 데이터
- `analysis_type`: 분석 유형 (`"latency"`, `"io_pattern"`, `"size"`)
- `filters`: (선택 사항) 필터링 옵션

**반환값:**
- 성공 시: `AnalysisResult` 객체
- 실패 시: 오류 메시지

### 필터 모듈 (`filter` 모듈)

데이터 필터링 및 처리를 위한 API입니다.

#### `apply_filter`

트레이스 데이터에 필터를 적용합니다.

```rust
#[tauri::command]
pub fn apply_filter(
    trace_data: TraceData,
    filter_rules: Vec<FilterRule>
) -> Result<TraceData, String>
```

**매개변수:**
- `trace_data`: 필터링할 트레이스 데이터
- `filter_rules`: 적용할 필터 규칙 목록

**반환값:**
- 성공 시: 필터링된 `TraceData` 객체
- 실패 시: 오류 메시지

### 내보내기 모듈 (`export` 모듈)

데이터 내보내기 기능을 제공하는 API입니다.

#### `export_data`

데이터를 지정된 형식으로 내보냅니다.

```rust
#[tauri::command]
pub fn export_data(
    data: ExportableData,
    format: String,
    path: String
) -> Result<(), String>
```

**매개변수:**
- `data`: 내보낼 데이터
- `format`: 내보내기 형식 (`"csv"`, `"json"`, `"html"`, `"pdf"`)
- `path`: 저장 경로

**반환값:**
- 성공 시: 빈 결과
- 실패 시: 오류 메시지

### 패턴 모듈 (`patterns` 모듈)

I/O 패턴 관리 및 테스트를 위한 API입니다.

#### `detect_patterns`

트레이스 데이터에서 패턴을 감지합니다.

```rust
#[tauri::command]
pub fn detect_patterns(
    trace_data: TraceData,
    sensitivity: u8
) -> Result<Vec<Pattern>, String>
```

**매개변수:**
- `trace_data`: 패턴을 감지할 트레이스 데이터
- `sensitivity`: 감지 민감도 (1-10)

**반환값:**
- 성공 시: 감지된 `Pattern` 객체 목록
- 실패 시: 오류 메시지

## JavaScript 프론트엔드 API

프론트엔드 컴포넌트와 시각화 기능을 제공하는 API입니다.

### `TraceDataStore` 클래스

트레이스 데이터 관리를 위한 Svelte 스토어입니다.

```typescript
import { writable } from 'svelte/store';

export class TraceDataStore {
  // 트레이스 데이터 로드
  public async loadTraceData(filePath: string): Promise<void> {
    // 구현...
  }

  // 트레이스 데이터 저장
  public async saveTraceData(filePath: string): Promise<void> {
    // 구현...
  }

  // 데이터 필터링
  public filterData(filterFn: (item: TraceItem) => boolean): void {
    // 구현...
  }

  // 구독 메소드
  public subscribe(callback: (data: TraceData) => void): Unsubscriber {
    // 구현...
  }
}
```

### 차트 API

데이터 시각화를 위한 래퍼 API입니다.

```typescript
// ECharts 래퍼
export function createScatterChart(
  element: HTMLElement,
  data: ChartData,
  options?: ScatterChartOptions
): EChartsInstance {
  // 구현...
}

// Plotly 래퍼
export function createHeatmapChart(
  element: HTMLElement,
  data: HeatmapData,
  options?: HeatmapOptions
): Plotly.PlotlyHTMLElement {
  // 구현...
}
```

## 명령줄 인터페이스 (CLI)

ESTrace를 명령줄에서 실행하고 제어하기 위한 인터페이스입니다.

```bash
# 트레이스 수집
estrace collect --type block --duration 30 --device emulator-5554 --output trace.json

# 데이터 분석
estrace analyze --input trace.json --type latency --filter "size>1024" --output analysis.json

# 보고서 생성
estrace report --input analysis.json --format html --output report.html

# 배치 작업 실행
estrace batch --script batch_config.yml
```

## API 사용 예제

### Rust에서 ESTrace 기능 사용

```rust
use estrace_lib::trace::{collect_trace, TraceType};
use estrace_lib::filter::{apply_filter, FilterRule, FilterOperator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 트레이스 수집
    let trace_result = collect_trace(
        TraceType::Block,
        30,
        Some("emulator-5554".to_string())
    )?;
    
    // 필터 적용
    let filter_rules = vec![
        FilterRule {
            field: "size".to_string(),
            operator: FilterOperator::GreaterThan,
            value: "1024".to_string(),
        }
    ];
    
    let filtered_data = apply_filter(trace_result.data, filter_rules)?;
    
    // 결과 처리
    println!("Collected {} trace events", filtered_data.events.len());
    
    Ok(())
}
```

### JavaScript에서 ESTrace 기능 사용

```typescript
import { invoke } from '@tauri-apps/api/tauri';
import { TraceDataStore } from './stores/trace';

async function collectAndAnalyzeTrace() {
  try {
    // 트레이스 수집
    const traceResult = await invoke('collect_trace', {
      traceType: 'block',
      duration: 30,
      deviceId: 'emulator-5554'
    });
    
    // 트레이스 데이터 저장
    const traceStore = new TraceDataStore();
    traceStore.setData(traceResult);
    
    // 분석 수행
    const analysis = await invoke('analyze_trace', {
      traceData: traceResult,
      analysisType: 'latency',
      filters: { size: '>1024' }
    });
    
    // 결과 처리
    console.log('Analysis complete:', analysis);
    
    // 차트 생성
    createScatterChart(
      document.getElementById('chart-container'),
      analysis.chartData
    );
  } catch (error) {
    console.error('Error:', error);
  }
}
```

## API 버전 관리

ESTrace API는 시맨틱 버저닝을 따릅니다. 주요 버전 변경은 하위 호환성이 깨질 수 있으므로 주의해서 업그레이드해야 합니다.

현재 API 버전: v1.0

## API 제한 사항

- 일부 트레이스 기능은 루트 권한이 필요할 수 있습니다.
- 대용량 데이터 처리 시 메모리 사용량에 주의하세요.
- 네이티브 API는 지원되는 플랫폼(Windows, macOS, Linux)에서만 사용할 수 있습니다.

## 개발자 도구

ESTrace는 개발자를 위한 다양한 도구를 제공합니다:

- **개발자 콘솔**: 로그 및 디버그 정보 확인
- **API 탐색기**: 사용 가능한 API 기능 탐색
- **성능 프로파일러**: API 호출 성능 모니터링

자세한 개발 정보는 [기여 가이드](./contributing.md)를 참조하세요.
