<script lang='ts'>
    import { createEventDispatcher, onDestroy, onMount } from 'svelte';
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
    
    // 경과 시간 관련 변수
    let elapsedSeconds = $state(0);
    let timerInterval = $state(null);

    const unsubscribe = setting.subscribe(value => {
        logfolder = value.logfolder;
    });
    
    // 타이머 시작 함수
    function startTimer() {
        // 타이머가 이미 실행 중이면 초기화
        if (timerInterval) {
            clearInterval(timerInterval);
        }
        
        elapsedSeconds = 0;
        const startTime = Date.now();
        
        timerInterval = setInterval(() => {
            // 현재 시간과 시작 시간의 차이를 초 단위로 계산
            elapsedSeconds = Math.floor((Date.now() - startTime) / 1000);
        }, 1000); // 1초마다 업데이트
    }
    
    // 타이머 중지 함수
    function stopTimer() {
        if (timerInterval) {
            clearInterval(timerInterval);
            timerInterval = null;
        }
    }
    
    // 경과 시간을 포맷팅하는 함수
    function formatElapsedTime(seconds) {
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    
    onDestroy(() => {
        unsubscribe();
        stopTimer(); // 컴포넌트 소멸 시 타이머 정리
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
            startTimer(); // 타이머 시작
            
            const parsed = await invoke<string>('starttrace', { fname: fileName, logfolder: logfolder });            
            console.log('Parsed Trace length:', parsed.length);
            console.log('Parsed Trace:', parsed);
            
            stopTimer(); // 타이머 중지
            
            if (!parsed) {
                await message('Trace가 실패하였습니다.');
                traceStatusStore.set(Status.Idle);
                dialogopen = false; // 핸들러 종료 후 dialog off
                return;
            }
            
            // Save test info and get the ID
            let filename = parsed.ufs_parquet_filename;
            if (parsed.block_parquet_filename) {
                filename = filename + "," + parsed.block_parquet_filename;
            }
            
            // Save with source log path included
            await setTestInfo(logtype, title, content, logfolder, filename, fileName);
            
            await message(`Trace가 성공적으로 완료되었습니다. (총 소요시간: ${formatElapsedTime(elapsedSeconds)})`);
            traceStatusStore.set(Status.Success); // 상태를 명확히 Success로 설정
            // 잠시 대기 후 다이얼로그 닫기 (UI 업데이트 시간 확보)
            setTimeout(() => {
                dialogopen = false; 
                dispatch('close');
            }, 500);
        } catch (error) {
            stopTimer(); // 타이머 중지
            console.error('starttrace 호출 오류:', error);
            await message('Trace가 실패하였습니다.');
            dialogopen = false; // 핸들러 종료 후 dialog off
            dispatch('close'); // 핸들러 종료 후 dialog off
        } finally {
            stopTimer(); // 타이머 중지
            // 오류 발생 시에만 상태 변경 (성공 시엔 위에서 이미 Success로 설정)
            if ($traceStatusStore !== Status.Success) {
                traceStatusStore.set(Status.Idle);
            }
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
            <div class="flex justify-end items-center w-full">
            {#if $traceStatusStore === Status.Loading}
                <div class="w-full flex flex-col items-center">
                    <div class="mt-2 text-sm font-medium">
                        처리 중... 경과 시간: {formatElapsedTime(elapsedSeconds)}
                    </div>
                </div>
            {/if}
            
            <Button type="submit" disabled={$traceStatusStore === Status.Loading} onclick={handleTraceStart}>
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