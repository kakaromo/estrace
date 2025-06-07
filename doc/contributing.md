# ESTrace 기여 가이드

ESTrace는 오픈 소스 프로젝트로, 커뮤니티의 기여를 환영합니다. 이 가이드는 프로젝트에 기여하는 방법을 설명합니다.

## 시작하기

### 저장소 포크 및 클론

1. GitHub에서 [ESTrace 저장소](https://github.com/kakaromo/estrace)를 포크합니다.
2. 포크한 저장소를 로컬 환경에 클론합니다:
   ```bash
   git clone https://github.com/your-username/estrace.git
   cd estrace
   ```

### 개발 환경 설정

1. 필수 도구 설치:
   - Node.js 16.x 이상
   - Rust 1.60 이상
   - VSCode 또는 선호하는 IDE

2. 의존성 설치:
   ```bash
   npm install
   ```

3. 개발 서버 실행:
   ```bash
   npm run tauri-dev
   ```

## 코딩 스타일 및 규칙

### JavaScript/TypeScript 코딩 스타일

- [ESLint](https://eslint.org/) 및 [Prettier](https://prettier.io/) 구성을 따릅니다.
- 함수형 프로그래밍 접근 방식을 선호합니다.
- 모든 새 코드는 TypeScript로 작성해야 합니다.
- 주석과 문서 문자열을 통해 코드를 문서화합니다.

```typescript
// 좋은 예:
/**
 * 지연 시간 통계를 계산합니다.
 * @param data 분석할 트레이스 데이터
 * @returns 계산된 통계 객체
 */
function calculateLatencyStats(data: TraceData): LatencyStats {
  // 구현...
}

// 나쁜 예:
function calc(d) {
  // ...
}
```

### Rust 코딩 스타일

- [Rustfmt](https://github.com/rust-lang/rustfmt) 및 [Clippy](https://github.com/rust-lang/rust-clippy) 구성을 따릅니다.
- 함수와 구조체에 문서 주석을 작성합니다.
- 오류 처리에 `Result` 타입을 사용합니다.

```rust
/// 트레이스 데이터를 필터링합니다.
///
/// # Arguments
///
/// * `data` - 필터링할 트레이스 데이터
/// * `filter` - 적용할 필터 규칙
///
/// # Returns
///
/// 필터링된 트레이스 데이터 또는 오류
pub fn filter_trace_data(data: &TraceData, filter: &FilterRule) -> Result<TraceData, TraceError> {
    // 구현...
}
```

### Svelte 컴포넌트 스타일

- 하나의 컴포넌트는 하나의 책임만 가져야 합니다.
- 상태 관리는 Svelte 스토어를 통해 집중화합니다.
- 컴포넌트 이름은 파스칼 케이스(PascalCase)로 작성합니다.
- 스타일은 Tailwind CSS 또는 컴포넌트 범위 CSS를 사용합니다.

```svelte
<!-- 좋은 예: -->
<script lang="ts">
  import { traceStore } from '../stores/trace';
  
  export let chartType: 'scatter' | 'bar' = 'scatter';
  
  function handleClick() {
    // 구현...
  }
</script>

<div class="chart-container">
  <h2>{chartType} Chart</h2>
  <div class="chart" use:createChart={chartType} />
  <button on:click={handleClick}>Update</button>
</div>

<style>
  .chart-container {
    /* 스타일 */
  }
</style>
```

## 브랜치 모델 및 커밋 규칙

### 브랜치 모델

- `main`: 안정적인 릴리즈 브랜치
- `develop`: 개발 브랜치, 새 기능이 통합되는 곳
- `feature/feature-name`: 새 기능 개발
- `bugfix/issue-number`: 버그 수정
- `enhancement/feature-name`: 기존 기능 개선

### 커밋 메시지 형식

커밋 메시지는 다음 형식을 따릅니다:

```
<type>(<scope>): <summary>

<body>
```

예:
```
feat(trace): 실시간 모니터링 기능 추가

- 실시간 데이터 스트리밍 구현
- UI 업데이트 메커니즘 개선
- 성능 최적화
```

**타입:**
- `feat`: 새 기능
- `fix`: 버그 수정
- `docs`: 문서 변경
- `style`: 코드 형식 변경 (기능 변경 없음)
- `refactor`: 코드 리팩토링
- `perf`: 성능 개선
- `test`: 테스트 추가 또는 수정
- `chore`: 빌드 프로세스, 도구 등의 변경

## 기여 프로세스

### 기능 개발 및 버그 수정

1. 먼저 [이슈 트래커](https://github.com/kakaromo/estrace/issues)를 확인하여 이미 보고된 이슈인지 확인합니다.
2. 새 이슈인 경우, 새 이슈를 생성하여 논의합니다.
3. 적절한 브랜치를 생성합니다.
4. 코드 변경 작업을 수행합니다.
5. 테스트를 작성하거나 수정합니다.
6. 변경사항을 커밋하고 원격 저장소에 푸시합니다.
7. 풀 리퀘스트(PR)를 생성합니다.

### 풀 리퀘스트 가이드라인

PR을 생성할 때 다음 정보를 포함해야 합니다:

1. PR의 목적과 해결하는 이슈에 대한 명확한 설명
2. 변경 사항에 대한 요약
3. 테스트 방법 및 결과
4. 스크린샷 (UI 변경의 경우)

### 코드 리뷰 프로세스

모든 PR은 병합되기 전에 코드 리뷰를 거쳐야 합니다:

1. 자동화된 검사 (린트, 타입 체크, 테스트) 통과
2. 최소 1명의 메인테이너 승인
3. 모든 리뷰 의견 해결
4. 필요한 경우 변경 사항 업데이트

## 테스트

### 테스트 작성 가이드라인

- 모든 새 기능에는 단위 테스트가 포함되어야 합니다.
- 가능하면 통합 테스트도 포함하세요.
- 테스트는 명확하고 간결해야 합니다.
- 테스트 더블(모의 객체 등)을 사용하여 외부 의존성을 격리하세요.

### 테스트 실행 방법

```bash
# 프론트엔드 단위 테스트
npm run test

# Rust 백엔드 테스트
cargo test

# 전체 테스트 스위트
npm run test:all
```

## 문서화

### 코드 문서화

- 모든 공개 API는 문서 주석을 포함해야 합니다.
- 복잡한 로직은 인라인 주석으로 설명합니다.
- TypeScript 타입 정의를 사용하여 코드 문서화를 보완합니다.

### 사용자 문서화

- 새 기능을 추가할 때 해당 문서를 업데이트하세요.
- 문서는 마크다운 형식으로 `/doc` 디렉토리에 있습니다.
- 스크린샷이나 다이어그램은 `/doc/assets` 디렉토리에 저장합니다.

## 릴리즈 프로세스

### 버전 관리

ESTrace는 [시맨틱 버저닝](https://semver.org/)을 따릅니다:

- **메이저 버전(X.y.z)**: 주요 호환성이 깨지는 변경사항
- **마이너 버전(x.Y.z)**: 후방 호환성이 있는 기능 추가
- **패치 버전(x.y.Z)**: 버그 수정 및 작은 개선 사항

### 릴리즈 체크리스트

릴리즈 전 다음 사항을 확인합니다:

1. 모든 테스트 통과
2. 문서 업데이트
3. 변경 로그 업데이트
4. 버전 번호 업데이트
5. 릴리즈 노트 작성

## 행동 강령

ESTrace 프로젝트는 기여자 행동 강령을 채택하여 모든 참여자에게 환영하는 커뮤니티를 만드는 데 전념합니다. 모든 기여자는 [행동 강령](CODE_OF_CONDUCT.md)을 존중해야 합니다.

## 라이선스

ESTrace는 MIT 라이선스로 배포됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참조하세요.

## 질문 및 도움말

질문이나 도움이 필요한 경우 다음 방법으로 문의할 수 있습니다:

- GitHub 이슈 생성
- 개발자 포럼 방문
- 메인테이너에게 직접 연락

감사합니다!

ESTrace 팀
