<script lang="ts">
    import { onMount } from 'svelte';
    import { message } from "@tauri-apps/plugin-dialog";
    
    import { Button } from "$lib/components/ui/button";
    import * as Dialog from "$lib/components/ui/dialog";
    import { Input } from "$lib/components/ui/input";
    import { Label } from "$lib/components/ui/label";
    import { Textarea } from "$lib/components/ui/textarea";
    import * as Tabs from "$lib/components/ui/tabs";
    import * as Select from "$lib/components/ui/select";
    import { Badge } from "$lib/components/ui/badge";
    import { Loader2, Plus, Trash2, Save, Check, Edit } from 'lucide-svelte';
    
    import { getPatterns, getPatternsByTypeFromDb, addPattern, setActivePattern, deletePatternById, updateExistingPattern } from '$api/pattern';
    
    let { dialogopen } = $props();
    
    interface Pattern {
        id: number;
        name: string;
        type: string;
        pattern: string;
        description: string;
        is_active: boolean;
        created_at: string;
    }
    
    // UI state
    let ufsPatterns: Pattern[] = $state([]);
    let blockPatterns: Pattern[] = $state([]);
    let activeTab = $state('ufs');
    let isLoading = $state(false);
    let showAddDialog = $state(false);
    let showEditDialog = $state(false); 
    let showConfirmDialog = $state(false);
    let patternToDelete: Pattern | null = $state(null);
    let patternToEdit: Pattern | null = $state(null);
    
    // New/Edit pattern form values
    let newPatternName = $state('');
    let newPatternType = $state('ufs');
    let newPatternRegex = $state('');
    let newPatternDescription = $state('');
    
    // Load patterns
    async function loadPatterns() {
        isLoading = true;
        try {
            // Get all patterns from DB
            const patterns = await getPatterns();
            
            // Split patterns by type
            ufsPatterns = patterns.filter(p => p.type === 'ufs');
            blockPatterns = patterns.filter(p => p.type === 'block');
        } catch (error) {
            console.error('Error loading patterns:', error);
            await message('패턴 로딩 중 오류가 발생했습니다: ' + error);
        } finally {
            isLoading = false;
        }
    }
    
    // Add new pattern
    async function submitPattern() {
        isLoading = true;
        try {
            if (patternToEdit) {
                // Update existing pattern
                await updateExistingPattern(
                    patternToEdit.id,
                    newPatternName,
                    newPatternRegex,
                    newPatternDescription
                );
                await message('패턴이 성공적으로 수정되었습니다.');
                showEditDialog = false;
                patternToEdit = null;
            } else {
                // Add new pattern
                await addPattern(
                    newPatternName,
                    newPatternType,
                    newPatternRegex,
                    newPatternDescription
                );
                await message('패턴이 성공적으로 추가되었습니다.');
                showAddDialog = false;
            }
            
            // Clear form and reload patterns
            resetPatternForm();
            await loadPatterns();
        } catch (error) {
            console.error('Error with pattern operation:', error);
            await message('패턴 작업 중 오류가 발생했습니다: ' + error);
        } finally {
            isLoading = false;
        }
    }
    
    // Set active pattern
    async function markAsActive(patternId: number) {
        isLoading = true;
        try {
            await setActivePattern(patternId);
            
            // Reload patterns
            await loadPatterns();
            await message('패턴이 활성화되었습니다.');
        } catch (error) {
            console.error('Error setting active pattern:', error);
            await message('패턴 활성화 중 오류가 발생했습니다: ' + error);
        } finally {
            isLoading = false;
        }
    }
    
    // Delete pattern
    async function deletePattern() {
        if (!patternToDelete) return;
        
        isLoading = true;
        try {
            await deletePatternById(patternToDelete.id);
            
            // Close confirmation dialog
            showConfirmDialog = false;
            patternToDelete = null;
            
            // Reload patterns
            await loadPatterns();
            await message('패턴이 성공적으로 삭제되었습니다.');
        } catch (error) {
            console.error('Error deleting pattern:', error);
            await message('패턴 삭제 중 오류가 발생했습니다: ' + error);
        } finally {
            isLoading = false;
        }
    }
    
    // Open edit dialog
    function openEditDialog(pattern: Pattern) {
        patternToEdit = pattern;
        newPatternName = pattern.name;
        newPatternType = pattern.type;
        newPatternRegex = pattern.pattern;
        newPatternDescription = pattern.description || '';
        showEditDialog = true;
    }
    
    // Open delete confirmation dialog
    function confirmDelete(pattern: Pattern) {
        if (pattern.is_active) {
            message('활성 패턴은 삭제할 수 없습니다. 먼저 다른 패턴을 활성화하세요.');
            return;
        }
        
        patternToDelete = pattern;
        showConfirmDialog = true;
    }
    
    // Reset pattern form
    function resetPatternForm() {
        newPatternName = '';
        newPatternType = activeTab;
        newPatternRegex = '';
        newPatternDescription = '';
        patternToEdit = null;
    }
    
    // Open add pattern dialog
    function openAddDialog() {
        resetPatternForm();
        showAddDialog = true;
    }
    
    onMount(() => {
        loadPatterns();
    });
