import { writable, derived } from "svelte/store";

export const trace = writable({});
export const filtertracedata = writable({});
export const selectedTrace = writable<string>('');
export const prevselectedTrace = writable<string>('');

// trace size count
export const traceSizeCount = writable();
export const traceSaimpleCount = writable();

// trace size count 비교 (같지 않으면 parquet 파일에서 읽어와 filter 진행, 
// 없으면 filtertracedata에서 filter 진행)
export const compareTraceCount = derived(
  [traceSizeCount, traceSaimpleCount],
  ([$traceSizeCount, $traceSaimpleCount]) => {
    return $traceSizeCount === $traceSaimpleCount;
  }
);

// 이전 filtertrace 값을 저장할 store
export const prevFilterTrace = writable({
  zoom_column: 'dtoc',
  from_time: 0.0,
  to_time: 0.0,
  from_lba: 0.0,
  to_lba: 0.0
});

// filtertrace store
export const filtertrace = writable({
  zoom_column: 'dtoc',
  from_time: 0.0,
  to_time: 0.0,
  from_lba: 0.0,
  to_lba: 0.0
});

// filtertrace가 변경되었는지 확인하는 derived store
export const filtertraceChanged = derived(
  [filtertrace, prevFilterTrace],
  ([$filtertrace, $prevFilterTrace]) => {
    return JSON.stringify($filtertrace) !== JSON.stringify($prevFilterTrace);
  }
);

export const filterselectedTraceChanged = derived(
  [selectedTrace, prevselectedTrace],
  ([$selectedTrace, $prevselectedTrace]) => {
    return JSON.stringify($selectedTrace) !== JSON.stringify($prevselectedTrace);
  }
);
export const testinfoid = writable(0);

export type TestInfo = {
  id: number;
  logtype: string;
  title: string;
  content: string;
  logfolder: string;
  logname: string;
  sourcelog_path: string;
};


export function initialTraceData() {
  trace.set({});
  filtertracedata.set({});
  selectedTrace.set('');
  prevselectedTrace.set('');
  traceSizeCount.set(0);
  traceSaimpleCount.set(0);
  prevFilterTrace.set({
    zoom_column: 'dtoc',
    from_time: 0.0, 
    to_time: 0.0,
    from_lba: 0.0,
    to_lba: 0.0
  });
  filtertrace.set({
    zoom_column: 'dtoc',
    from_time: 0.0,
    to_time: 0.0,
    from_lba: 0.0,
    to_lba: 0.0
  });
  testinfoid.set(0);
}
