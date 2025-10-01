# ESTrace UI/UX 설계 문서

## 1. 사용자 인터페이스 개요

ESTrace는 데이터 분석 도구의 복잡성을 사용자 친화적인 인터페이스로 추상화하여 제공합니다. 본 문서는 사용자 경험 설계 원칙, 컴포넌트 구조, 인터랙션 패턴을 상세히 설명합니다.

### 1.1 설계 철학

- **단순성**: 복잡한 데이터를 직관적으로 표현
- **일관성**: 모든 화면에서 일관된 디자인 언어 사용
- **효율성**: 최소한의 클릭으로 원하는 작업 수행
- **접근성**: 다양한 사용자 요구사항 고려
- **반응성**: 빠른 피드백과 로딩 상태 표시

### 1.2 타겟 사용자

- **Primary**: Android 성능 엔지니어, 시스템 개발자
- **Secondary**: QA 엔지니어, 기술 지원팀
- **사용 맥락**: 성능 문제 진단, 최적화 작업, 보고서 작성

## 2. 정보 아키텍처

### 2.1 네비게이션 구조

```
ESTrace App
├── 메인 대시보드
│   ├── 트레이스 파일 목록
│   ├── 최근 분석 결과
│   └── 빠른 작업 패널
├── 분석 화면
│   ├── 개요 탭
│   ├── 지연시간 분석 탭
│   ├── 읽기/쓰기 분석 탭
│   ├── 크기 분석 탭
│   └── 고급 시각화 탭
├── 설정 화면
│   ├── 패턴 관리
│   ├── 내보내기 설정
│   └── 일반 설정
└── 도움말
    ├── 사용 가이드
    ├── API 문서
    └── 문제 해결
```

### 2.2 화면 간 연결 구조

```
[메인 대시보드] ←→ [분석 화면] ←→ [설정 화면]
      ↓               ↓              ↓
[파일 선택기]    [상세 차트]    [패턴 편집기]
      ↓               ↓              ↓
[진행 상황]      [데이터 필터]   [테스트 도구]
```

## 3. 주요 화면 설계

### 3.1 메인 대시보드

