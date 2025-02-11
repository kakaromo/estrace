import { writable}  from 'svelte/store';
// enum 정의 (문자열 형태로도 사용 가능)
export enum Status {
    Idle = 'idle',
    Opened = 'opened',
    Loading = 'loading',
    Success = 'success',
    Error = 'error'
  }
export const traceFile = writable<string>("");
export const traceStatusStore = writable<Status>(Status.Idle);
