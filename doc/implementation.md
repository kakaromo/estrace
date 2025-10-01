# ESTrace 구현 문서

## 1. 기술 스택 및 개발 환경

### 1.1 핵심 기술 스택

#### Frontend
- **SvelteKit**: 모던 웹 프레임워크
- **TypeScript**: 타입 안전성과 개발 생산성
- **Tailwind CSS**: 유틸리티 퍼스트 CSS 프레임워크
- **Vite**: 빠른 빌드 도구

#### Backend
- **Rust**: 시스템 프로그래밍 언어 (성능 및 안전성)
- **Tauri**: 웹 기반 데스크톱 애플리케이션 프레임워크
- **SQLx**: 타입 안전한 SQL 쿼리 빌더
- **Tokio**: 비동기 런타임

#### 데이터 처리
- **Apache Arrow**: 고성능 컬럼형 데이터 처리
- **DataFusion**: SQL 쿼리 엔진
- **Parquet**: 컬럼형 스토리지 포맷
- **Rayon**: 병렬 처리 라이브러리

#### 데이터 시각화
- **ECharts**: 인터랙티브 차트 라이브러리
- **Plotly.js**: 과학적 시각화 라이브러리

#### 데이터베이스
- **SQLite**: 로컬 데이터베이스
- **IDB-KeyVal**: 브라우저 스토리지

### 1.2 개발 도구

```json
{
  "devDependencies": {
    "@sveltejs/kit": "^2.9.0",
    "@sveltejs/adapter-static": "^3.0.6",
    "@tauri-apps/cli": "^2",
    "autoprefixer": "^10.4.20",
    "postcss": "^8.4.45",
    "tailwindcss": "^3.4.15",
    "typescript": "^5.7.2",
    "vite": "^6.0.1"
  }
}
```

```toml
[dependencies]
tauri = { version = "2.6", features = [] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
sqlx = { version = "0.8.3", features = ["runtime-tokio", "sqlite", "macros", "chrono"] }
arrow = "54.1.0"
parquet = "54.1.0"
datafusion = "45.0.0"
rayon = "1.10.0"
regex = "1.11.1"
chrono = "0.4.39"
```

## 2. 프로젝트 구조 및 모듈 구현

### 2.1 Frontend 구현

#### 2.1.1 라우팅 시스템
```typescript
// src/routes/+layout.ts
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async () => {
  return {
    // 글로벌 설정 로드
  };
};
```

#### 2.1.2 상태 관리 (Svelte Stores)
```typescript
// src/stores/trace.ts
import { writable } from 'svelte/store';

export interface TraceData {
  id: string;
  type: 'block' | 'ufs';
  filename: string;
  data: any[];
  stats: TraceStats;
}

export const traceStore = writable<TraceData[]>([]);
export const selectedTrace = writable<TraceData | null>(null);
```

#### 2.1.3 API 통신 모듈
```typescript
// src/api/db.ts
import Database from '@tauri-apps/plugin-sql';

let db: Database = null;

export async function initial() {
    await open();
    await createTables();
}

async function createTables() {
    await db.execute(`
        CREATE TABLE IF NOT EXISTS testinfo (
            id INTEGER PRIMARY KEY,
            logtype TEXT,
            title TEXT,
            content TEXT,
            logfolder TEXT,
            logname TEXT,
            sourcelog_path TEXT
        )
    `);
    // 기타 테이블 생성...
}
```

### 2.2 Backend 구현

#### 2.2.1 Tauri 명령어 시스템
```rust
// src-tauri/src/lib.rs
use tauri::Manager;

#[tauri::command]
async fn parse_trace_file(file_path: String, trace_type: String) -> Result<Vec<u8>, String> {
    match trace_type.as_str() {
        "block" => trace::block::parse_file(&file_path).await,
        "ufs" => trace::ufs::parse_file(&file_path).await,
        _ => Err("Unsupported trace type".to_string()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            parse_trace_file,
            analyze_trace_data,
            export_trace_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

#### 2.2.2 트레이스 파싱 모듈
```rust
// src-tauri/src/trace/block.rs
use regex::Regex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

static BLOCK_CACHE: Lazy<Mutex<HashMap<String, Vec<Block>>>> = 
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block {
    pub command: String,
    pub pid: i32,
    pub timestamp: f64,
    pub device_id: String,
    pub start_lba: u64,
    pub num_sectors: u32,
    pub latency: f64,
    pub size: u64,
    pub queue_depth: u32,
    pub completion_time: f64,
}

impl Block {
    pub fn from_line(line: &str, pattern: &Regex) -> Option<Self> {
        if let Some(captures) = pattern.captures(line) {
            Some(Block {
                command: captures.get(1)?.as_str().to_string(),
                pid: captures.get(2)?.as_str().parse().ok()?,
                timestamp: captures.get(3)?.as_str().parse().ok()?,
                // ... 기타 필드 파싱
            })
        } else {
            None
        }
    }
}