#### 3.1.1 레이아웃 구조
```
┌─────────────────────────────────────────────────────────────┐
│  ESTrace                                            ⚙️ ❓ ✕  │
├─────────────────────────────────────────────────────────────┤
│  📁 새 파일 추가   |   🔄 새로고침   |   📊 대시보드       │
├─────────────────────────────────────────────────────────────┤
│  트레이스 파일 목록                    │   빠른 작업         │
│  ┌─────────────────────────────────┐   │  ┌─────────────────┐ │
│  │ 📄 trace_001.log                │   │  │ 🔍 패턴 테스터  │ │
│  │    Block I/O • 2.3MB            │   │  │ 📤 일괄 내보내기│ │
│  │    2024-10-01 14:30             │   │  │ 🧹 캐시 정리   │ │
│  ├─────────────────────────────────┤   │  └─────────────────┘ │
│  │ 📄 ufs_perf.log                 │   │                     │
│  │    UFS • 1.8MB                  │   │   최근 분석 결과     │
│  │    2024-10-01 13:15             │   │  ┌─────────────────┐ │
│  └─────────────────────────────────┘   │  │ 📈 지연시간     │ │
│                                         │  │    평균: 2.3ms  │ │
│                                         │  │ 📊 처리량       │ │
│                                         │  │    1.2GB/s     │ │
│                                         │  └─────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.2 주요 컴포넌트

**파일 목록 컴포넌트**
- 카드 형태의 파일 표시
- 파일 타입별 아이콘 구분
- 빠른 미리보기 기능
- 드래그 앤 드롭 지원

**상태 표시 컴포넌트**
- 파싱 진행률 표시
- 오류 상태 알림
- 성공/실패 피드백

### 3.2 분석 화면

#### 3.2.1 탭 구조 설계
```
┌─────────────────────────────────────────────────────────────┐
│  ← 뒤로가기  trace_001.log                              ⚙️   │
├─────────────────────────────────────────────────────────────┤
│  📊 개요  📈 지연시간  📖 R/W분석  📏 크기분석  🎯 고급      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   [활성 탭 내용]                                            │
│                                                             │
│                                                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.2 개요 탭 레이아웃
```
┌─────────────────────────────────────────────────────────────┐
│  기본 정보                           │   주요 통계            │
│  ┌─────────────────────────────────┐ │  ┌─────────────────────┐ │
│  │ 파일명: trace_001.log           │ │  │ 총 이벤트: 10,234   │ │
│  │ 타입: Block I/O                 │ │  │ 읽기: 6,789 (66%)   │ │
│  │ 크기: 2.3MB                     │ │  │ 쓰기: 3,445 (34%)   │ │
│  │ 기간: 00:02:30                  │ │  │ 평균 지연: 2.3ms    │ │
│  └─────────────────────────────────┘ │  └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│  시간별 활동 그래프                                          │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ [활동량 타임라인 차트]                                  │ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.3 지연시간 분석 탭
```
┌─────────────────────────────────────────────────────────────┐
│  필터 및 설정                                              │
│  [시간 범위] [명령 타입] [크기 범위] [🔍 적용] [🗑️ 초기화]  │
├─────────────────────────────────────────────────────────────┤
│  통계 요약                          │   분포 차트            │
│  ┌─────────────────────────────────┐ │  ┌─────────────────────┐ │
│  │ 최소: 0.1ms  │ P90: 4.2ms      │ │  │                     │ │
│  │ 최대: 45.2ms │ P95: 8.1ms      │ │  │  [히스토그램]       │ │
│  │ 평균: 2.3ms  │ P99: 15.3ms     │ │  │                     │ │
│  │ 중앙값: 1.8ms│ 표준편차: 3.4ms │ │  │                     │ │
│  └─────────────────────────────────┘ │  └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│  시계열 차트                                                │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ [시간 vs 지연시간 스캐터 플롯]                          │ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 설정 화면

#### 3.3.1 패턴 관리 인터페이스
```
┌─────────────────────────────────────────────────────────────┐
│  패턴 관리                                                  │
├─────────────────────────────────────────────────────────────┤
│  패턴 목록                          │   패턴 편집기          │
│  ┌─────────────────────────────────┐ │  ┌─────────────────────┐ │
│  │ ● Default Block Pattern         │ │  │ 패턴명:             │ │
│  │   Default UFS Pattern           │ │  │ [텍스트 입력]       │ │
│  │   Custom Pattern 1              │ │  │                     │ │
│  │                                 │ │  │ 정규표현식:         │ │
│  │ [+ 새 패턴 추가]                │ │  │ [코드 에디터]       │ │
│  └─────────────────────────────────┘ │  │                     │ │
│                                     │  │ [테스트] [저장]     │ │
│                                     │  └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│  테스트 결과                                                │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ 샘플 로그: sample log line...                           │ │
│  │ 매칭 결과: ✅ 성공 (3개 그룹 캡처)                      │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## 4. 컴포넌트 설계

### 4.1 재사용 가능한 UI 컴포넌트

#### 4.1.1 기본 컴포넌트
```typescript
// 기본 버튼 컴포넌트
interface ButtonProps {
  variant: 'primary' | 'secondary' | 'danger';
  size: 'sm' | 'md' | 'lg';
  icon?: string;
  loading?: boolean;
  disabled?: boolean;
}

// 데이터 카드 컴포넌트
interface DataCardProps {
  title: string;
  value: string | number;
  change?: number;
  trend?: 'up' | 'down' | 'stable';
  unit?: string;
}

// 진행률 표시 컴포넌트
interface ProgressProps {
  value: number;
  max: number;
  label?: string;
  showPercentage?: boolean;
  variant?: 'default' | 'success' | 'warning' | 'error';
}
```

#### 4.1.2 차트 컴포넌트
```typescript
// 통용 차트 래퍼
interface ChartWrapperProps {
  type: 'line' | 'bar' | 'scatter' | 'heatmap';
  data: ChartData;
  options?: ChartOptions;
  loading?: boolean;
  error?: string;
}

