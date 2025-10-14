<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { AppMenu, Trace } from "../components/index.js"
  import { TestInfo } from "../components/table/index.js";
  import { onMount } from 'svelte';
  import { initial, getAllTestInfo } from "../api/db.js";
  import { Status, traceStatusStore } from '../stores/file.js';
  import { clear } from 'idb-keyval';
  import { syncPatterns } from '../api/pattern.js';
  import { autoCleanupOnStartup } from '../api/cleanup.js';

  let name = $state("");
  let greetMsg = $state("");
  let isInitializing = $state(true);

  async function greet(event: Event) {
    event.preventDefault();
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    greetMsg = await invoke("greet", { name });
  }

  onMount(async () => {
    // 프로그램 실행 시 단 한 번만 idb-keyval clear를 실행
    if (!(window as any).__idbCleared) {
      await clear();
      (window as any).__idbCleared = true;
    }

    try {
      // Initialize the database
      await initial();
      
      // Sync patterns from DB to Rust backend
      await syncPatterns();
      
      // Load test info
      await getAllTestInfo();
      
      // 자동으로 오래된 임시 파일 정리 (백그라운드에서 실행)
      autoCleanupOnStartup().catch(err => {
        console.warn('자동 임시 파일 정리 실패 (무시됨):', err);
      });
    } catch (error) {
      console.error('Error during initialization:', error);
    } finally {
      isInitializing = false;
    }
  });
</script>

{#if isInitializing}
  <div class="flex justify-center items-center h-screen">
    <div class="flex flex-col items-center">
      <div class="animate-spin h-10 w-10 border-4 border-blue-500 rounded-full border-t-transparent"></div>
      <p class="mt-4 text-gray-700">애플리케이션을 초기화하는 중...</p>
    </div>
  </div>
{:else}
  <div class="hedden md:block">
    <AppMenu />
    <TestInfo />
  </div>
{/if}


<!-- <main class="container">  
  <h1>Welcome to Tauri + Svelte</h1>

  <div class="row">
    <a href="https://vitejs.dev" target="_blank">
      <img src="/vite.svg" class="logo vite" alt="Vite Logo" />
    </a>
    <a href="https://tauri.app" target="_blank">
      <img src="/tauri.svg" class="logo tauri" alt="Tauri Logo" />
    </a>
    <a href="https://kit.svelte.dev" target="_blank">
      <img src="/svelte.svg" class="logo svelte-kit" alt="SvelteKit Logo" />
    </a>
  </div>
  <p>Click on the Tauri, Vite, and SvelteKit logos to learn more.</p>

  <form class="row" onsubmit={greet}>
    <input id="greet-input" placeholder="Enter a name..." bind:value={name} />
    <button type="submit">Greet</button>
  </form>
  <p>{greetMsg}</p>
</main> -->

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}
</style>
