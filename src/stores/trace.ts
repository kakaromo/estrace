import { writable, derived } from "svelte/store";

export const trace = writable([]);
export const filtertracedata = writable([]);
export const selectedTrace = writable('');
export const prevselectedTrace = writable('');
// 이전 filtertrace 값을 저장할 store
export const prevFilterTrace = writable({
  zoom_column: 'lba',
  from_time: 0.0,
  to_time: 0.0,
  from_lba: 0.0,
  to_lba: 0.0
});

// filtertrace store
export const filtertrace = writable({
  zoom_column: 'lba',
  from_time: 0.0,
  to_time: 0.0,
  from_lba: 0.0,
  to_lba: 0.0
});

// filtertrace가 변경되었는지 확인하는 derived store
export const filtertraceChanged = derived(
  [filtertrace, prevFilterTrace],
  ([$filtertrace, $prevFilterTrace]) => {
    if (JSON.stringify($filtertrace) !== JSON.stringify($prevFilterTrace)) {
      console.log('filtertrace changed');
    } else {
      console.log('filtertrace not changed');
    }
    return JSON.stringify($filtertrace) !== JSON.stringify($prevFilterTrace);
  }
);

export const filterselectedTraceChanged = derived(
  [selectedTrace, prevselectedTrace],
  ([$selectedTrace, $prevselectedTrace]) => {
    return JSON.stringify($selectedTrace) !== JSON.stringify($prevselectedTrace);
  }
);
export const currentValue = writable(false);