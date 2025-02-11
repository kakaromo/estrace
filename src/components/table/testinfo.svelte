<script lang='ts'>
    import VirtualList from '@sveltejs/svelte-virtual-list';
    import { getAllTestInfo } from '../../api/db';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';

    import { Badge } from "$lib/components/ui/badge";
    import { Circle2 } from 'svelte-loading-spinners';

    interface TestInfo {
        id: number;
        title: string;
        content: string;
        logtype: string;
        logfolder: string;
        logname: string;
    }
    let testData:TestInfo[] = $state([]);
    let start: number = $state(0);
	let end: number = $state(0);
    let tracedata;
    let isLoading = $state(false);

    async function getTestData(id: number) {
        goto(`/detail/${id}`);
    }
    
    onMount(async () => {
        testData = await getAllTestInfo();
        console.log('testData:', testData);
    });
</script>
<div class="container font-sans">
    <h2
    class="scroll-m-20 border-b pb-2 text-3xl font-semibold tracking-tight transition-colors first:mt-0"
    >
        Trace Information
    </h2>
    {#if isLoading}
    <div class="spinner-overlay">
        <Circle2 color="#FF3E00" size="60" unit="px" />
    </div>
    {/if}
    
    <!-- 테이블 헤더 -->
    <div class="header grid grid-cols-[80px_500px_250px_150px] bg-gray-200 font-bold border-b border-gray-300">
        <div class="p-2">ID</div>
        <div class="p-2">Title</div>
        <!-- <div class="p-2">Content</div> -->
        <div class="p-2">Log Folder</div>
        <div class="p-2">Log File</div>
    </div>
    <div class="list-wrapper">
        <VirtualList items={testData} itemHeight={16} bind:start bind:end let:item>
            <div class="row grid grid-cols-[80px_500px_250px_150px] border-b border-gray-300" on:click={() =>  getTestData(item.id)}>
                <div class="content-item">{item.id}</div>
                <div class="content-item"><Badge variant="outline">{item.logtype}</Badge>{item.title}</div>
                <div class="content-item">{item.logfolder}</div>
                <div class="content-item">{item.logname}</div>                
            </div>
        </VirtualList>
    </div>
    <p class="footer p-2">showing {start}-{end} of {testData.length} rows</p>
</div>


<style>
    .container {
      display: flex;
      flex-direction: column;
      /* border-top: 1px solid #333;
      border-bottom: 1px solid #333; */
      min-height: 200px;
      height: calc(100vh - 15em);
      overflow: hidden;
      
    }
    .header {
      flex-shrink: 0;
      font-size: 13px;
    }
    .list-wrapper {
      flex-grow: 1;
      overflow-y: auto;
    }
    .footer {
      flex-shrink: 0;
      font-size: 11px;
    }
    .row {
        transition: background-color 0.2s ease-in-out;
    }
    .row:hover {
        background-color: #f0f0f0;
        cursor: pointer;
    }
    .content-item {
        @apply py-1 px-2;
        font-size: 11px;
    }
    /* Spinner overlay styling */
    .spinner-overlay {
        position: absolute;
        top: 0;
        left: 0;
        width: 100%;
        height: 100%;
        background: rgba(255, 255, 255, 0.8);
        display: flex;
        flex-direction: column;
        justify-content: center;
        align-items: center;
        z-index: 10;
    }
  </style>