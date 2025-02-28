<script lang="ts">
    import * as Sheet from "$lib/components/ui/sheet";
    import { Checkbox } from "$lib/components/ui/checkbox";
    import { Label } from "$lib/components/ui/label";

    import { AlignJustify } from 'lucide-svelte';

    interface VisualItemProps {
        ispattern: boolean;
        isrwd: boolean;
        isqd: boolean;
        islatency: boolean;
        issizestats: boolean;
    }
    let { ispattern = $bindable(), isrwd = $bindable(), isqd = $bindable(), islatency = $bindable(), issizestats = $bindable() }:VisualItemProps = $props();       
</script>

<Sheet.Root>
    <Sheet.Trigger><AlignJustify /></Sheet.Trigger>
    <Sheet.Content side="left">
        <Sheet.Header>
            <Sheet.Title>Are you sure absolutely sure?</Sheet.Title>
            <Sheet.Description>
                This action cannot be undone. This will permanently delete your account
                and remove your data from our servers.
            </Sheet.Description>
        </Sheet.Header>
        <div class="flex items-center space-x-2 pt-4 pb-4">
            <Checkbox id="pattern" bind:checked={ispattern}/>
            <Label for="pattern" >Pattern</Label>
        </div>
        <div class="flex items-center space-x-2 pb-4">
            <Checkbox id="qd" bind:checked={isrwd}/>
            <Label for="qd">Read/Write/Discard Stats.</Label>
        </div>
        <div class="flex items-center space-x-2 pb-4">
            <Checkbox id="qd" bind:checked={isqd}/>
            <Label for="qd">QueueDepth</Label>
        </div>
        <div class="flex items-center space-x-2 pb-4">
            <Checkbox id="latency" bind:checked={islatency}/>
            <Label for="latency">Latency</Label>
        </div>
        <div class="flex items-center space-x-2 pb-4">
            <Checkbox id="sizestats" bind:checked={issizestats}/>
            <Label for="sizestats">Size</Label>
        </div>
    </Sheet.Content>
</Sheet.Root> 