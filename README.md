# ESTrace - Performance Trace tool

ESTrace는 Tauri, SvelteKit, TypeScript를 활용한 데스크톱 애플리케이션으로, Android에서 Block,UFS I/O Performance Trace 및 분석 기능을 제공합니다.

## 주요 기능

- Android에서 Block,UFS I/O Performance Trace 및 모니터링
- CPU, 메모리, I/O 액세스 패턴 분석
- Latency 통계 및 시각화
- 패턴 테스트 및 관리
- 데이터 내보내기 및 공유

## 시작하기

### 필수 조건

- [Node.js](https://nodejs.org/) (16.x 이상)  
- [Rust](https://www.rust-lang.org/tools/install) (1.60 이상)
- [VS Code](https://code.visualstudio.com/) (권장 IDE)

### 설치

```bash
# 의존성 설치
npm install

# 개발 모드에서 실행
npm run tauri-dev

# 애플리케이션 빌드
npm run tauri build
```

## 개발 워크플로우

- `npm run dev`: Vite 개발 서버 실행
- `npm run check`: TypeScript type check 실행
- `npm run tauri`: Tauri CLI 명령 실행
- `npm run tauri-dev`: 파일 변경 감지 없이 Tauri 개발 서버 실행(db로 이 명령어로 실행해야 함 - tool 무한 refresh 방지)

## 프로젝트 구조

- `/src` - SvelteKit frontend
  - `/components` - UI 컴포넌트
  - `/routes` - 페이지 라우팅
  - `/stores` - Svelte 전역 상태 변수 관리
  - `/utils` - 유틸리티 함수
- `/src-tauri` - Rust backend
  - `/src/trace` - Trace 기능 모듈

## 기술 스택

- **frontend**: SvelteKit, TypeScript, Tailwind CSS
- **backend**: Rust, Tauri
- **데이터 시각화**: ECharts, Plotly.js
- **상태 관리**: Svelte 전역 상태 변수 관리


## 권장 개발 환경

[VS Code](https://code.visualstudio.com/)에 다음 확장 프로그램을 설치하는 것을 권장합니다:
- [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) 
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) 
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
- [Tailwind CSS IntelliSense](https://marketplace.visualstudio.com/items?itemName=bradlc.vscode-tailwindcss)
