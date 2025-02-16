<script lang="ts">    
  // import * as Menubar from "$lib/components/ui/menubar";
  import { onMount } from 'svelte';
  
  import { Menu, Submenu, MenuItem} from '@tauri-apps/api/menu'
  import { open } from '@tauri-apps/plugin-dialog';
  
  import * as Dialog from "$lib/components/ui/dialog";  
  // import SettingDialog from './menu/setting.svelte';
  import { AboutDialog, SettingDialog } from './menu';
  import Trace from './trace.svelte'; // 새로 추가됨
  import { getFolder } from "../api/db.js";
  import { traceFile, Status, traceStatusStore } from '../stores/file.js';

  import { clear } from 'idb-keyval'

  const macOS = navigator.userAgent.includes('Macintosh')
  let showAboutDialog = false;
  let showSettingsDialog = false;
  let showTraceDialog = false; // 새로 추가됨

  async function handleFileOpen() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'All Files', extensions: ['*'] }
        ]
        // 필터 조건을 추가할 수 있습니다.
        // filters: [{
        //   name: 'All Files',
        //   extensions: ['*']
        // }]
      });
      console.log('선택된 파일:', selected);
      traceFile.set(selected);
      traceStatusStore.set(Status.Opened);
    } catch (error) {
      console.error('파일 열기 실패:', error);
    }
  }

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
            // handleFileOpen(); 기존 호출 대신 다이얼로그 표시
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
          text: "setting",
          action: () => {
            showSettingsDialog = false;
            showSettingsDialog = true;
            console.log('App setting');
          }
        },
        {
          text: "session clear",
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

<SettingDialog dialogopen={showSettingsDialog} />
<AboutDialog open={showAboutDialog}/>
<Trace dialogopen={showTraceDialog} />

<!-- shadcn Dialog를 이용해 Trace 컴포넌트를 다이얼로그로 띄움 -->
<!-- <Dialog.Root open={showTraceDialog} onOpenChange={(val) => showTraceDialog = val}>
  <Dialog.DialogContent>
    <Trace />
    <Dialog.DialogClose>
      <button class="btn">닫기</button>
    </Dialog.DialogClose>
  </Dialog.DialogContent>
</Dialog.Root> -->

<!-- <Dialog.Root bind:open={showSettingsDialog}>
  <Dialog.DialogContent>
    <Dialog.DialogHeader>
      <Dialog.DialogTitle>Settings</Dialog.DialogTitle>
      <Dialog.DialogDescription>
        설정 내용을 구성해 주세요.
      </Dialog.DialogDescription>
    </Dialog.DialogHeader>
    <div class="mt-4 flex justify-end">
      <Dialog.DialogClose>
        <button class="btn">닫기</button>
      </Dialog.DialogClose>
    </div>
  </Dialog.DialogContent>
</Dialog.Root> -->


<!-- <Menubar.Root>
  <Menubar.Menu>
    <Menubar.Trigger>File</Menubar.Trigger>
    <Menubar.Content>
      <Menubar.Item>
        New Tab
        <Menubar.Shortcut>⌘T</Menubar.Shortcut>
      </Menubar.Item>
      <Menubar.Item>New Window</Menubar.Item>
      <Menubar.Separator />
      <Menubar.Item>Share</Menubar.Item>
      <Menubar.Separator />
      <Menubar.Item>Print</Menubar.Item>
    </Menubar.Content>
  </Menubar.Menu>
</Menubar.Root> -->