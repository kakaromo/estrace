<script lang="ts">

    import { Button } from "$lib/components/ui/button";
    import * as Dialog from "$lib/components/ui/dialog";

    let { open } = $props();
    const closeDialog = () => {
        open = false;
    };
</script>

<Dialog.Root bind:open={open}>
    <Dialog.Content>
      <Dialog.Header>
        <Dialog.Title>About</Dialog.Title>
        <Dialog.Description>
          This is an example Tauri application.
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button on:click={() => open = false}>Close</Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>