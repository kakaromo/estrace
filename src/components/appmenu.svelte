<script lang="ts">    
  import { onMount } from 'svelte';
  
  import { Menu, Submenu } from '@tauri-apps/api/menu'
  import { open } from '@tauri-apps/plugin-dialog';
  
  import * as Dialog from "$lib/components/ui/dialog";  
  // import SettingDialog from './menu/setting.svelte';
  import { AboutDialog, SettingDialog, BufferSizeDialog, PatternManagerDialog, PatternTesterDialog } from './menu';
  import Trace from './trace.svelte'; 
  import { getFolder } from "../api/db.js";
  import { traceFile, Status, traceStatusStore } from '../stores/file.js';

  import { clear } from 'idb-keyval'

  const macOS = navigator.userAgent.includes('Macintosh')
  let showTraceDialog = false; 
  let showAboutDialog = false;
  let showSettingsDialog = false;
  let showBuffersizeDialog = false;  
  let showPatternManagerDialog = false; 
  let showPatternTesterDialog = false; 

  async function menu() {
    const about = await Submenu.new({
      text: 'About',
      items: [
        {
          text: 'About',
          action: () => {
            showAboutDialog = false;
            showAboutDialog = true;
            console.log('About clicked');
          }
        }
      ]
    })

    const filemenu = await Submenu.new({
      text: "File",
      items: [
        {
          text: "Open",
          action: () => {
            showTraceDialog = false;
            showTraceDialog = true;
            console.log('Open clicked, Trace dialog should open');
          }
        },
      ]
    })

    const settingmenu = await Submenu.new({
      text: "Setting",
      items: [
        {
          text: "Setting",
          action: () => {
            showSettingsDialog = false;
            showSettingsDialog = true;
            console.log('App setting');
          }
        },
        {
          text: "Buffer Size",
          action: () => {
            showBuffersizeDialog = false;
            showBuffersizeDialog = true;
            console.log('Buffersize setting');
          }
        },
        {
          text: "Pattern Manager",
          action: () => {
            showPatternManagerDialog = false;
            showPatternManagerDialog = true;
            console.log('Pattern manager');
          }
        },
        {
          text: "Pattern Tester",
          action: () => {
            showPatternTesterDialog = false;
            showPatternTesterDialog = true;
            console.log('Pattern tester');
          }
        },
        {
          text: "Session Clear",
          action: () => {
            clear();
            console.log('session clear');
          }
        },
      ]
    })
    const menu = await Menu.new({
      items: [ filemenu, settingmenu, about]
    })
    await (macOS ? menu.setAsAppMenu() : menu.setAsWindowMenu())
  }
  onMount(async () => {
    await menu();
    let result = await getFolder();
    if(result.length === 0) {
      showSettingsDialog = true;
    }
  });
</script>

<Trace dialogopen={showTraceDialog} />
<SettingDialog dialogopen={showSettingsDialog} />
<BufferSizeDialog dialogopen={showBuffersizeDialog} />
<PatternManagerDialog dialogopen={showPatternManagerDialog} />
<PatternTesterDialog dialogopen={showPatternTesterDialog} />
<AboutDialog open={showAboutDialog}/>