</script>

<Dialog.Root bind:open={dialogopen}>
    <Dialog.Content class="max-w-4xl">
        <Dialog.Header>
            <Dialog.Title>패턴 관리</Dialog.Title>
            <Dialog.Description>
                UFS와 Block 패턴을 관리합니다. 적절한 패턴을 활성화하여 로그 파싱에 사용할 수 있습니다.
            </Dialog.Description>
        </Dialog.Header>
        
        <div class="py-4">
            <Tabs.Root value={activeTab} onValueChange={(val) => activeTab = val} class="w-full">
                <Tabs.List class="grid grid-cols-2">
                    <Tabs.Trigger value="ufs">UFS 패턴</Tabs.Trigger>
                    <Tabs.Trigger value="block">Block 패턴</Tabs.Trigger>
                </Tabs.List>
                
                <Tabs.Content value="ufs" class="p-4 h-[400px] overflow-y-auto">
                    <div class="flex justify-between mb-4">
                        <h3 class="text-lg font-semibold">UFS 패턴 목록</h3>
                        <Button variant="outline" size="sm" onclick={openAddDialog}>
                            <Plus class="mr-1 h-4 w-4" />
                            새 패턴 추가
                        </Button>
                    </div>
                    
                    {#if isLoading}
                        <div class="flex justify-center items-center h-40">
                            <Loader2 class="h-8 w-8 animate-spin text-primary" />
                        </div>
                    {:else if ufsPatterns.length === 0}
                        <div class="text-center py-8 text-muted-foreground">
                            등록된 UFS 패턴이 없습니다.
                        </div>
                    {:else}
                        <div class="space-y-4">
                            {#each ufsPatterns as pattern}
                                <div class="border rounded-md p-4 transition-all hover:bg-muted/30">
                                    <div class="flex justify-between items-start">
                                        <div>
                                            <div class="flex items-center gap-2">
                                                <h4 class="font-medium">{pattern.name}</h4>
                                                {#if pattern.is_active}
                                                    <Badge variant="secondary" class="bg-green-100 text-green-800">활성</Badge>
                                                {/if}
                                            </div>
                                            <p class="text-xs text-muted-foreground mt-1">{pattern.description || ''}</p>
                                            <div class="mt-2 text-xs font-mono bg-muted p-2 rounded overflow-x-auto">
                                                {pattern.pattern}
                                            </div>
                                        </div>
                                        <div class="flex gap-2">
                                            {#if !pattern.is_active}
                                                <Button 
                                                    variant="outline" 
                                                    size="sm" 
                                                    onclick={() => markAsActive(pattern.id)}
                                                >
                                                    <Check class="mr-1 h-4 w-4" />
                                                    활성화
                                                </Button>
                                            {/if}
                                            <Button 
                                                variant="outline" 
                                                size="sm"
                                                onclick={() => openEditDialog(pattern)}
                                            >
                                                <Edit class="h-4 w-4" />
                                            </Button>
                                            {#if !pattern.is_active}
                                                <Button 
                                                    variant="outline" 
                                                    size="sm"
                                                    onclick={() => confirmDelete(pattern)}
                                                    class="text-destructive hover:bg-destructive/10"
                                                >
                                                    <Trash2 class="h-4 w-4" />
                                                </Button>
                                            {/if}
                                        </div>
                                    </div>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </Tabs.Content>
                
                <Tabs.Content value="block" class="p-4 h-[400px] overflow-y-auto">
                    <div class="flex justify-between mb-4">
                        <h3 class="text-lg font-semibold">Block 패턴 목록</h3>
                        <Button variant="outline" size="sm" onclick={openAddDialog}>
                            <Plus class="mr-1 h-4 w-4" />
                            새 패턴 추가
                        </Button>
                    </div>
                    
                    {#if isLoading}
                        <div class="flex justify-center items-center h-40">
                            <Loader2 class="h-8 w-8 animate-spin text-primary" />
                        </div>
                    {:else if blockPatterns.length === 0}
                        <div class="text-center py-8 text-muted-foreground">
                            등록된 Block 패턴이 없습니다.
                        </div>
                    {:else}
                        <div class="space-y-4">
                            {#each blockPatterns as pattern}
                                <div class="border rounded-md p-4 transition-all hover:bg-muted/30">
                                    <div class="flex justify-between items-start">
                                        <div>
                                            <div class="flex items-center gap-2">
                                                <h4 class="font-medium">{pattern.name}</h4>
                                                {#if pattern.is_active}
                                                    <Badge variant="secondary" class="bg-green-100 text-green-800">활성</Badge>
                                                {/if}
                                            </div>
                                            <p class="text-xs text-muted-foreground mt-1">{pattern.description || ''}</p>
                                            <div class="mt-2 text-xs font-mono bg-muted p-2 rounded overflow-x-auto">
                                                {pattern.pattern}
                                            </div>
                                        </div>
                                        <div class="flex gap-2">
                                            {#if !pattern.is_active}
                                                <Button 
                                                    variant="outline" 
                                                    size="sm" 
                                                    onclick={() => markAsActive(pattern.id)}
                                                >
                                                    <Check class="mr-1 h-4 w-4" />
                                                    활성화
                                                </Button>
                                            {/if}
                                            <Button 
                                                variant="outline" 
                                                size="sm"
                                                onclick={() => openEditDialog(pattern)}
                                            >
                                                <Edit class="h-4 w-4" />
                                            </Button>
                                            {#if !pattern.is_active}
                                                <Button 
                                                    variant="outline" 
                                                    size="sm"
                                                    onclick={() => confirmDelete(pattern)}
                                                    class="text-destructive hover:bg-destructive/10"
                                                >
                                                    <Trash2 class="h-4 w-4" />
                                                </Button>
                                            {/if}
                                        </div>
                                    </div>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </Tabs.Content>
            </Tabs.Root>
        </div>
        
        <Dialog.Footer>
            <Button variant="outline" onclick={() => dialogopen = false}>닫기</Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>

<!-- Add Pattern Dialog -->
<Dialog.Root bind:open={showAddDialog}>
    <Dialog.Content class="max-w-md">
        <Dialog.Header>
            <Dialog.Title>새 패턴 추가</Dialog.Title>
            <Dialog.Description>
                새로운 패턴을 추가하여 로그 파싱에 사용할 수 있습니다.
            </Dialog.Description>
        </Dialog.Header>
        
        <div class="space-y-4 py-4">
            <div class="grid grid-cols-4 items-center gap-4">
                <Label for="pattern-name" class="text-right">이름</Label>
                <Input id="pattern-name" bind:value={newPatternName} class="col-span-3" />
            </div>
            
            <div class="grid grid-cols-4 items-center gap-4">
                <Label for="pattern-type" class="text-right">유형</Label>
                <Select.Root 
                    value={newPatternType}
                    onSelectedChange={(v) => v && (newPatternType = v.value)} 
                    class="col-span-3"
                >
                    <Select.Trigger id="pattern-type" class="w-full">
                        <Select.Value placeholder="패턴 유형 선택" />
                    </Select.Trigger>
                    <Select.Content>
                        <Select.Item value="ufs">UFS</Select.Item>
                        <Select.Item value="block">Block</Select.Item>
                    </Select.Content>
                </Select.Root>
            </div>
            
            <div class="grid grid-cols-4 items-start gap-4">
                <Label for="pattern-regex" class="text-right pt-2">정규식</Label>
                <Textarea 
                    id="pattern-regex" 
                    bind:value={newPatternRegex} 
                    class="col-span-3 font-mono text-xs"
                    rows="6"
                />
            </div>
            
            <div class="grid grid-cols-4 items-start gap-4">
                <Label for="pattern-desc" class="text-right pt-2">설명</Label>
                <Textarea 
                    id="pattern-desc" 
                    bind:value={newPatternDescription} 
                    class="col-span-3"
                    rows="3"
                />
            </div>
        </div>
        
        <Dialog.Footer>
            <Button variant="outline" onclick={() => showAddDialog = false} class="mr-2">취소</Button>
            <Button 
                variant="default" 
                onclick={submitPattern}
                disabled={!newPatternName || !newPatternRegex || isLoading}
            >
                {#if isLoading}
                    <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                    처리 중...
                {:else}
                    <Save class="mr-2 h-4 w-4" />
                    저장
                {/if}
            </Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>

<!-- Edit Pattern Dialog -->
<Dialog.Root bind:open={showEditDialog}>
    <Dialog.Content class="max-w-md">
        <Dialog.Header>
            <Dialog.Title>패턴 수정</Dialog.Title>
            <Dialog.Description>
                패턴 정보를 수정합니다.
            </Dialog.Description>
        </Dialog.Header>
        
        <div class="space-y-4 py-4">
            <div class="grid grid-cols-4 items-center gap-4">
                <Label for="edit-pattern-name" class="text-right">이름</Label>
                <Input id="edit-pattern-name" bind:value={newPatternName} class="col-span-3" />
            </div>
            
            <div class="grid grid-cols-4 items-start gap-4">
                <Label for="edit-pattern-regex" class="text-right pt-2">정규식</Label>
                <Textarea 
                    id="edit-pattern-regex" 
                    bind:value={newPatternRegex} 
                    class="col-span-3 font-mono text-xs"
                    rows="6"
                />
            </div>
            
            <div class="grid grid-cols-4 items-start gap-4">
                <Label for="edit-pattern-desc" class="text-right pt-2">설명</Label>
                <Textarea 
                    id="edit-pattern-desc" 
                    bind:value={newPatternDescription} 
                    class="col-span-3"
                    rows="3"
                />
            </div>
        </div>
        
        <Dialog.Footer>
            <Button variant="outline" onclick={() => showEditDialog = false} class="mr-2">취소</Button>
            <Button 
                variant="default" 
                onclick={submitPattern}
                disabled={!newPatternName || !newPatternRegex || isLoading}
            >
                {#if isLoading}
                    <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                    처리 중...
                {:else}
                    <Save class="mr-2 h-4 w-4" />
                    업데이트
                {/if}
            </Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>

<!-- Confirm Delete Dialog -->
<Dialog.Root bind:open={showConfirmDialog}>
    <Dialog.Content class="max-w-md">
        <Dialog.Header>
            <Dialog.Title>패턴 삭제 확인</Dialog.Title>
            <Dialog.Description>
                패턴을 삭제하시겠습니까? 이 작업은 되돌릴 수 없습니다.
            </Dialog.Description>
        </Dialog.Header>
        
        {#if patternToDelete}
            <div class="py-4 px-2">
                <div class="font-medium mb-2">{patternToDelete.name}</div>
                <div class="text-xs font-mono bg-muted p-2 rounded overflow-x-auto mb-4">
                    {patternToDelete.pattern}
                </div>
            </div>
        {/if}
        
        <Dialog.Footer>
            <Button variant="outline" onclick={() => showConfirmDialog = false} class="mr-2">취소</Button>
            <Button 
                variant="destructive" 
                onclick={deletePattern}
                disabled={isLoading}
            >
                {#if isLoading}
                    <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                    처리 중...
                {:else}
                    <Trash2 class="mr-2 h-4 w-4" />
                    삭제
                {/if}
            </Button>
        </Dialog.Footer>
    </Dialog.Content>
</Dialog.Root>