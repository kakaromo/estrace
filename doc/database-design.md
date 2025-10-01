# ESTrace 데이터베이스 및 스토리지 설계 문서

## 1. 개요

ESTrace는 다층 데이터 저장 아키텍처를 사용하여 메타데이터, 트레이스 데이터, 사용자 설정을 효율적으로 관리합니다. 이 문서는 데이터 모델, 스토리지 전략, 성능 최적화 기법을 상세히 설명합니다.

## 2. 데이터 저장 아키텍처

### 2.1 다층 스토리지 구조

```
┌─────────────────────────────────────────────────────────────┐
│                    응용 계층 (Frontend)                      │
│  ┌─────────────────┐  ┌─────────────────┐  ┌───────────────┐ │
│  │   IDB-KeyVal    │  │  Browser Cache  │  │  Svelte Store │ │
│  │  (임시 캐시)    │  │   (이미지 등)   │  │  (UI 상태)    │ │
│  └─────────────────┘  └─────────────────┘  └───────────────┘ │
└─────────────────────────────────────────────────────────────┘
                               │
                        Tauri Bridge
                               │
┌─────────────────────────────────────────────────────────────┐
│                      백엔드 계층 (Rust)                      │
│  ┌─────────────────┐  ┌─────────────────┐  ┌───────────────┐ │
│  │  Memory Cache   │  │   SQLite DB     │  │  File System  │ │
│  │ (트레이스 데이터)│  │  (메타데이터)   │  │ (원본 로그 등) │ │
│  └─────────────────┘  └─────────────────┘  └───────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 데이터 분류 및 저장 방식

| 데이터 유형 | 저장 방식 | 크기 | 접근 빈도 | 영속성 |
|------------|----------|------|-----------|--------|
| 트레이스 데이터 | Memory Cache + File | 대용량 | 높음 | 세션 |
| 메타데이터 | SQLite | 소용량 | 중간 | 영구 |
| 사용자 설정 | SQLite | 소용량 | 낮음 | 영구 |
| 패턴 정의 | SQLite | 소용량 | 중간 | 영구 |
| 임시 UI 상태 | Browser Storage | 소용량 | 높음 | 세션 |

## 3. SQLite 데이터베이스 설계

### 3.1 데이터베이스 스키마

```sql
-- 애플리케이션 설정 테이블
CREATE TABLE setting (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 테스트 정보 메타데이터
CREATE TABLE testinfo (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    logtype TEXT NOT NULL,           -- 'block', 'ufs'
    title TEXT NOT NULL,             -- 사용자 정의 제목
    content TEXT,                    -- 설명
    logfolder TEXT NOT NULL,         -- 로그 폴더 경로
    logname TEXT NOT NULL,           -- 로그 파일명
    sourcelog_path TEXT NOT NULL,    -- 원본 로그 파일 전체 경로
    file_size INTEGER,               -- 파일 크기 (바이트)
    line_count INTEGER,              -- 총 라인 수
    parsed_count INTEGER,            -- 파싱된 라인 수
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 파싱 패턴 관리
CREATE TABLE patterns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,       -- 패턴 이름
    pattern_type TEXT NOT NULL,      -- 'block', 'ufs'
    regex_pattern TEXT NOT NULL,     -- 정규표현식 패턴
    description TEXT,                -- 패턴 설명
    is_active BOOLEAN DEFAULT FALSE, -- 활성 패턴 여부
    test_data TEXT,                  -- 테스트용 샘플 데이터
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 트레이스 분석 결과 캐시
CREATE TABLE analysis_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    testinfo_id INTEGER NOT NULL,
    analysis_type TEXT NOT NULL,     -- 'latency', 'throughput', 'pattern'
    cache_key TEXT NOT NULL,         -- 분석 매개변수 해시
    result_data BLOB,                -- 압축된 분석 결과
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME,
    FOREIGN KEY (testinfo_id) REFERENCES testinfo(id) ON DELETE CASCADE
);

-- 사용자 즐겨찾기 및 북마크
CREATE TABLE bookmarks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    testinfo_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    filter_conditions TEXT,          -- JSON 형태의 필터 조건
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (testinfo_id) REFERENCES testinfo(id) ON DELETE CASCADE
);

