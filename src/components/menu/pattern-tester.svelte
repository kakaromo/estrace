<script lang="ts">
    import { onMount } from 'svelte';
    import { invoke } from "@tauri-apps/api/core";
    import { message } from "@tauri-apps/plugin-dialog";
    
    import { Button } from "$lib/components/ui/button";
    import * as Dialog from "$lib/components/ui/dialog";
    import { Textarea } from "$lib/components/ui/textarea";
    import { Label } from "$lib/components/ui/label";
    import * as Tabs from "$lib/components/ui/tabs";
    import * as Select from "$lib/components/ui/select";
    import { Input } from "$lib/components/ui/input";
    import { Badge } from "$lib/components/ui/badge";
    import { Loader2, Play, Save, Clipboard, CheckCircle2, XCircle } from 'lucide-svelte';
    
    import { addPattern } from '$api/pattern';
    
    let { dialogopen } = $props();
    
    // UI 상태
    let sampleText = $state('');
    let regexPattern = $state('');
    let patternName = $state('');
    let patternDescription = $state('');
    let patternType = $state('ufs');
    let isLoading = $state(false);
    let isTestLoading = $state(false);
    let lastTestSuccess = $state<boolean | null>(null);
    let matchResults = $state<any[] | null>(null);
    let errorMessage = $state('');
    
    // 테스트 결과 컬럼 정보
    let resultColumns = $state<string[]>([]);
    
    // 클립보드에서 붙여넣기 함수
    async function pasteFromClipboard() {
        try {
            const text = await navigator.clipboard.readText();
            sampleText = text;
        } catch (error) {
            console.error('Failed to read from clipboard:', error);
            errorMessage = '클립보드에서 텍스트를 가져오는데 실패했습니다.';
        }
    }
    
    // 정규식 테스트 함수
    async function testRegex() {
        if (!sampleText || !regexPattern) {
            errorMessage = '샘플 텍스트와 정규식 패턴을 모두 입력해주세요.';
            return;
        }
        
        errorMessage = '';
        isTestLoading = true;
        lastTestSuccess = null;
        matchResults = null;
        resultColumns = [];
        
        try {
            // Rust 백엔드에 정규식 테스트 요청
            const result = await invoke<string>('test_regex_pattern', {
                text: sampleText,
                pattern: regexPattern
            });
            
            // 결과 파싱
            const parsedResult = JSON.parse(result);
            
            if (parsedResult.success) {
                lastTestSuccess = true;
                
                // 결과가 있는 경우 처리
                if (parsedResult.matches && parsedResult.matches.length > 0) {
                    matchResults = parsedResult.matches;
                    
                    // 첫 번째 매치 결과에서 모든 컬럼 추출
                    if (matchResults[0].groups) {
                        resultColumns = Object.keys(matchResults[0].groups);
                    } else if (matchResults[0].captures) {
                        resultColumns = Array.from(
                            { length: matchResults[0].captures.length }, 
                            (_, i) => i === 0 ? 'full_match' : `group_${i}`
                        );
                    }
                } else {
                    // 매치는 성공했으나 결과가 없는 경우
                    errorMessage = '매치는 성공했으나 결과가 없습니다. 샘플 텍스트를 확인해주세요.';
                }
            } else {
                lastTestSuccess = false;
                errorMessage = parsedResult.error || '정규식 패턴이 잘못되었습니다.';
            }
        } catch (error) {
            console.error('Error testing regex:', error);
            lastTestSuccess = false;
            errorMessage = '정규식 테스트 중 오류가 발생했습니다: ' + error;
        } finally {
            isTestLoading = false;
        }
    }
    
    // 패턴 저장 함수
    async function savePattern() {
        if (!patternName || !regexPattern) {
            errorMessage = '패턴 이름과 정규식을 입력해주세요.';
            return;
        }
        
        if (!lastTestSuccess) {
            errorMessage = '성공적으로 테스트된 패턴만 저장할 수 있습니다. 먼저 테스트해주세요.';
            return;
        }
        
        isLoading = true;
        errorMessage = '';
        
        try {
            await addPattern(
                patternName,
                patternType,
                regexPattern,
                patternDescription
            );
            
            await message('패턴이 성공적으로 저장되었습니다.');
            
            // 입력 필드 초기화
            patternName = '';
            patternDescription = '';
            lastTestSuccess = null;
            matchResults = null;
            
            // 다이얼로그 닫기 (옵션)
            // dialogopen = false;
        } catch (error) {
            console.error('Error saving pattern:', error);
            errorMessage = '패턴 저장 중 오류가 발생했습니다: ' + error;
        } finally {
            isLoading = false;
        }
    }
    
    // 테스트 결과 행 색상 지정
    function getRowClass(index: number) {
        return index % 2 === 0 ? 'bg-gray-50' : 'bg-white';
    }
    
    // 패턴 타입 변경 시 이름 업데이트 (선택사항)
    $effect(() => {
        if (!patternName || patternName.startsWith('UFS ') || patternName.startsWith('Block ')) {
            patternName = `${patternType.charAt(0).toUpperCase() + patternType.slice(1)} Pattern`;
        }
    });
