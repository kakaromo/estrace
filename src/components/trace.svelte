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
    import { Ban } from 'lucide-svelte';
    import { Progress } from "$lib/components/ui/progress";
    
    import { invoke } from "@tauri-apps/api/core";
    import { message } from "@tauri-apps/plugin-dialog";
    import { open } from '@tauri-apps/plugin-dialog';
    import { listen } from '@tauri-apps/api/event';

    let { dialogopen } = $props(); // 새로 추가됨
    const dispatch = createEventDispatcher();

    // dialogopen이 변경될 때마다 실행되는 $effect
    $effect(() => {
        // 다이얼로그가 닫힐 때 상태 초기화
        if (!dialogopen && $traceStatusStore === Status.Loading) {
            resetDialogState();
            invoke('cancel_trace_process').catch(err => {
                console.error('작업 취소 실패:', err);
            });
        }
    });

    let logtype = $state('');
    let title = $state('');
    let logfolder = $state('');   
    let content = $state('');
    
    // 경과 시간 관련 변수
    let elapsedSeconds = $state(0);
    let timerInterval = $state(null);

    // 진행 상태 관련 변수
    let progressValue = $state(0);
    let progressStage = $state('');
    let progressMessage = $state('');
    let processingSpeed = $state(0);
    let remainingTime = $state(0);
    let currentItem = $state(0);
    let totalItems = $state(0);
    let showDetails = $state(false);
    
    // 이벤트 리스너 해제 함수
    let unlisten = null;
    
    // 작업 중단/재시작 관련 변수
    let isCancelled = $state(false);
    let isRestarting = $state(false);

    const unsubscribe = setting.subscribe(value => {
        logfolder = value.logfolder;
    });
    
    // 진행 상태 이벤트 리스너 설정
    onMount(async () => {
        unlisten = await listen('trace-progress', (event) => {
            const progress = event.payload;
            
            progressStage = progress.stage;
            progressValue = progress.progress;
            progressMessage = progress.message;
            processingSpeed = progress.processing_speed;
            remainingTime = progress.eta_seconds;
            currentItem = progress.current;
            totalItems = progress.total;
            
            // 진행 상태가 완료되면 타이머 중지
            if (progressStage === 'complete') {
                stopTimer();
            }
        });
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
    
    // 남은 시간을 포맷팅하는 함수
    function formatRemainingTime(seconds) {
        if (!seconds || seconds <= 0) return '00:00';
        
        const mins = Math.floor(seconds / 60);
        const secs = Math.floor(seconds % 60);
        return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }
    
    // 처리 속도를 포맷팅하는 함수
    function formatSpeed(speed) {
        if (!speed || speed <= 0) return '0 항목/초';
        
        if (speed > 1000000) {
            return `${(speed / 1000000).toFixed(2)}M 항목/초`;
        } else if (speed > 1000) {
            return `${(speed / 1000).toFixed(2)}K 항목/초`;
        } else {
            return `${speed.toFixed(2)} 항목/초`;
        }
    }
    
    // 진행 단계에 따른 메시지
    function getStageDescription(stage) {
        switch(stage) {
            case 'init': return '초기화';
            case 'reading': return '파일 읽기';
            case 'parsing': return '로그 파싱';
            case 'latency': return '지연 시간 계산';
            case 'saving': return '파일 저장';
            case 'complete': return '완료';
            default: return stage;
        }
    }
    
    // 작업 취소 함수
    async function cancelTrace() {
        if (confirm('정말 현재 처리 작업을 취소하시겠습니까?')) {
            try {
                isCancelled = true;
                await invoke('cancel_trace_process');
                progressMessage = "사용자에 의해 작업이 취소되었습니다...";
            } catch (error) {
                console.error('작업 취소 실패:', error);
            }
        }
    }
    
    // 작업 재시작 함수
    async function restartTrace() {
        try {
            isRestarting = true;
            
            // 취소 신호 초기화
            await invoke('reset_cancel_signal');
            
            // 상태 초기화
            isCancelled = false;
            progressValue = 0;
            progressStage = '';
            progressMessage = '';
            
            // 작업 재시작
            await handleTraceStart();
            
        } catch (error) {
            console.error('작업 재시작 실패:', error);
            await message('작업 재시작에 실패했습니다.');
        } finally {
            isRestarting = false;
        }
    }
    
    // 다이얼로그 닫기 함수 (상태 초기화 포함)
    function closeDialog() {
        // 작업이 진행 중이면 취소할지 확인
        if ($traceStatusStore === Status.Loading && !isCancelled) {
            if (confirm('작업이 진행 중입니다. 취소하고 닫으시겠습니까?')) {
                invoke('cancel_trace_process').catch(err => {
                    console.error('작업 취소 실패:', err);
                });
            } else {
                return; // 사용자가 취소하면 다이얼로그를 닫지 않음
            }
        }
        
        // 타이머와 이벤트 리스너 정리
        stopTimer();
        
        // 상태 초기화
        resetDialogState();
        
        // 다이얼로그 닫기
        dialogopen = false;
        dispatch('close');
    }
    
    // 다이얼로그 상태 초기화 함수
    function resetDialogState() {
        // 입력 필드 초기화
        if (!logfolder) {
            logfolder = get(setting).logfolder || '';
        }
        logtype = '';
        title = '';
        content = '';
        
        // 진행 상태 관련 변수 초기화
        progressValue = 0;
        progressStage = '';
        progressMessage = '';
        processingSpeed = 0;
        remainingTime = 0;
        currentItem = 0;
        totalItems = 0;
        showDetails = false;
        isCancelled = false;
        isRestarting = false;
        
        // 앱 상태 초기화 (성공한 경우가 아니면)
        if ($traceStatusStore !== Status.Success) {
            traceStatusStore.set(Status.Idle);
        }
    }
    
    onDestroy(() => {
        unsubscribe();
        stopTimer(); // 컴포넌트 소멸 시 타이머 정리
        
        // 이벤트 리스너 정리
        if (unlisten) {
            unlisten();
        }
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
            
            // 진행 상태 초기화
            progressValue = 0;
            progressStage = 'init';
            progressMessage = '초기화 중...';
            isCancelled = false;
            
            const parsed = await invoke('starttrace', { 
                fname: fileName, 
                logfolder: logfolder
            });            
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
            
            // 사용자 취소인 경우 메시지 표시하지 않음
            if (isCancelled) {
                progressMessage = "사용자에 의해 작업이 취소되었습니다.";
            } else {
                await message('Trace가 실패하였습니다: ' + error);
                dialogopen = false; // 핸들러 종료 후 dialog off
                dispatch('close'); // 핸들러 종료 후 dialog off
            }
        } finally {
            stopTimer(); // 타이머 중지
            // 오류 발생 시에만 상태 변경 (성공 시엔 위에서 이미 Success로 설정)
            if ($traceStatusStore !== Status.Success && !isCancelled) {
                traceStatusStore.set(Status.Idle);
            }
        }
    }
</script>
<Dialog.Root bind:open={dialogopen}>
    
    <Dialog.DialogContent class="sm:max-w-[700px]">
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
        
        {#if $traceStatusStore === Status.Loading}
            <div class="px-4 pb-2">
                <div class="mb-2">
                    <div class="flex justify-between items-center mb-1">
                        <span class="text-sm font-medium">진행 상태: {getStageDescription(progressStage)}</span>
                        <span class="text-sm">{progressValue.toFixed(1)}%</span>
                    </div>
                    <Progress value={progressValue} />
                </div>
                
                <div class="text-sm mb-4 text-gray-600">
                    {progressMessage}
                </div>
                
                <div class="flex justify-between items-center mb-1">
                    <button 
                        class="text-xs text-blue-600 hover:text-blue-800 underline"
                        on:click={() => showDetails = !showDetails}
                    >
                        {showDetails ? '상세 정보 숨기기' : '상세 정보 보기'}
                    </button>
                    <span class="text-sm">경과 시간: {formatElapsedTime(elapsedSeconds)}</span>
                </div>
                
                {#if showDetails}
                    <div class="bg-gray-50 p-2 rounded text-xs space-y-1 mb-2">
                        <div class="flex justify-between">
                            <span>처리 항목:</span>
                            <span>{currentItem.toLocaleString()} / {totalItems.toLocaleString()}</span>
                        </div>
                        <div class="flex justify-between">
                            <span>처리 속도:</span>
                            <span>{formatSpeed(processingSpeed)}</span>
                        </div>
                        <div class="flex justify-between">
                            <span>예상 완료까지:</span>
                            <span>{formatRemainingTime(remainingTime)}</span>
                        </div>
                    </div>
                {/if}
                
                {#if isCancelled}
                    <div class="text-center my-2">
                        <p class="text-amber-600 mb-2">작업이 취소되었습니다.</p>
                        <Button 
                            variant="outline" 
                            class="w-full" 
                            onclick={restartTrace}
                            disabled={isRestarting}
                        >
                            {#if isRestarting}
                                <Reload class="mr-2 h-4 w-4 animate-spin" />
                                재시작 중...
                            {:else}
                                작업 재시작
                            {/if}
                        </Button>
                    </div>
                {/if}
            </div>
        {/if}
        
        <Dialog.Footer>
            <div class="flex justify-between items-center w-full">
                {#if $traceStatusStore === Status.Loading && !isCancelled}
                    <Button 
                        variant="destructive" 
                        class="mr-auto" 
                        onclick={cancelTrace}
                    >
                        <Ban class="mr-2 h-4 w-4" />
                        취소
                    </Button>
                {:else}
                    <Button 
                        variant="outline" 
                        onclick={closeDialog}
                        class="mr-auto"
                    >
                        닫기
                    </Button>
                {/if}
                
                <Button 
                    type="submit" 
                    disabled={$traceStatusStore === Status.Loading && !isCancelled} 
                    onclick={handleTraceStart}
                >
                    {#if $traceStatusStore === Status.Loading && !isCancelled}
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