pub async fn parse_file(file_path: &str) -> Result<Vec<u8>, String> {
    let pattern = get_active_pattern();
    let content = read_file_content(file_path)?;
    
    let blocks: Vec<Block> = content
        .lines()
        .filter_map(|line| Block::from_line(line, &pattern))
        .collect();
    
    // Apache Arrow로 변환
    let arrow_table = convert_to_arrow(&blocks)?;
    let arrow_bytes = serialize_arrow_table(arrow_table)?;
    
    // 캐시에 저장
    cache_blocks(file_path, blocks);
    
    Ok(arrow_bytes)
}
```

#### 2.2.3 데이터 분석 모듈
```rust
// src-tauri/src/trace/utils.rs
use arrow::array::*;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub fn calculate_latency_stats(data: &[Block]) -> LatencyStats {
    let latencies: Vec<f64> = data.iter().map(|b| b.latency).collect();
    
    LatencyStats {
        min: latencies.iter().copied().fold(f64::INFINITY, f64::min),
        max: latencies.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        mean: latencies.iter().sum::<f64>() / latencies.len() as f64,
        median: calculate_median(&latencies),
        p95: calculate_percentile(&latencies, 0.95),
        p99: calculate_percentile(&latencies, 0.99),
    }
}

pub fn convert_to_arrow(blocks: &[Block]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let command_array = StringArray::from(
        blocks.iter().map(|b| b.command.as_str()).collect::<Vec<_>>()
    );
    let timestamp_array = Float64Array::from(
        blocks.iter().map(|b| b.timestamp).collect::<Vec<_>>()
    );
    let latency_array = Float64Array::from(
        blocks.iter().map(|b| b.latency).collect::<Vec<_>>()
    );
    
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("command", arrow::datatypes::DataType::Utf8, false),
        arrow::datatypes::Field::new("timestamp", arrow::datatypes::DataType::Float64, false),
        arrow::datatypes::Field::new("latency", arrow::datatypes::DataType::Float64, false),
    ]));
    
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(command_array),
            Arc::new(timestamp_array),
            Arc::new(latency_array),
        ],
    )
}
```

### 2.3 패턴 관리 시스템

#### 2.3.1 동적 패턴 컴파일
```rust
// src-tauri/src/trace/patterns.rs
use regex::Regex;
use std::collections::HashMap;
use std::sync::RwLock;

static UFS_PATTERNS: Lazy<RwLock<HashMap<String, Regex>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

#[tauri::command]
pub async fn add_pattern(name: String, pattern_str: String) -> Result<(), String> {
    let compiled_pattern = Regex::new(&pattern_str)
        .map_err(|e| format!("Invalid regex: {}", e))?;
    
    // 패턴 테스트
    if !test_pattern(&compiled_pattern) {
        return Err("Pattern validation failed".to_string());
    }
    
    // 패턴 저장
    {
        let mut patterns = UFS_PATTERNS.write().unwrap();
        patterns.insert(name.clone(), compiled_pattern);
    }
    
    // 데이터베이스에 저장
    save_pattern_to_db(&name, &pattern_str).await?;
    
    Ok(())
}

fn test_pattern(pattern: &Regex) -> bool {
    let test_line = "sample log line...";
    pattern.is_match(test_line)
}
```

### 2.4 성능 최적화 구현

#### 2.4.1 병렬 처리
```rust
use rayon::prelude::*;

pub fn parallel_parse_lines(lines: &[String], pattern: &Regex) -> Vec<Block> {
    lines
        .par_iter()
        .filter_map(|line| Block::from_line(line, pattern))
        .collect()
}
```

#### 2.4.2 메모리 매핑
```rust
use memmap2::Mmap;
use std::fs::File;

pub fn read_large_file(path: &str) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let content = std::str::from_utf8(&mmap).unwrap();
    Ok(content.to_string())
}
```

#### 2.4.3 데이터 압축
```rust
use zstd::stream::{encode_all, decode_all};

pub fn compress_data(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    encode_all(data, 3) // 압축 레벨 3
}

pub fn decompress_data(compressed: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    decode_all(compressed)
}
```

## 3. API 설계 및 구현

### 3.1 Tauri Command API

```rust
#[tauri::command]
pub async fn parse_trace_file(
    file_path: String, 
    trace_type: String,
    window: Window
) -> Result<Vec<u8>, String> {
    // 진행상황 이벤트 발송
    window.emit("parse_progress", 0).unwrap();
    
    let result = match trace_type.as_str() {
        "block" => block::parse_file(&file_path, &window).await,
        "ufs" => ufs::parse_file(&file_path, &window).await,
        _ => Err("Unsupported trace type".to_string()),
    };
    
    window.emit("parse_progress", 100).unwrap();
    result
}