// 지연시간 히스토그램
interface LatencyHistogramProps {
  data: LatencyData[];
  bins?: number;
  showStats?: boolean;
  interactive?: boolean;
}

// 시계열 차트
interface TimeSeriesChartProps {
  data: TimeSeriesData[];
  xAxis: 'timestamp' | 'sequence';
  yAxis: string[];
  aggregation?: 'raw' | 'average' | 'sum';
}
```

### 4.2 복합 컴포넌트

#### 4.2.1 파일 선택기 컴포넌트
```svelte
<!-- FileSelector.svelte -->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  
  export let accept: string = '.log,.txt';
  export let multiple: boolean = false;
  export let dragAndDrop: boolean = true;
  
  const dispatch = createEventDispatcher<{
    filesSelected: File[];
    error: string;
  }>();
  
  let isDragOver = false;
  
  function handleDrop(event: DragEvent) {
    event.preventDefault();
    isDragOver = false;
    
    const files = Array.from(event.dataTransfer?.files || []);
    if (files.length > 0) {
      dispatch('filesSelected', files);
    }
  }
</script>

<div 
  class="file-selector"
  class:drag-over={isDragOver}
  on:dragover|preventDefault={() => isDragOver = true}
  on:dragleave={() => isDragOver = false}
  on:drop={handleDrop}
>
  <div class="drop-zone">
    <svg class="upload-icon" viewBox="0 0 24 24">
      <!-- 업로드 아이콘 -->
    </svg>
    <h3>파일을 선택하거나 여기에 드래그하세요</h3>
    <p>지원 형식: .log, .txt (최대 100MB)</p>
    <button class="select-button">파일 선택</button>
  </div>
</div>
```

#### 4.2.2 분석 결과 테이블
```svelte
<!-- AnalysisTable.svelte -->
<script lang="ts">
  import { VirtualList } from '@sveltejs/svelte-virtual-list';
  
  export let data: AnalysisResult[];
  export let columns: TableColumn[];
  export let sortBy: string = '';
  export let sortOrder: 'asc' | 'desc' = 'asc';
  
  let filteredData = data;
  let searchTerm = '';
  
  $: {
    filteredData = data.filter(row => 
      Object.values(row).some(value => 
        String(value).toLowerCase().includes(searchTerm.toLowerCase())
      )
    );
  }
</script>

