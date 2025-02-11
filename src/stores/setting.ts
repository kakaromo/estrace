import { writable } from "svelte/store";

interface App {
    name: string;
    filename: string;
    isNew?: boolean;
  }

export const setting = writable({
    // appsfolder: "",
    logfolder: "",
    // apps: [] as App[],
});