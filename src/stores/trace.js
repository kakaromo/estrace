import { writable } from "svelte/store";

export const trace = writable([]);
export const selectedTrace = writable('');