<div class="table-container">
  <div class="table-header">
    <input 
      bind:value={searchTerm}
      placeholder="검색..."
      class="search-input"
    />
    <button class="export-button">내보내기</button>
  </div>
  
  <div class="table-wrapper">
    <VirtualList items={filteredData} let:item>
      <tr class="table-row">
        {#each columns as column}
          <td class="table-cell">
            {item[column.key]}
          </td>
        {/each}
      </tr>
    </VirtualList>
  </div>
</div>
```

## 5. 상호작용 설계

### 5.1 사용자 흐름 (User Flow)

#### 5.1.1 기본 분석 워크플로우
```
1. 파일 선택
   ↓
2. 로딩 상태 표시
   ↓
3. 파싱 완료 확인
   ↓
4. 개요 화면 표시
   ↓
5. 세부 분석 탭 이동
   ↓
6. 필터 적용 및 분석
   ↓
7. 결과 내보내기
```

#### 5.1.2 패턴 관리 워크플로우
```
1. 설정 메뉴 접근
   ↓
2. 패턴 관리 선택
   ↓
3. 새 패턴 추가 또는 기존 패턴 편집
   ↓
4. 정규표현식 입력
   ↓
5. 테스트 데이터로 검증
   ↓
6. 저장 및 활성화
```

### 5.2 인터랙션 패턴

#### 5.2.1 즉시 피드백
- 버튼 클릭: 0.1초 내 시각적 반응
- 폼 입력: 실시간 유효성 검사
- 파일 드래그: 드롭 영역 하이라이트

#### 5.2.2 진행 상태 표시
```typescript
interface ProgressState {
  stage: 'parsing' | 'analyzing' | 'rendering';
  progress: number; // 0-100
  message: string;
  estimatedTime?: number;
}

// 예시: 파일 파싱 진행 상태
{
  stage: 'parsing',
  progress: 45,
  message: '로그 파일을 파싱하는 중... (234,567 / 521,890 라인)',
  estimatedTime: 15000 // 15초 예상
}
```

#### 5.2.3 오류 처리 및 복구
```typescript
interface ErrorState {
  type: 'warning' | 'error' | 'info';
  title: string;
  message: string;
  actions?: ErrorAction[];
  dismissible: boolean;
}

interface ErrorAction {
  label: string;
  action: () => void;
  style: 'primary' | 'secondary';
}
```

## 6. 반응형 설계

### 6.1 브레이크포인트 전략

```css
/* 데스크톱 기본 */
.container {
  max-width: 1200px;
  margin: 0 auto;
}

/* 중간 화면 (노트북) */
@media (max-width: 1024px) {
  .sidebar {
    width: 250px;
  }
  
  .chart-grid {
    grid-template-columns: 1fr;
  }
}

/* 작은 화면 (태블릿) */
@media (max-width: 768px) {
  .sidebar {
    transform: translateX(-100%);
  }
  
  .mobile-menu {
    display: block;
  }
}
```

### 6.2 적응형 컴포넌트 설계

```svelte
<script>
  import { onMount } from 'svelte';
  
  let screenSize = 'desktop';
  let windowWidth = 0;
  
  $: {
    if (windowWidth < 768) {
      screenSize = 'mobile';
    } else if (windowWidth < 1024) {
      screenSize = 'tablet';
    } else {
      screenSize = 'desktop';
    }
  }
</script>

<svelte:window bind:innerWidth={windowWidth} />

{#if screenSize === 'mobile'}
  <MobileLayout>
    <slot />
  </MobileLayout>
{:else if screenSize === 'tablet'}
  <TabletLayout>
    <slot />
  </TabletLayout>
{:else}
  <DesktopLayout>
    <slot />
  </DesktopLayout>
{/if}
```

## 7. 접근성 설계

### 7.1 키보드 내비게이션

```typescript
// 키보드 단축키 정의
const KEYBOARD_SHORTCUTS = {
  'Ctrl+O': 'openFile',
  'Ctrl+S': 'saveAnalysis',
  'Ctrl+E': 'exportData',
  'F5': 'refresh',
  'Escape': 'closeModal',
  'Tab': 'nextElement',
  'Shift+Tab': 'previousElement'
};

// 포커스 관리
class FocusManager {
  private focusableElements: HTMLElement[] = [];
  private currentIndex: number = 0;
  
  updateFocusableElements() {
    this.focusableElements = Array.from(
      document.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      )
    );
  }
  
  focusNext() {
    this.currentIndex = (this.currentIndex + 1) % this.focusableElements.length;
    this.focusableElements[this.currentIndex]?.focus();
  }
  
  focusPrevious() {
    this.currentIndex = this.currentIndex === 0 
      ? this.focusableElements.length - 1 
      : this.currentIndex - 1;
    this.focusableElements[this.currentIndex]?.focus();
  }
}
```

### 7.2 시각적 접근성

```css
/* 고대비 테마 지원 */
@media (prefers-contrast: high) {
  :root {
    --text-color: #000000;
    --bg-color: #ffffff;
    --border-color: #000000;
    --accent-color: #0000ff;
  }
}

/* 애니메이션 감소 설정 */
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}

/* 포커스 표시 */
.focus-visible {
  outline: 2px solid var(--accent-color);
  outline-offset: 2px;
}
```

### 7.3 스크린 리더 지원

```svelte
<!-- ARIA 레이블 및 설명 -->
<div 
  role="region" 
  aria-labelledby="chart-title"
  aria-describedby="chart-description"
