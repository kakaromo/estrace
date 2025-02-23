import { writable } from "svelte/store";

export const trace = writable([]);
export const selectedTrace = writable('');
export const filtertrace = writable({
    from_time: 0.0,
    to_time: 0.0,
    from_lba: 0.0,
    to_lba: 0.0
  });