#[tauri::command]
pub async fn get_trace_stats(
    file_path: String,
    trace_type: String
) -> Result<TraceStats, String> {
    let cached_data = get_cached_data(&file_path, &trace_type)?;
    let stats = calculate_comprehensive_stats(&cached_data);
    Ok(stats)
}
```

### 3.2 Frontend API 래퍼

```typescript
// src/api/trace.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export class TraceAPI {
    static async parseFile(filePath: string, traceType: string): Promise<Uint8Array> {
        // 진행상황 리스너 등록
        const unlisten = await listen('parse_progress', (event) => {
            console.log('Progress:', event.payload);
        });
        
        try {
            const result = await invoke('parse_trace_file', {
                filePath,
                traceType
            });
            return new Uint8Array(result as number[]);
        } finally {
            unlisten();
        }
    }
    
    static async getStats(filePath: string, traceType: string): Promise<TraceStats> {
        return await invoke('get_trace_stats', {
            filePath,
            traceType
        });
    }
}
```

## 4. 데이터 시각화 구현

### 4.1 ECharts 통합

```typescript
// src/components/detail/scattercharts.svelte
import * as echarts from 'echarts';
import { onMount } from 'svelte';

let chartContainer: HTMLDivElement;
let chart: echarts.ECharts;

onMount(() => {
    chart = echarts.init(chartContainer);
    
    const option = {
        title: { text: '지연시간 분석' },
        xAxis: { type: 'value', name: '시간 (초)' },
        yAxis: { type: 'value', name: '지연시간 (ms)' },
        series: [{
            type: 'scatter',
            data: processedData,
            symbolSize: 3,
            itemStyle: { color: '#1f77b4' }
        }],
        tooltip: {
            trigger: 'item',
            formatter: (params) => `시간: ${params.value[0]}초<br/>지연시간: ${params.value[1]}ms`
        }
    };
    
    chart.setOption(option);
});
```

### 4.2 Apache Arrow 데이터 처리

```typescript
// src/utils/arrow-helper.ts
import { tableFromIPC } from 'apache-arrow';

export function parseArrowData(bytes: Uint8Array) {
    const table = tableFromIPC(bytes);
    
    return {
        timestamps: table.getChild('timestamp')?.toArray() || [],
        latencies: table.getChild('latency')?.toArray() || [],
        commands: table.getChild('command')?.toArray() || [],
    };
}

export function aggregateData(data: ArrowData, aggregationType: 'mean' | 'sum' | 'count') {
    // 시간 윈도우별 집계
    const timeWindows = createTimeWindows(data.timestamps);
    
    return timeWindows.map(window => ({
        time: window.start,
        value: calculateAggregation(window.data, aggregationType)
    }));
}
```

## 5. 오류 처리 및 로깅

### 5.1 Rust 오류 처리

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<TraceError> for String {
    fn from(error: TraceError) -> Self {
        error.to_string()
    }
}
```

### 5.2 Frontend 오류 처리

```typescript
// src/utils/error-handler.ts
export class ESTraceError extends Error {
    constructor(
        message: string,
        public code: string,
        public details?: any
    ) {
        super(message);
        this.name = 'ESTraceError';
    }
}

export function handleAPIError(error: any): ESTraceError {
    if (typeof error === 'string') {
        return new ESTraceError(error, 'UNKNOWN_ERROR');
    }
    
    return new ESTraceError(
        'An unexpected error occurred',
        'INTERNAL_ERROR',
        error
    );
}
```

## 6. 테스트 전략

### 6.1 Rust 단위 테스트

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_block_parsing() {
        let test_line = "sample block trace line...";
        let pattern = Regex::new(r"test pattern").unwrap();
        
        let result = Block::from_line(test_line, &pattern);
        assert!(result.is_some());
    }
    
    #[tokio::test]
    async fn test_file_parsing() {
        let result = parse_file("test_data/sample.log").await;
        assert!(result.is_ok());
    }
}
```

### 6.2 Frontend 테스트

```typescript
// tests/trace-api.test.ts
import { describe, it, expect } from 'vitest';
import { TraceAPI } from '../src/api/trace';

describe('TraceAPI', () => {
    it('should parse trace file correctly', async () => {
        const result = await TraceAPI.parseFile('test.log', 'block');
        expect(result).toBeInstanceOf(Uint8Array);
    });
});
```

## 7. 빌드 및 배포

### 7.1 개발 환경 설정

```bash
# 의존성 설치
npm install

# Rust 툴체인 설정
rustup target add x86_64-apple-darwin  # macOS
rustup target add x86_64-pc-windows-msvc  # Windows
rustup target add x86_64-unknown-linux-gnu  # Linux
```

### 7.2 빌드 스크립트

```json
{
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "tauri-dev": "tauri dev --no-watch",
    "tauri-build": "tauri build"
  }
}
```

### 7.3 CI/CD 파이프라인

```yaml
# .github/workflows/build.yml
name: Build and Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    
    steps:
    - uses: actions/checkout@v3
    - uses: actions/setup-node@v3
      with:
        node-version: '18'
    - uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
    
    - run: npm install
    - run: npm run test
    - run: cargo test
```

이 구현 문서는 ESTrace의 실제 코드 구현 방법과 기술적 세부사항을 제공하며, 개발자가 프로젝트를 이해하고 기여할 수 있도록 도움을 줍니다.