>
  <h2 id="chart-title">지연시간 분석 차트</h2>
  <p id="chart-description">
    시간에 따른 I/O 지연시간 변화를 보여주는 스캐터 플롯입니다.
  </p>
  
  <!-- 차트 데이터 테이블 (스크린 리더용) -->
  <table class="sr-only">
    <caption>지연시간 데이터 테이블</caption>
    <thead>
      <tr>
        <th>시간</th>
        <th>지연시간 (ms)</th>
        <th>명령 타입</th>
      </tr>
    </thead>
    <tbody>
      {#each chartData as row}
        <tr>
          <td>{row.timestamp}</td>
          <td>{row.latency}</td>
          <td>{row.command}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
```

## 8. 성능 최적화

### 8.1 가상화 및 지연 로딩

```svelte
<!-- 대용량 데이터 테이블 가상화 -->
<script>
  import { VirtualList } from '@sveltejs/svelte-virtual-list';
  
  export let items: TraceData[];
  
  // 항목 높이 계산
  const itemHeight = 50;
  const containerHeight = 400;
</script>

<div class="virtual-container" style="height: {containerHeight}px">
  <VirtualList 
    {items} 
    {itemHeight}
    let:item
    let:index
  >
    <TraceRow data={item} {index} />
  </VirtualList>
</div>
```

### 8.2 이미지 및 차트 최적화

```typescript
// 차트 렌더링 최적화
class ChartRenderer {
  private renderQueue: RenderTask[] = [];
  private isRendering: boolean = false;
  
  async renderChart(config: ChartConfig): Promise<void> {
    // 렌더링 작업을 큐에 추가
    return new Promise((resolve) => {
      this.renderQueue.push({
        config,
        resolve
      });
      
      this.processQueue();
    });
  }
  
  private async processQueue(): Promise<void> {
    if (this.isRendering || this.renderQueue.length === 0) {
      return;
    }
    
    this.isRendering = true;
    
    // requestAnimationFrame을 사용한 부드러운 렌더링
    requestAnimationFrame(async () => {
      const task = this.renderQueue.shift();
      if (task) {
        await this.performRender(task.config);
        task.resolve();
      }
      
      this.isRendering = false;
      this.processQueue();
    });
  }
}
```

## 9. 테마 및 스타일 시스템

### 9.1 디자인 토큰

```css
:root {
  /* 색상 시스템 */
  --primary-50: #eff6ff;
  --primary-500: #3b82f6;
  --primary-900: #1e3a8a;
  
  --gray-50: #f9fafb;
  --gray-500: #6b7280;
  --gray-900: #111827;
  
  /* 타이포그래피 */
  --font-family-sans: 'Inter', system-ui, sans-serif;
  --font-family-mono: 'JetBrains Mono', monospace;
  
  --text-xs: 0.75rem;
  --text-sm: 0.875rem;
  --text-base: 1rem;
  --text-lg: 1.125rem;
  
  /* 간격 시스템 */
  --spacing-1: 0.25rem;
  --spacing-2: 0.5rem;
  --spacing-4: 1rem;
  --spacing-8: 2rem;
  
  /* 그림자 */
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1);
  --shadow-lg: 0 10px 15px -3px rgb(0 0 0 / 0.1);
  
  /* 둥근 모서리 */
  --radius-sm: 0.125rem;
  --radius-md: 0.375rem;
  --radius-lg: 0.5rem;
}
```

### 9.2 다크 테마 지원

```css
@media (prefers-color-scheme: dark) {
  :root {
    --bg-primary: #111827;
    --bg-secondary: #1f2937;
    --text-primary: #f9fafb;
    --text-secondary: #d1d5db;
    --border-color: #374151;
  }
}

/* 수동 테마 토글 */
[data-theme="dark"] {
  --bg-primary: #111827;
  --bg-secondary: #1f2937;
  --text-primary: #f9fafb;
  --text-secondary: #d1d5db;
  --border-color: #374151;
}
```

이 UI/UX 설계 문서는 ESTrace의 사용자 인터페이스 설계 원칙과 구현 방법을 종합적으로 다루며, 사용자 중심의 직관적이고 효율적인 인터페이스 구축을 위한 가이드를 제공합니다.