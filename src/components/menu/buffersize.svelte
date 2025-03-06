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
    import { getBufferSize, setBufferSize } from "../../api/db.js";
  
    import { open } from '@tauri-apps/plugin-dialog';
  
    interface App {
      name: string;
      filename: string;
      isNew?: boolean;
    }
  
    // let appsfolder = $state("");
    // let logfolder = setting.subscribe(value => value.logfolder);
    let buffersize = $state(0);  
    // const appsStore = writable<App[]>([]);
  
    let { dialogopen } = $props();
    const closeDialog = () => {
      dialogopen = false;
    };
    
    // Handle input to ensure only positive numbers are accepted
    function handleInput(event) {
        const inputValue = event.target.value;
        
        // Convert to number or 0 if empty
        let numValue = inputValue === '' ? 0 : Number(inputValue);
        
        // Ensure the value is not negative
        if (numValue < 0) {
        numValue = 0;
        event.target.value = numValue;
        }
        
        buffersize = numValue;
    }
    
    // Prevent negative sign from being typed
    function handleKeyDown(event) {
        if (event.key === '-' || event.key === 'e') {
        event.preventDefault();
        }
    }
  
    onMount(async () => {
        buffersize = await getBufferSize();
    });
  
    async function saveSetting() {
      await setBufferSize(buffersize);
      closeDialog();
    }
  </script>
  
  <Dialog.Root bind:open={dialogopen}>
      <Dialog.Content class="sm:max-w-[650px]">
        <Dialog.Header>
          <Dialog.Title>Setting</Dialog.Title>
          <Dialog.Description>
            Please specify the location of the log folder here. Click save when you're done.
          </Dialog.Description>
        </Dialog.Header>
        <div class="grid gap-4 py-2">
          <div class="grid grid-cols-4 items-center gap-4">
            <Label for="logfolder">buffersize</Label>
            <Input type="number" id="logfolder" bind:value={buffersize} oninput={handleInput} onkeydown={handleKeyDown} class="col-span-3" />
          </div>          
        <Dialog.Footer>
          <Button type="submit" onclick={saveSetting}>Save changes</Button>
        </Dialog.Footer>
      </Dialog.Content>
    </Dialog.Root>
    
  