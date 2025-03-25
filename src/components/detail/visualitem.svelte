<script lang="ts">
    import * as Sheet from "$lib/components/ui/sheet";
    import { Checkbox } from "$lib/components/ui/checkbox";
    import { Label } from "$lib/components/ui/label";

    import { AlignJustify } from 'lucide-svelte';

    interface VisualItemProps {
        ispattern: boolean;
        isrwd: boolean;
        isqd: boolean;
        iscpu: boolean;
        islatency: boolean;
        issizestats: boolean;
    }
    let { ispattern = $bindable(), isrwd = $bindable(), isqd = $bindable(), iscpu = $bindable(), islatency = $bindable(), issizestats = $bindable() }:VisualItemProps = $props();       
</script>

<Sheet.Root>
    <Sheet.Trigger><AlignJustify /></Sheet.Trigger>
    <Sheet.Content side="left">
        <Sheet.Header>
            <Sheet.Title>Trace Information</Sheet.Title>
            <Sheet.Description>                
                <p>확인하고자 하는 Trace의 정보를 선택하세요.</p>
                <p>checkbox를 선택하면 해당 정보를 확인할 수 있습니다.</p>
                <p>해제하면 disable됩니다.</p>
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
            <Checkbox id="cpu" bind:checked={iscpu}/>
            <Label for="cpu">CPU</Label>
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