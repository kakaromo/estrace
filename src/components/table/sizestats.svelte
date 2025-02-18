<script lang='ts'>    
    import { Grid } from "wx-svelte-grid";
    import { Willow } from "wx-svelte-grid";

    let { sizecounts } = $props();

    let columns = [];
    let size_type_key = $state<String[]>([]);
    let size_count_key = $state<Record<string, number[]>>({});
    sizeTypeKey();
    sizeCountKey();
    function sizeTypeKey() {
        Object.keys(sizecounts).forEach((key) => {
            size_type_key.push(key);
        });
    }
    function sizeCountKey() {
        for (let i = 0; i < size_type_key.length; i++) {
            const key = size_type_key[i];
            size_count_key[key] = [];
            Object.keys(sizecounts[key]).forEach((subKey) => {          
                let column = {
                    id: subKey,
                    header: subKey,                                        
                    width: 100
                };      
                size_count_key[key].push(column);
            });
        }
    }

</script>

<Willow></Willow>