-- 버퍼 크기 설정
CREATE TABLE buffersize (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    buffersize INTEGER NOT NULL DEFAULT 8192
);

-- 인덱스 생성
CREATE INDEX idx_testinfo_logtype ON testinfo(logtype);
CREATE INDEX idx_testinfo_created_at ON testinfo(created_at);
CREATE INDEX idx_patterns_type_active ON patterns(pattern_type, is_active);
CREATE INDEX idx_analysis_cache_testinfo ON analysis_cache(testinfo_id);
CREATE INDEX idx_analysis_cache_type ON analysis_cache(analysis_type);
```

### 3.2 데이터베이스 초기화 및 마이그레이션

```rust
// src-tauri/src/db/mod.rs
use sqlx::{Pool, Sqlite, SqlitePool};
use std::path::Path;

pub struct DatabaseManager {
    pool: SqlitePool,
}

impl DatabaseManager {
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        let connection_string = format!("sqlite:{}", db_path.display());
        let pool = SqlitePool::connect(&connection_string).await?;
        
        let manager = Self { pool };
        manager.run_migrations().await?;
        
        Ok(manager)
    }
    
    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        // 테이블 생성
        sqlx::query(include_str!("../sql/create_tables.sql"))
            .execute(&self.pool)
            .await?;
        
        // 스키마 버전 확인 및 업그레이드
        self.upgrade_schema().await?;
        
        Ok(())
    }
    
    async fn upgrade_schema(&self) -> Result<(), sqlx::Error> {
        let version = self.get_schema_version().await?;
        
        match version {
            0 => self.upgrade_to_v1().await?,
            1 => self.upgrade_to_v2().await?,
            _ => {} // 최신 버전
        }
        
        Ok(())
    }
}
```

## 4. 메모리 캐시 시스템

### 4.1 계층적 캐시 구조

```rust
// src-tauri/src/cache/mod.rs
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use once_cell::sync::Lazy;

// 트레이스 데이터 캐시
pub static TRACE_CACHE: Lazy<RwLock<HashMap<String, Arc<TraceData>>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

// 분석 결과 캐시
pub static ANALYSIS_CACHE: Lazy<RwLock<HashMap<String, Arc<AnalysisResult>>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
pub struct CacheManager {
    max_memory_usage: usize,    // 최대 메모리 사용량 (바이트)
    current_usage: Arc<RwLock<usize>>,
}

impl CacheManager {
    pub fn new(max_memory_mb: usize) -> Self {
        Self {
            max_memory_usage: max_memory_mb * 1024 * 1024,
            current_usage: Arc::new(RwLock::new(0)),
        }
    }
    
    pub fn get_trace_data(&self, key: &str) -> Option<Arc<TraceData>> {
        let cache = TRACE_CACHE.read().unwrap();
        cache.get(key).cloned()
    }
    
    pub fn set_trace_data(&self, key: String, data: TraceData) -> Result<(), CacheError> {
        let data_size = std::mem::size_of_val(&data);
        
        // 메모리 사용량 확인
        if self.would_exceed_limit(data_size) {
            self.evict_lru_entries(data_size)?;
        }
        
        let arc_data = Arc::new(data);
        {
            let mut cache = TRACE_CACHE.write().unwrap();
            cache.insert(key, arc_data);
        }
        
        // 사용량 업데이트
        {
            let mut usage = self.current_usage.write().unwrap();
            *usage += data_size;
        }
        
        Ok(())
    }
    
