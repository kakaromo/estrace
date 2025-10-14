# 임시 파일 자동 정리 기능 - 구현 완료

## 📋 개요

GitHub Copilot PR 리뷰에서 제안된 "임시 파일 자동 정리 메커니즘"을 구현했습니다.
DB에 등록된 로그 폴더에서 24시간 이상 된 임시 Arrow 파일(`estrace_temp_*.arrow`)을 자동으로 삭제합니다.

## 🎯 구현된 기능

### 1. 백엔드 (Rust)

**파일**: `src-tauri/src/trace/utils.rs`

```rust
pub async fn cleanup_temp_arrow_files_impl(
    db_path: String, 
    max_age_hours: u64
) -> Result<usize, String>
```

**특징**:
- ✅ DB 기반 폴더 검색: `test.db`의 `folder`와 `testinfo` 테이블에서 로그 폴더 경로 가져오기
- ✅ 정확한 대상 지정: DB에 등록된 폴더(및 하위 폴더)만 검색
- ✅ 시간 기반 삭제: 24시간 이상 된 임시 파일만 삭제
- ✅ 안전한 패턴 매칭: `estrace_temp_*.arrow` 파일만 대상
- ✅ 재귀 검색: 하위 폴더의 임시 파일도 정리

**주요 로직**:
```sql
-- 1. folder 테이블에서 기본 로그 폴더
SELECT path FROM folder WHERE id = 1

-- 2. testinfo 테이블에서 모든 테스트의 로그 폴더
SELECT DISTINCT logfolder FROM testinfo WHERE logfolder IS NOT NULL
```

**의존성 추가**: `Cargo.toml`
```toml
rusqlite = "0.32"
```

### 2. 프론트엔드 (TypeScript/Svelte)

#### API 모듈: `src/api/cleanup.ts`

```typescript
// 수동 정리
export async function cleanupTempArrowFiles(maxAgeHours: number = 24): Promise<number>

// 자동 정리 (앱 시작 시)
export async function autoCleanupOnStartup(): Promise<void>
```

#### 설정 메뉴: `src/components/menu/setting.svelte`

추가된 기능:
- "임시 파일 정리 (Clean Temp Files)" 버튼
- 버튼 클릭 시 24시간 이상 된 임시 파일 삭제
- 삭제된 파일 수를 알림으로 표시

#### 자동 실행: `src/routes/+page.svelte`

```typescript
onMount(async () => {
    // ... 기존 초기화 코드 ...
    
    // 자동으로 오래된 임시 파일 정리 (백그라운드)
    autoCleanupOnStartup().catch(err => {
        console.warn('자동 임시 파일 정리 실패 (무시됨):', err);
    });
});
```

## 📊 작동 방식

### 자동 정리 (앱 시작 시)
```
애플리케이션 시작
    ↓
DB에서 로그 폴더 경로 조회
    ↓
각 폴더 및 하위 폴더 검색
    ↓
estrace_temp_*.arrow 파일 찾기
    ↓
24시간 이상 된 파일만 삭제
    ↓
삭제 결과 로그 출력
```

### 수동 정리 (설정 메뉴)
```
설정 메뉴 열기
    ↓
"임시 파일 정리" 버튼 클릭
    ↓
cleanup_temp_arrow_files(24) 호출
    ↓
삭제 결과 알림 표시
```

## 🚀 사용 방법

### 1. 자동 정리 (기본 활성화)
- 애플리케이션을 시작하면 자동으로 24시간 이상 된 임시 파일 정리
- 콘솔에 정리 결과 로그 출력
- 에러 발생 시에도 앱 시작은 계속됨

### 2. 수동 정리
1. 상단 메뉴에서 **설정 (Settings)** 열기
2. "Data Management" 섹션에서 **"임시 파일 정리 (Clean Temp Files)"** 버튼 클릭
3. 삭제 결과 확인

### 3. 프로그래밍 방식
```typescript
import { cleanupTempArrowFiles } from '$api/cleanup';

// 24시간 기준
const count = await cleanupTempArrowFiles(24);
console.log(`${count}개 삭제됨`);

// 1시간 기준 (테스트용)
const count = await cleanupTempArrowFiles(1);
```

## 📁 변경된 파일

### 백엔드
- ✅ `src-tauri/Cargo.toml` - rusqlite 의존성 추가
- ✅ `src-tauri/src/trace/utils.rs` - 정리 함수 구현
- ✅ `src-tauri/src/trace/mod.rs` - Tauri 명령 래퍼
- ✅ `src-tauri/src/lib.rs` - 명령 핸들러 등록

### 프론트엔드
- ✅ `src/api/cleanup.ts` - 새 파일, API 모듈
- ✅ `src/components/menu/setting.svelte` - 정리 버튼 추가
- ✅ `src/routes/+page.svelte` - 자동 정리 추가

## ✅ 테스트 확인 사항

1. **컴파일 성공**: `cargo check` ✅
2. **타입스크립트 문법 확인** ✅
3. **실행 테스트 필요**:
   - [ ] 앱 시작 시 콘솔에 정리 로그 확인
   - [ ] 설정 메뉴에서 수동 정리 버튼 동작 확인
   - [ ] 임시 파일이 실제로 삭제되는지 확인

## 🔍 로그 예시

### 정상 실행
```
🚀 애플리케이션 시작: 임시 파일 자동 정리 시작
🧹 임시 파일 정리 시작 (DB: /Users/.../test.db)
📂 검색할 폴더 수: 3
🗑️  삭제: /path/to/log/estrace_temp_ufs_1729012345.arrow (26시간 전)
🗑️  삭제: /path/to/log/estrace_temp_block_1729012345.arrow (26시간 전)
✅ 임시 파일 정리 완료: 2개 삭제
```

### 정리할 파일 없음
```
🧹 임시 파일 정리 시작 (DB: /Users/.../test.db)
📂 검색할 폴더 수: 1
ℹ️  정리할 임시 파일 없음
```

## 🎉 완료된 Copilot 리뷰 항목

| 번호 | 항목 | 상태 |
|------|------|------|
| 1 | 임시 파일 삭제 에러 메시지 개선 | ✅ |
| 2 | 백엔드 임시 파일 자동 정리 | ✅ |
| 3 | JSDoc 문서화 | ✅ |
| 4 | NaN 체크 가독성 개선 | ✅ |
| 5 | 중복 처리 구조 | ✅ (현재 구조 유지) |
| 6 | 하드코딩 버퍼 크기 | ⏭️ (보류) |

**핵심 리뷰 항목 4개 완료! 🎊**
