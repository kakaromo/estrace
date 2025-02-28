use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Serialize, Debug, Clone)]
pub struct UFS {
    pub time: f64,
    pub process: String,
    pub cpu: u32,
    pub action: String,
    pub tag: u32,
    pub opcode: String,
    pub lba: u64,
    pub size: u32,
    pub groupid: u32,
    pub hwqid: u32,
    pub qd: u32,   // Queue Depth
    pub dtoc: f64, // Device to Complete latency
    pub ctoc: f64, // Complete to Complete latency
    pub ctod: f64, // Complete to Device latency
    pub continuous: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct Block {
    pub time: f64,
    pub process: String,
    pub cpu: u32,
    pub flags: String,
    pub action: String,
    pub devmajor: u32,
    pub devminor: u32,
    pub io_type: String,
    pub extra: u32,
    pub sector: u64,
    pub size: u32,
    pub comm: String,
    pub qd: u32,   // Queue Depth
    pub dtoc: f64, // Device to Complete latency
    pub ctoc: f64, // Complete to Complete latency
    pub ctod: f64, // Complete to Device latency
    pub continuous: bool,
}

#[derive(Serialize, Debug, Clone)]
pub enum LatencyValue {
    F64(f64),
    U32(u32),
    U64(u64),
}

impl LatencyValue {
    // filtering 용도로 f64 값으로 변환 (u32, u64는 f64로 변환)
    pub fn as_f64(&self) -> f64 {
        match *self {
            LatencyValue::F64(v) => v,
            LatencyValue::U32(v) => v as f64,
            LatencyValue::U64(v) => v as f64,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct LatencyStat {
    pub time: f64,
    pub opcode: String,
    pub value: LatencyValue,
}

#[derive(Serialize, Debug, Clone)]
pub struct TraceParseResult {
    pub missing_lines: Vec<usize>,
    pub ufs_parquet_filename: String,
    pub block_parquet_filename: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct LatencySummary {
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub median: f64,
    pub std_dev: f64,
    pub percentiles: HashMap<String, f64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LatencyStats {
    pub latency_counts: BTreeMap<String, BTreeMap<String, usize>>,
    pub summary: Option<BTreeMap<String, LatencySummary>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SizeStats {
    pub opcode_stats: BTreeMap<String, BTreeMap<u32, usize>>,
    pub total_counts: BTreeMap<String, usize>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ContinuityStats {
    pub op_stats: BTreeMap<String, ContinuityCount>,
    pub total: TotalContinuity,
}

#[derive(Serialize, Debug, Clone)]
pub struct ContinuityCount {
    pub continuous: usize,     // 연속적인 요청 수
    pub non_continuous: usize, // 비연속적인 요청 수
    pub ratio: f64,            // 연속적인 요청의 비율
    pub total_bytes: u64,      // 전체 처리된 바이트 수
    pub continuous_bytes: u64, // 연속 요청으로 처리된 바이트 수
    pub bytes_ratio: f64,      // 연속 바이트 비율
}

#[derive(Serialize, Debug, Clone)]
pub struct TotalContinuity {
    pub total_requests: usize,      // 전체 요청 수
    pub continuous_requests: usize, // 연속적인 요청 수
    pub overall_ratio: f64,         // 전체 연속 비율
    pub total_bytes: u64,           // 전체 바이트 수
    pub continuous_bytes: u64,      // 연속 바이트 수
    pub bytes_ratio: f64,           // 연속 바이트 비율
}
