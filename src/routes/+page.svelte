<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { AppMenu, Trace } from "../components/index.js"
  import { TestInfo } from "../components/table/index.js";
  import { onMount } from 'svelte';
  import { initial, getAllTestInfo } from "../api/db.js";
  import { Status, traceStatusStore } from '../stores/file.js';
  import { clear } from 'idb-keyval'

  let name = $state("");
  let greetMsg = $state("");

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
    await initial();   
    await getAllTestInfo();  
  });

  
</script>

<div class="hedden md:block">
  <AppMenu />
  <TestInfo />
</div>


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
