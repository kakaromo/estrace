<script lang="ts">    
  import * as Menubar from "$lib/components/ui/menubar";
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import * as Dialog from "$lib/components/ui/dialog";  
  import { AboutDialog, SettingDialog, BufferSizeDialog, PatternManagerDialog, PatternTesterDialog } from './menu';
  import Trace from './trace.svelte';
  import { getFolder } from "../api/db.js";
  import { traceFile, Status, traceStatusStore } from '../stores/file.js';
  import { clear } from 'idb-keyval';

  const macOS = navigator.userAgent.includes('Macintosh');
  let showAboutDialog = false;
  let showSettingsDialog = false;
  let showBuffersizeDialog = false;
  let showTraceDialog = false;
  let showPatternManagerDialog = false;
  let showPatternTesterDialog = false;

  async function handleFileOpen() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'All Files', extensions: ['*'] }
        ]
      });
      console.log('선택된 파일:', selected);
      traceFile.set(selected);
      traceStatusStore.set(Status.Opened);
    } catch (error) {
      console.error('파일 열기 실패:', error);
    }
  }

  onMount(async () => {
    let result = await getFolder();
    if(result.length === 0) {
      showSettingsDialog = true;
    }
  });
</script>

<Menubar.Root>
  <Menubar.Menu>
    <Menubar.Trigger>File</Menubar.Trigger>
    <Menubar.Content>
      <Menubar.Item on:click={() => {
        showTraceDialog = false;
        showTraceDialog = true;
        console.log('Open clicked, Trace dialog should open');
      }}>
        Open
      </Menubar.Item>
    </Menubar.Content>
  </Menubar.Menu>

  <Menubar.Menu>
    <Menubar.Trigger>Setting</Menubar.Trigger>
    <Menubar.Content>
      <Menubar.Item on:click={() => {
        showSettingsDialog = false;
        showSettingsDialog = true;
        console.log('App setting');
      }}>
        Setting
      </Menubar.Item>

      <Menubar.Item on:click={() => {
        showBuffersizeDialog = false;
        showBuffersizeDialog = true;
        console.log('Buffersize setting');
      }}>
        Buffer Size
      </Menubar.Item>

      <Menubar.Item on:click={() => {
        showPatternManagerDialog = false;
        showPatternManagerDialog = true;
        console.log('Pattern manager');
      }}>
        Pattern Manager
      </Menubar.Item>

      <Menubar.Item on:click={() => {
        showPatternTesterDialog = false;
        showPatternTesterDialog = true;
        console.log('Pattern tester');
      }}>
        Pattern Tester
      </Menubar.Item>

      <Menubar.Separator />

      <Menubar.Item on:click={() => {
        clear();
        console.log('session clear');
      }}>
        Session Clear
      </Menubar.Item>
    </Menubar.Content>
  </Menubar.Menu>

  <Menubar.Menu>
    <Menubar.Trigger>About</Menubar.Trigger>
    <Menubar.Content>
      <Menubar.Item on:click={() => {
        showAboutDialog = false;
        showAboutDialog = true;
        console.log('About clicked');
      }}>
        About
      </Menubar.Item>
    </Menubar.Content>
  </Menubar.Menu>
</Menubar.Root>

<SettingDialog dialogopen={showSettingsDialog} />
<BufferSizeDialog dialogopen={showBuffersizeDialog} />
<PatternManagerDialog dialogopen={showPatternManagerDialog} />
<PatternTesterDialog dialogopen={showPatternTesterDialog} />
<AboutDialog open={showAboutDialog}/>
<Trace dialogopen={showTraceDialog} />