    fn evict_lru_entries(&self, needed_space: usize) -> Result<(), CacheError> {
        // LRU 방식으로 캐시 엔트리 제거
        // 구현 세부사항...
        Ok(())
    }
}
```

### 4.2 캐시 전략

#### 4.2.1 읽기 전략 (Read-Through Cache)
```rust
pub async fn get_trace_data_with_fallback(
    file_path: &str,
    cache_manager: &CacheManager
) -> Result<Arc<TraceData>, TraceError> {
    // 1. 캐시에서 확인
    if let Some(cached_data) = cache_manager.get_trace_data(file_path) {
        return Ok(cached_data);
    }
    
    // 2. 파일에서 로드
    let trace_data = load_trace_from_file(file_path).await?;
    
    // 3. 캐시에 저장
    cache_manager.set_trace_data(file_path.to_string(), trace_data.clone())?;
    
    Ok(Arc::new(trace_data))
}
```

#### 4.2.2 쓰기 전략 (Write-Behind Cache)
```rust
pub async fn save_analysis_result(
    key: &str,
    result: AnalysisResult,
    cache_manager: &CacheManager,
    db_manager: &DatabaseManager
) -> Result<(), TraceError> {
    // 1. 캐시에 즉시 저장
    cache_manager.set_analysis_result(key.to_string(), result.clone())?;
    
    // 2. 비동기적으로 데이터베이스에 저장
    tokio::spawn(async move {
        if let Err(e) = db_manager.save_analysis_cache(key, &result).await {
            eprintln!("Failed to save analysis to DB: {}", e);
        }
    });
    
    Ok(())
}
```

## 5. 파일 시스템 관리

### 5.1 파일 구조

```
~/.estrace/
├── database/
│   └── trace.db              # SQLite 데이터베이스
├── cache/
│   ├── compressed/           # 압축된 트레이스 데이터
│   └── analysis/             # 분석 결과 캐시
├── exports/
│   ├── parquet/              # Parquet 내보내기 파일
│   └── csv/                  # CSV 내보내기 파일
├── patterns/
│   └── custom/               # 사용자 정의 패턴
└── logs/
    └── application.log       # 애플리케이션 로그
```

### 5.2 파일 관리 API

```rust
// src-tauri/src/storage/file_manager.rs
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct FileManager {
    base_dir: PathBuf,
}

impl FileManager {
    pub fn new() -> Result<Self, std::io::Error> {
        let base_dir = Self::get_app_data_dir()?;
        fs::create_dir_all(&base_dir)?;
        
        Ok(Self { base_dir })
    }
    
    fn get_app_data_dir() -> Result<PathBuf, std::io::Error> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Home directory not found"
            ))?;
        
        Ok(home_dir.join(".estrace"))
    }
    
    pub fn get_cache_path(&self, cache_type: &str) -> PathBuf {
        self.base_dir.join("cache").join(cache_type)
    }
    
    pub fn get_export_path(&self, format: &str) -> PathBuf {
        self.base_dir.join("exports").join(format)
    }
    
    pub async fn ensure_directory(&self, path: &Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            fs::create_dir_all(path).await?;
        }
        Ok(())
    }
}
```

## 6. 데이터 압축 및 직렬화

### 6.1 압축 전략

```rust
// src-tauri/src/storage/compression.rs
use zstd::stream::{encode_all, decode_all};
use serde::{Serialize, Deserialize};

pub struct CompressionManager {
    compression_level: i32,
}

impl CompressionManager {
    pub fn new(level: i32) -> Self {
        Self {
            compression_level: level,
        }
    }
    
    pub fn compress_trace_data<T>(&self, data: &T) -> Result<Vec<u8>, CompressionError>
    where
        T: Serialize,
    {
        // 1. 직렬화
        let serialized = bincode::serialize(data)?;
        
        // 2. 압축
        let compressed = encode_all(&serialized[..], self.compression_level)?;
        
        Ok(compressed)
    }
    
    pub fn decompress_trace_data<T>(&self, compressed_data: &[u8]) -> Result<T, CompressionError>
    where
        T: for<'de> Deserialize<'de>,
    {
        // 1. 압축 해제
        let decompressed = decode_all(compressed_data)?;
        
        // 2. 역직렬화
        let data = bincode::deserialize(&decompressed)?;
        
        Ok(data)
    }
}
```

### 6.2 Apache Arrow 통합

```rust
// src-tauri/src/storage/arrow_storage.rs
use arrow::array::*;
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use arrow::ipc::reader::StreamReader;

pub struct ArrowStorageManager {
    compression: Option<CompressionType>,
}

