<script lang='ts'>
    import VirtualList from '@sveltejs/svelte-virtual-list';
    import { getAllTestInfo } from '$api/db';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';

    import { Badge } from "$lib/components/ui/badge";
    import { Circle2 } from 'svelte-loading-spinners';
    import Separator from '$lib/components/ui/separator/separator.svelte';
    import { traceStatusStore, Status } from '$stores/file';
    import { VirtualTable, type Column } from '$lib/components/ui/virtual-table';


    interface TestInfo {
        id: number;
        title: string;
        content: string;
        logtype: string;
        logfolder: string;
        logname: string;
    }
    let testData:TestInfo[] = $state([]);
    // let start: number = $state(0);
	// let end: number = $state(0);
    let isLoading = $state(false);

    // 테이블 컬럼 정의
    const columns: Column<TestInfo>[] = [
        { 
            header: 'ID', 
            accessor: 'id', 
            width: '80px' 
        },
        { 
            header: 'Title', 
            accessor: (item) => `${item.title}`,
            width: '500px',
            cell: (item) => `<span class="badge-container"><span class="badge badge-outline">${item.logtype}</span> ${item.title}</span>`
        },
        { 
            header: 'Log Folder', 
            accessor: 'logfolder', 
            width: '250px' 
        },
        { 
            header: 'Log File', 
            accessor: 'logname', 
            width: '150px' 
        }
    ];

    function handleRowClick(event) {
        // rowClick 이벤트에서 item 객체 추출
        const item = event.detail.item;
        
        // item에서 id 추출하고 페이지 이동
        if (item && item.id) {
            goto(`/detail/${item.id}`);
        }
    }
    // trace 성공하면 table update
    $effect(() => {
        if ($traceStatusStore === Status.Success) {
            console.log('Trace was successful, updating test data table...');
            isLoading = true;
            void (async () => {
                try {
                    testData = await getAllTestInfo();
                    console.log('Test data updated:', testData);
                } catch (error) {
                    console.error('Failed to update test data:', error);
                } finally {
                    isLoading = false;
                }
            })();
        }
    });
    
    onMount(async () => {
        isLoading = true;
        try {
            testData = await getAllTestInfo();
        } catch (error) {
            console.error('Error loading test data:', error);
        } finally {
            isLoading = false;
        }
    });
</script>
<div class="container font-sans">
    <div class="space-y-1">
        <div>
            <h3 class="text-lg font-medium">
                Trace Information
            </h3>
            <p class="text-muted-foreground text-sm">Configure how you receive notifications.</p>
        </div>
        <Separator class="my-4" />
    </div>
    {#if isLoading}
    <div class="spinner-overlay">
        <Circle2 color="#FF3E00" size="60" unit="px" />
    </div>
    {/if}
    
    <!-- 테이블 헤더 -->
    <!-- <div class="header grid grid-cols-[80px_500px_250px_150px] bg-gray-200 font-bold border-b border-gray-300 ">
        <div class="p-2">ID</div>
        <div class="p-2">Title</div>
        <div class="p-2">Content</div>
        <div class="p-2">Log Folder</div>
        <div class="p-2">Log File</div>
    </div>
    <div class="list-wrapper">
        <VirtualList items={testData} itemHeight={16} bind:start bind:end let:item>
            <div class="row grid grid-cols-[80px_500px_250px_150px] border-b border-gray-300" on:click={() =>  getTestData(item.id)}>
                <div class="py-1 px-2 content-item">{item.id}</div>
                <div class="py-1 px-2 content-item"><Badge variant="badge-outline">{item.logtype}</Badge> {item.title}</div>
                <div class="py-1 px-2 content-item">{item.logfolder}</div>
                <div class="py-1 px-2 content-item">{item.logname}</div>                
            </div>
        </VirtualList>
    </div>
    <p class="footer p-2">showing {start}-{end} of {testData.length} rows</p>  -->
    <div class="table-container">
        <VirtualTable 
            items={testData} 
            columns={columns} 
            itemHeight={36}
            hoverable={true}
            on:rowClick={handleRowClick}
        />
    </div>
</div>


<style>
    .container {
      display: flex;
      flex-direction: column;      
      width: 100%;
      height: 100vh;
      overflow: hidden;      
    }
    .table-container {
        flex-grow: 1;
        overflow: hidden;
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