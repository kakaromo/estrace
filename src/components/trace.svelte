<script lang='ts'>
    import { createEventDispatcher, onDestroy } from 'svelte';
    import { get, writable } from 'svelte/store';
    import { traceFile, Status, traceStatusStore } from '../stores/file.js';
    import { setting } from '../stores/setting.js';    
    import { setTestInfo } from '../api/db.js';
    
    import { Button } from "$lib/components/ui/button";
    import * as Dialog from "$lib/components/ui/dialog";
    import { Input } from "$lib/components/ui/input";
    import { Label } from "$lib/components/ui/label";
    import { Card } from "$lib/components/ui/card";
    import { Checkbox } from "$lib/components/ui/checkbox";  
    import { Textarea } from "$lib/components/ui/textarea";
    import { BarLoader } from 'svelte-loading-spinners';
    import { Reload } from "svelte-radix";
    
    import { invoke } from "@tauri-apps/api/core";
    import { message } from "@tauri-apps/plugin-dialog";
    import { open } from '@tauri-apps/plugin-dialog';

    let { dialogopen } = $props(); // 새로 추가됨
    const dispatch = createEventDispatcher();

    let logtype = $state('');
    let title = $state('');
    let logfolder = $state('');   
    let content = $state('');

    const unsubscribe = setting.subscribe(value => {
        logfolder = value.logfolder;
    });
    onDestroy(() => {
        unsubscribe();
    });

    async function handleFileOpen() {
        try {
            const selected = await open({
                multiple: false,
                filters: [
                { name: 'All Files', extensions: [] }
                ]        
            });
            console.log('선택된 파일:', selected);
            traceFile.set(selected);            
        } catch (error) {
        console.error('파일 열기 실패:', error);
        }
    }


    async function handleTraceStart() {        
        const fileName = get(traceFile);
        console.log('fileName:', fileName);
        if (!fileName) {            
            await message('Trace file이 지정되지 않았습니다.');
            traceStatusStore.set(Status.Idle);
            return;
        }

        if (!logtype) {
            await message('Test type이 지정되지 않았습니다.');            
            return;
        }

        if (!logfolder) {
            await message('Log folder가 지정되지 않았습니다.');            
            return;
        }        

        if (!content) {
            await message('Content가 지정되지 않았습니다.');            
            return;
        }

        try {
            traceStatusStore.set(Status.Loading);
            const parsed = await invoke<string>('starttrace', { fname: fileName, logfolder: logfolder });            
            console.log('Parsed Trace length:', parsed.length);
            console.log('Parsed Trace:', parsed);
            if (!parsed) {
                await message('Trace가 실패하였습니다.');
                traceStatusStore.set(Status.Idle);
                dialogopen = false; // 핸들러 종료 후 dialog off
                return;
            }
            let filename = parsed.ufs_parquet_filename 
            if (parsed.block_parquet_filename) {
                filename = filename + "," + parsed.block_parquet_filename;
            }
            await setTestInfo(logtype, title, content, logfolder, filename);
            await message('Trace가 성공적으로 완료되었습니다.');
        } catch (error) {
            console.error('starttrace 호출 오류:', error);
            await message('Trace가 실패하였습니다.');
            dialogopen = false; // 핸들러 종료 후 dialog off
            dispatch('close'); // 핸들러 종료 후 dialog off
        } finally {
            traceStatusStore.set(Status.Idle);
            dialogopen = false; // 핸들러 종료 후 dialog off
            dispatch('close'); // 핸들러 종료 후 dialog off
        }
    }
</script>
<Dialog.Root bind:open={dialogopen}>
    
    <Dialog.DialogContent>
        <Dialog.Header>
            <Dialog.Title>Trace</Dialog.Title>        
        </Dialog.Header>
        <div class="p-4">
            <div class="space-y-2">
                <div class="grid grid-cols-4 items-center gap-4">
                <Label for="trace-file">Trace File</Label>
                <Input id="trace-file" type="text" value={$traceFile} readonly class="col-span-3" onclick={handleFileOpen}/>
                </div>
                <div class="grid grid-cols-4 items-center gap-4">
                <Label for="logfolder">Log folder</Label>
                <Input id="logfolder" bind:value={logfolder} class="col-span-3" />
                </div>
                <div class="grid grid-cols-4 items-center gap-4">
                <Label for="logtype">Log Type</Label>
                <Input id="logtype" bind:value={logtype} class="col-span-3" />
                </div>
                <div class="grid grid-cols-4 items-center gap-4">
                <Label for="title">Title</Label>
                <Input id="title" bind:value={title} class="col-span-3" />
                </div>
                <div class="grid w-full gap-1.5">
                <Label for="content">Content</Label>
                <Textarea id="content" bind:value={content}  />
                </div>            
            </div>
        </div>
        
        <Dialog.Footer>
            <div class="flex justify-end items-center">
            {#if $traceStatusStore === Status.Loading}
                <div style="width: 100%;">
                <BarLoader color="#FF3E00" unit="px" />
                </div>
            {/if}
            
            <Button type="submit" disabled={$traceStatusStore === Status.Loading}  onclick={handleTraceStart}>
            {#if $traceStatusStore === Status.Loading}
                <Reload class="mr-2 h-4 w-4 animate-spin" />
                Tracing...
            {:else}
                Trace
            {/if}
            </Button>
            </div>
    </Dialog.Footer>
</Dialog.DialogContent>
</Dialog.Root>