impl ArrowStorageManager {
    pub fn serialize_to_ipc(batch: &RecordBatch) -> Result<Vec<u8>, ArrowError> {
        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema())?;
            writer.write(batch)?;
            writer.finish()?;
        }
        Ok(buffer)
    }
    
    pub fn deserialize_from_ipc(data: &[u8]) -> Result<RecordBatch, ArrowError> {
        let cursor = std::io::Cursor::new(data);
        let mut reader = StreamReader::try_new(cursor, None)?;
        
        reader.next()
            .ok_or_else(|| ArrowError::IoError("No data in stream".to_string()))?
    }
}
```

## 7. 성능 최적화

### 7.1 인덱싱 전략

```sql
-- 복합 인덱스로 쿼리 성능 향상
CREATE INDEX idx_testinfo_composite ON testinfo(logtype, created_at DESC);
CREATE INDEX idx_analysis_cache_composite ON analysis_cache(testinfo_id, analysis_type);

-- 부분 인덱스로 저장 공간 절약
CREATE INDEX idx_patterns_active ON patterns(pattern_type) WHERE is_active = 1;
```

### 7.2 쿼리 최적화

```rust
// 효율적인 페이징 쿼리
pub async fn get_test_info_paginated(
    &self,
    offset: i64,
    limit: i64,
    filter: Option<&str>
) -> Result<Vec<TestInfo>, sqlx::Error> {
    let query = match filter {
        Some(f) => sqlx::query_as!(
            TestInfo,
            "SELECT * FROM testinfo WHERE logtype = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            f, limit, offset
        ),
        None => sqlx::query_as!(
            TestInfo,
            "SELECT * FROM testinfo ORDER BY created_at DESC LIMIT ? OFFSET ?",
            limit, offset
        ),
    };
    
    query.fetch_all(&self.pool).await
}
```

### 7.3 메모리 사용량 모니터링

```rust
pub struct MemoryMonitor {
    max_usage: usize,
    current_usage: Arc<AtomicUsize>,
    warning_threshold: f64,
}

impl MemoryMonitor {
    pub fn check_memory_pressure(&self) -> MemoryPressure {
        let current = self.current_usage.load(Ordering::Relaxed);
        let usage_ratio = current as f64 / self.max_usage as f64;
        
        match usage_ratio {
            r if r > 0.9 => MemoryPressure::Critical,
            r if r > self.warning_threshold => MemoryPressure::High,
            r if r > 0.5 => MemoryPressure::Medium,
            _ => MemoryPressure::Low,
        }
    }
    
    pub async fn handle_memory_pressure(&self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Critical => {
                // 강제 가비지 컬렉션 및 캐시 정리
                self.force_cache_cleanup().await;
            },
            MemoryPressure::High => {
                // 오래된 캐시 엔트리 제거
                self.cleanup_old_entries().await;
            },
            _ => {}
        }
    }
}
```

## 8. 백업 및 복구

### 8.1 데이터베이스 백업

```rust
pub async fn create_backup(&self, backup_path: &Path) -> Result<(), BackupError> {
    // SQLite 백업 API 사용
    let backup_conn = Connection::open(backup_path)?;
    let main_conn = self.pool.acquire().await?;
    
    // 온라인 백업 수행
    let backup = Backup::new(&main_conn, &backup_conn)?;
    backup.run_to_completion(5, Duration::from_millis(250), None)?;
    
    Ok(())
}
```

### 8.2 데이터 무결성 검증

```rust
pub async fn verify_data_integrity(&self) -> Result<IntegrityReport, DatabaseError> {
    let mut report = IntegrityReport::new();
    
    // 외래 키 제약 조건 확인
    let fk_violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&self.pool)
        .await?;
    
    report.foreign_key_violations = fk_violations.len();
    
    // 인덱스 무결성 확인
    let index_check = sqlx::query("PRAGMA integrity_check")
        .fetch_one(&self.pool)
        .await?;
    
    report.integrity_ok = index_check.get::<String, _>("integrity_check") == "ok";
    
    Ok(report)
}
```

이 데이터베이스 및 스토리지 설계 문서는 ESTrace의 데이터 관리 전략과 성능 최적화 기법을 종합적으로 다루며, 확장 가능하고 효율적인 데이터 아키텍처를 제공합니다.