# ESTrace 설치 방법

ESTrace는 Tauri 기반 애플리케이션으로, 여러 플랫폼에서 실행할 수 있습니다. 아래는 개발 환경 설정부터 애플리케이션 설치까지의 전체 과정입니다.

## 필수 조건

ESTrace를 설치하거나 개발하기 전에 다음 도구가 설치되어 있어야 합니다:

### 모든 플랫폼 공통

- [Node.js](https://nodejs.org/) 16.x 이상
- [Rust](https://www.rust-lang.org/tools/install) 1.60 이상
- [Git](https://git-scm.com/downloads)

### Windows 전용 요구사항

- Microsoft Visual Studio C++ 빌드 도구
- WebView2 런타임

### macOS 전용 요구사항

- Xcode 커맨드 라인 도구: `xcode-select --install`

### Linux 전용 요구사항

- 기본 개발 도구: `sudo apt install build-essential`
- WebKit2GTK: `sudo apt install libwebkit2gtk-4.0-dev`
- 기타 의존성: `sudo apt install libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`

## 애플리케이션 다운로드 및 설치

### 미리 빌드된 패키지 사용 (일반 사용자)

1. [공식 웹사이트](https://your-website.com/download) 또는 [GitHub 릴리즈 페이지](https://github.com/kakaromo/estrace/releases)에서 운영체제에 맞는 설치 패키지를 다운로드합니다.

2. 플랫폼별 설치 방법:
   - **Windows**: `.msi` 또는 `.exe` 파일을 실행하고 설치 마법사를 따릅니다.
   - **macOS**: `.dmg` 파일을 열고 애플리케이션을 Applications 폴더로 드래그합니다.
   - **Linux**: `.deb`, `.rpm`, `.AppImage` 중 하나를 다운로드하고 시스템에 맞게 설치합니다.

### 소스 코드에서 빌드 (개발자)

1. 저장소 클론:
   ```bash
   git clone https://github.com/kakaromo/estrace.git
   cd estrace
   ```

2. 의존성 설치:
   ```bash
   npm install
   ```

3. 개발 모드에서 실행:
   ```bash
   npm run tauri-dev
   ```

4. 애플리케이션 빌드:
   ```bash
   npm run tauri build
   ```
   빌드된 애플리케이션은 `src-tauri/target/release` 폴더에서 찾을 수 있습니다.

## 초기 설정

### Android 디바이스 설정

ESTrace를 Android 디바이스와 함께 사용하려면:

1. 개발자 옵션 활성화:
   - 설정 -> 휴대전화 정보 -> 소프트웨어 정보 -> 빌드 번호 (7번 탭)
   - 설정 -> 시스템 -> 개발자 옵션 -> USB 디버깅 활성화

2. ADB 설정:
   - [Android SDK Platform Tools](https://developer.android.com/studio/releases/platform-tools)를 다운로드하고 설치
   - 시스템 환경 변수에 ADB 경로 추가

### 애플리케이션 권한

ESTrace는 처음 실행할 때 다음과 같은 권한을 요청할 수 있습니다:

- 파일 시스템 접근 (테스트 결과 저장 및 로드)
- 네트워크 접근 (ADB 네트워크 연결 사용 시)
- 프로세스 실행 (외부 도구 연동)

필요한 권한을 허용하여 모든 기능을 사용할 수 있도록 합니다.

## 문제 해결

설치 중 문제가 발생하면 [문제 해결](./troubleshooting.md) 페이지를 참조하거나 [GitHub 이슈](https://github.com/kakaromo/estrace/issues)를 통해 문의해주세요.

## 다음 단계

설치가 완료되었다면 [사용법](./usage.md) 문서를 참조하여 ESTrace를 시작하세요.
