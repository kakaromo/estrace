# UFSCUSTOM 구현 완료

## 구현 내용

UFSCUSTOM 형태를 estrace 프로젝트에 추가했습니다. GitHub의 다른 프로젝트(kakaromo/trace)를 참고하여 UFS, Block과 동일한 구조로 구현했습니다.

### 추가된 파일

1. **src-tauri/src/trace/ufscustom.rs** - 새로운 파일
   - `ufscustom_bottom_half_latency_process()`: QD, CTOC, CTOD, continuous 후처리
   - `ufscustom_to_record_batch()`: Arrow RecordBatch 변환
   - `save_ufscustom_to_parquet()`: Parquet 파일 저장

### 수정된 파일

1. **src-tauri/src/trace/types.rs**
   - UFSCUSTOM 구조체 추가:
     ```rust
     pub struct UFSCUSTOM {
         pub opcode: String,
         pub lba: u64,
         pub size: u32,
         pub start_time: f64,
         pub end_time: f64,
         pub dtoc: f64,
         pub start_qd: u32,     // Queue Depth at request start
         pub end_qd: u32,       // Queue Depth at request end
         pub ctoc: f64,         // Complete to Complete latency (ms)
         pub ctod: f64,         // Complete to Dispatch latency (ms)
         pub continuous: bool,  // 연속적인 요청 여부
     }
     ```

2. **src-tauri/src/trace/mod.rs**
   - ufscustom 모듈 추가
   - UFSCUSTOM_CACHE 추가
   - UFSCUSTOM_PATTERNS 추가
   - ACTIVE_UFSCUSTOM_PATTERN 정규식 패턴 추가:
     ```
     ^(?P<opcode>0x[0-9a-f]+),(?P<lba>\d+),(?P<size>\d+),(?P<start_time>\d+(?:\.\d+)?),(?P<end_time>\d+(?:\.\d+)?)$
     ```

3. **src-tauri/src/trace/utils.rs**
   - UFSCUSTOM import 추가
   - `parse_ufscustom_trace_with_caps()` 파싱 함수 추가
   - `starttrace()` 함수에서:
     - UFSCUSTOM 패턴 로드
     - 라인별 UFSCUSTOM 파싱 로직 추가
     - UFSCUSTOM 후처리 (QD, CTOC, CTOD, continuous 계산)
     - UFSCUSTOM Parquet 파일 저장

### 파싱 형식

로그 파일에서 다음 패턴을 찾아 UFSCUSTOM으로 파싱합니다:

```
0x2a,1887,2,141036.565121,141036.565317
0x2a,4748,17,141036.565161,141036.565423
0x2a,4748,104,141036.565191,141036.565471
```

각 필드:
- opcode: 0x로 시작하는 16진수
- lba: LBA 주소
- size: 크기
- start_time: 시작 시간 (초 단위, 소수점 가능)
- end_time: 종료 시간 (초 단위, 소수점 가능)

### 후처리 로직

1. **dtoc 계산**: (end_time - start_time) * 1000 (파싱 시 계산)

2. **QD (Queue Depth) 계산**:
   - 모든 요청의 start_time과 end_time을 이벤트로 변환
   - 시간순으로 정렬하여 QD 계산
   - start_qd: 요청 시작 시점의 QD
   - end_qd: 요청 완료 시점의 QD

3. **CTOC (Complete to Complete)** 계산:
   - 이전 요청의 end_time에서 현재 요청의 end_time까지의 시간 (밀리초)

4. **CTOD (Complete to Dispatch)** 계산:
   - start_qd가 1인 경우: 이전 QD=0 완료 시점에서 현재 시작까지
   - 그 외: 이전 완료 시점에서 현재 시작까지

5. **continuous** 판단:
   - 이전 요청의 (lba + size) == 현재 요청의 lba
   - AND 이전 opcode == 현재 opcode

### 출력

처리된 UFSCUSTOM 데이터는 `{logfolder}/{basename}_ufscustom.parquet` 형식으로 저장됩니다.

## 사용 방법

1. 로그 파일에 UFSCUSTOM 형식의 라인이 포함되어 있으면 자동으로 파싱됩니다.
2. UFS, Block과 함께 처리되며, 각각 독립적인 Parquet 파일로 저장됩니다.
3. 파싱, 후처리, 저장 과정에서 진행 상황이 콘솔과 UI에 표시됩니다.

## 참고

구현은 https://github.com/kakaromo/trace 프로젝트의 UFSCUSTOM 처리 로직을 참고했습니다.
