<script lang="ts">
    import { page } from '$app/stores';
    import { onMount } from 'svelte';
    import { invoke } from "@tauri-apps/api/core";
    
    import { getTestInfo } from '../../../api/db';

    import { Circle2 } from 'svelte-loading-spinners';
    import { StepBack } from 'svelte-lucide';
    import { Button } from "$lib/components/ui/button";

    let id: number;
    let data = $state({});
    let tracedata = $state([]);
    let isLoading = $state(false);

    // $page.params를 통해 동적 파라미터 id 값을 가져옵니다.
    id = $page.params.id;

    onMount(async () => {
        isLoading = true;
        data = await getTestInfo(id);   
        tracedata = await invoke<string>('readtrace', { logfolder: data.logfolder, logname: data.logname });
        console.log('tracedata:', tracedata);
        isLoading = false;
    });

    function goBack() {
        window.history.back();
    }
</script>
<div class="hidden md:block">
{#if isLoading}
    <div class="spinner-overlay">
        <Circle2 color="#FF3E00" size="60" unit="px" />
    </div>
{/if}


<Button href="/" class="fixed top-4 right-4" variant="primary">
    <StepBack size="24" />
    Back
</Button>
</div>


<style>
    .spinner-overlay {
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
    }
</style>