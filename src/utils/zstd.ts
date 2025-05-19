import { invoke } from "@tauri-apps/api/core";

export async function decompress(data: Uint8Array): Promise<Uint8Array> {
  const result: number[] = await invoke("decompress_zstd", { data: Array.from(data) });
  return new Uint8Array(result);
}