</script>

<Dialog.Root bind:open={dialogopen}>
    <Dialog.Content class="max-w-5xl">
        <Dialog.Header>
            <Dialog.Title>패턴 테스터</Dialog.Title>
            <Dialog.Description>
                정규식 패턴을 테스트하고 결과를 확인한 후 저장할 수 있습니다.
            </Dialog.Description>
        </Dialog.Header>
        
        <div class="py-4 grid grid-cols-1 md:grid-cols-2 gap-4">
            <!-- 왼쪽: 입력 영역 -->
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <Label for="sample-text">샘플 텍스트</Label>
                    <Button 
                        variant="outline" 
                        size="sm"
                        onclick={pasteFromClipboard}
                        title="클립보드에서 붙여넣기"
                    >
                        <Clipboard class="h-4 w-4 mr-1" />
                        붙여넣기
                    </Button>
                </div>
                
                <Textarea 
                    id="sample-text" 
                    bind:value={sampleText}
                    placeholder="여기에 테스트할 로그 라인을 붙여넣으세요."
                    rows="8"
                    class="font-mono text-xs"
                />
                
                <Label for="regex-pattern">정규식 패턴</Label>
                <div class="relative">
                    <Textarea 
                        id="regex-pattern" 
                        bind:value={regexPattern}
                        placeholder="정규식 패턴을 입력하세요."
                        rows="4"
                        class="font-mono text-xs pr-16"
                    />
                    <Button 
                        variant="secondary"
                        size="sm"
                        class="absolute right-2 bottom-2"
                        onclick={testRegex}
                        disabled={isTestLoading || !sampleText || !regexPattern}
                    >
                        {#if isTestLoading}
                            <Loader2 class="h-4 w-4 mr-1 animate-spin" />
                            테스트 중...
                        {:else}
                            <Play class="h-4 w-4 mr-1" />
                            테스트
                        {/if}
                    </Button>
                </div>
                
                <!-- 패턴 저장 폼 -->
                <div class="border rounded-md p-4 space-y-4">
                    <h3 class="text-sm font-medium">패턴 저장</h3>
                    
                    <div class="grid grid-cols-4 gap-2 items-center">
                        <Label for="pattern-type" class="text-right text-xs">유형</Label>
                        <Select.Root 
                            value={patternType}
                            onSelectedChange={(v) => v && (patternType = v.value)} 
                            class="col-span-3"
                        >
                            <Select.Trigger id="pattern-type" class="h-8 text-xs">
                                <Select.Value placeholder="패턴 유형 선택" />
                            </Select.Trigger>
                            <Select.Content>
                                <Select.Item value="ufs">UFS</Select.Item>
                                <Select.Item value="block">Block</Select.Item>
                            </Select.Content>
                        </Select.Root>
                    </div>
                    
                    <div class="grid grid-cols-4 gap-2 items-center">
                        <Label for="pattern-name" class="text-right text-xs">이름</Label>
                        <Input 
                            id="pattern-name" 
                            bind:value={patternName} 
                            class="col-span-3 h-8 text-xs"
                            placeholder="패턴 이름"
                        />
                    </div>
                    
                    <div class="grid grid-cols-4 gap-2 items-start">
                        <Label for="pattern-desc" class="text-right text-xs pt-2">설명</Label>
                        <Textarea 
                            id="pattern-desc" 
                            bind:value={patternDescription} 
                            placeholder="패턴 설명 (선택사항)"
                            class="col-span-3 text-xs"
                            rows="2"
                        />
                    </div>
                    
                    <div class="flex justify-end">
                        <Button 
                            variant="default" 
                            size="sm"
                            onclick={savePattern}
                            disabled={isLoading || !lastTestSuccess || !patternName}
                        >
                            {#if isLoading}
                                <Loader2 class="h-4 w-4 mr-1 animate-spin" />
                                저장 중...
                            {:else}
                                <Save class="h-4 w-4 mr-1" />
                                패턴 저장
                            {/if}
                        </Button>
                    </div>
                </div>
            </div>
            
            <!-- 오른쪽: 결과 영역 -->
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <Label>매칭 결과</Label>
                    
                    {#if lastTestSuccess !== null}
                        <div class="flex items-center">
                            {#if lastTestSuccess}
                                <CheckCircle2 class="h-4 w-4 text-green-500 mr-1" />
                                <span class="text-xs text-green-600">정규식 매칭 성공</span>
                            {:else}
                                <XCircle class="h-4 w-4 text-red-500 mr-1" />
                                <span class="text-xs text-red-600">정규식 매칭 실패</span>
                            {/if}
                        </div>
                    {/if}
                </div>
                
                {#if errorMessage}
                    <div class="bg-red-50 border border-red-200 text-red-600 rounded-md p-3 text-xs">
                        {errorMessage}
                    </div>
                {/if}
                
                {#if matchResults && matchResults.length > 0}
                    <div class="border rounded-md overflow-hidden">
                        <div class="overflow-x-auto max-h-[400px]">
                            <table class="w-full text-xs">
                                <thead class="bg-gray-100">
                                    <tr>
                                        <th class="px-2 py-1 text-left font-medium text-gray-600">Line</th>
                                        
                                        {#if matchResults[0].groups}
                                            {#each resultColumns as column}
                                                <th class="px-2 py-1 text-left font-medium text-gray-600">{column}</th>
                                            {/each}
                                        {:else if matchResults[0].captures}
                                            {#each resultColumns as column}
                                                <th class="px-2 py-1 text-left font-medium text-gray-600">{column}</th>
                                            {/each}
                                        {/if}
                                    </tr>
                                </thead>
                                <tbody>
                                    {#each matchResults as match, index}
                                        <tr class={getRowClass(index)}>
                                            <td class="px-2 py-1 font-medium">{index + 1}</td>
                                            
                                            {#if match.groups}
                                                {#each resultColumns as column}
                                                    <td class="px-2 py-1 font-mono">{match.groups[column] || ''}</td>
                                                {/each}
                                            {:else if match.captures}
                                                {#each match.captures as capture, i}
                                                    <td class="px-2 py-1 font-mono">{capture || ''}</td>
                                                {/each}
                                            {/if}
                                        </tr>
                                    {/each}
                                </tbody>
                            </table>
                        </div>
                    </div>
                    
                    <div class="text-xs text-muted-foreground">
                        총 {matchResults.length}개의 매치 결과가 있습니다.
                    </div>
                {:else if lastTestSuccess}
                    <div class="bg-yellow-50 border border-yellow-200 text-yellow-600 rounded-md p-3 text-xs">
                        정규식은 유효하지만, 매치된 결과가 없습니다. 패턴 또는 샘플 텍스트를 확인해주세요.
                    </div>
                {:else if !lastTestSuccess && !errorMessage}
                    <div class="bg-gray-50 border border-gray-200 text-gray-600 rounded-md p-3 text-xs">
                        정규식 패턴을 테스트하면 여기에 결과가 표시됩니다.
                    </div>
                {/if}
            </div>
        </div>
        
        <Dialog.Footer>
            <Button variant="outline" onclick={() => dialogopen = false}>닫기</Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>