<script lang="ts">
  import { get, writable } from 'svelte/store';
  import { Plus, Minus, Settings } from "svelte-lucide"
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Card } from "$lib/components/ui/card";
  import { setting } from "../../stores/setting";
  import { onMount } from 'svelte';
  import { getFolder, setFolder } from "../../api/db.js";
  import { cleanupTempArrowFiles } from "../../api/cleanup";

  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';

  interface App {
    name: string;
    filename: string;
    isNew?: boolean;
  }

  // let appsfolder = $state("");
  // let logfolder = setting.subscribe(value => value.logfolder);
  let logfolder = $state('');  
  // const appsStore = writable<App[]>([]);

  let { dialogopen } = $props();
  const closeDialog = () => {
    dialogopen = false;
  };

  async function handleFileOpen() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        filters: [
          { name: 'All Files', extensions: [] }
        ]        
      });
      console.log('선택된 파일:', selected);
      logfolder = selected;
    } catch (error) {
      console.error('파일 열기 실패:', error);
    }
  }

  onMount(async () => {
    await getFolder();
    if(get(setting).logfolder) {
      logfolder = get(setting).logfolder;
    }
  });
  
  // // 초기 앱 목록 설정


  // // 새 앱 입력 필드 추가
  // function addNewInputField() {
  //   appsStore.update(apps => [
  //     ...apps,
  //     { name: '', filename: '', isNew: true }
  //   ]);
  // }

  // // 앱 삭제
  // function removeApp(index: number) {
  //   appsStore.update(apps => 
  //     apps.filter((_, i) => i !== index)
  //   );
  // }

  // // 앱 입력 완료 처리
  // function handleInputComplete(index: number) {
  //   appsStore.update(apps => {
  //     const updatedApps = [...apps];
  //     if (updatedApps[index].name && updatedApps[index].filename) {
  //       updatedApps[index].isNew = false;
  //     }
  //     return updatedApps;
  //   });
  // }

  // 엔터 키 처리
  function handleKeydown(event: KeyboardEvent, index: number) {
    if (event.key === 'Enter') {
      handleInputComplete(index);
    }
  }
  async function saveSetting() {
    if(!logfolder || logfolder.trim() === '') {
      window.alert('please set log folder.');
      console.log('logfolder is empty');
      return;
    }
    await setFolder('logfolder', logfolder);
    closeDialog();
  }

  // 캐시 초기화 함수
  async function clearCache() {
    try {
      const result = await invoke('clear_all_cache');
      console.log('Cache cleared:', result);
      window.alert('캐시가 성공적으로 초기화되었습니다.\n' + result);
    } catch (error) {
      console.error('Cache clear failed:', error);
      window.alert('캐시 초기화에 실패했습니다: ' + error);
    }
  }

  // 임시 파일 정리 함수
  async function cleanupTempFiles() {
    try {
      const count = await cleanupTempArrowFiles(24);
      if (count > 0) {
        window.alert(`✅ ${count}개의 임시 파일이 삭제되었습니다.`);
      } else {
        window.alert('ℹ️ 정리할 임시 파일이 없습니다.');
      }
    } catch (error) {
      console.error('Temp file cleanup failed:', error);
      window.alert('임시 파일 정리에 실패했습니다: ' + error);
    }
  }
</script>

<Dialog.Root bind:open={dialogopen}>
    <!-- <Dialog.Trigger>
        <Button variant="outline" size="icon">
            <Settings class="h-4 w-4" />
          </Button>
    </Dialog.Trigger> -->
    <Dialog.Content class="sm:max-w-[650px]">
      <Dialog.Header>
        <Dialog.Title>Setting</Dialog.Title>
        <Dialog.Description>
          Please specify the location of the log folder here. Click save when you're done.
        </Dialog.Description>
      </Dialog.Header>
      <div class="grid gap-4 py-2">
        <!-- <div class="grid grid-cols-4 items-center gap-4">
          <Label for="appsfolder">Apps Folder</Label>
          <Input id="appsfolder" bind:value={appsfolder} class="col-span-3" />
        </div> -->
        <div class="grid grid-cols-4 items-center gap-4">
          <Label for="logfolder">Log Folder</Label>
          <Input id="logfolder" bind:value={logfolder} class="col-span-3" onclick={handleFileOpen} />
        </div>
        
        <!-- 캐시 및 파일 관리 섹션 -->
        <div class="grid grid-cols-4 items-center gap-4 pt-4 border-t">
          <Label>Data Management</Label>
          <div class="col-span-3 space-y-2">
            <Button variant="outline" onclick={clearCache} class="w-full">
              캐시 초기화 (Clear Cache)
            </Button>
            <p class="text-sm text-gray-500">
              캐시를 초기화하여 최신 데이터를 다시 로드합니다.
            </p>
            
            <Button variant="outline" onclick={cleanupTempFiles} class="w-full mt-2">
              임시 파일 정리 (Clean Temp Files)
            </Button>
            <p class="text-sm text-gray-500">
              24시간 이상 된 임시 Arrow 파일을 삭제합니다.
            </p>
          </div>
        </div>
        <!-- <div class="flex items-center">
          <Label>Apps</Label>
          <Button variant="outline" size="icon" class="h-8 w-8" on:click={addNewInputField}>
            <Plus />
          </Button>
        </div>      
        <Card class="p-4 border-dashed">
          <div class="space-y-2">
            {#each $appsStore as app, index}
              <div class="flex items-center justify-between p-2 hover:bg-gray-100 rounded-md">
                <div class="flex items-center space-x-4 flex-1">
                  {#if app.isNew}
                    <Input
                      class="text-left"
                      placeholder="앱 이름"
                      bind:value={app.name}
                      on:keydown={(e) => handleKeydown(e, index)}
                    />
                    <Input
                      placeholder="파일명"
                      bind:value={app.filename}
                      on:keydown={(e) => handleKeydown(e, index)}
                      on:blur={() => handleInputComplete(index)}
                    />
                  {:else}
                    <div class="w-32 px-4 py-2 border border-gray-200 rounded-md">
                      {app.name}
                    </div>
                    <div class="px-4 py-2 border border-gray-200 rounded-md">
                      {app.filename}
                    </div>
                  {/if}
                </div>
                <Button 
                  variant="outline" 
                  size="icon" 
                  class="h-8 w-8 bg-blue-500 hover:bg-blue-600"
                  on:click={() => removeApp(index)}
                >
                  <Minus class="h-4 w-4 text-white" />
                </Button>
              </div>
            {/each}
          </div>
        </Card>
      </div> -->
      <Dialog.Footer>
        <Button type="submit" onclick={saveSetting}>Save changes</Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
  
