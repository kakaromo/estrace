// src/api/pattern.ts

import { invoke } from "@tauri-apps/api/core";
import { getAllPatterns, getPatternsByType, addPattern as dbAddPattern, updatePattern, setPatternActive, deletePattern } from './db';

export interface Pattern {
    id: number;
    name: string;
    type: string;  // 'ufs' or 'block'
    pattern: string;
    description: string;
    is_active: boolean;
    created_at: string;
}

/**
 * Get all patterns from the database
 */
export async function getPatterns(): Promise<Pattern[]> {
    try {
        return await getAllPatterns();
    } catch (error) {
        console.error('Error getting patterns from DB:', error);
        throw error;
    }
}

/**
 * Get patterns of a specific type from the database
 */
export async function getPatternsByTypeFromDb(type: string): Promise<Pattern[]> {
    try {
        return await getPatternsByType(type);
    } catch (error) {
        console.error(`Error getting ${type} patterns from DB:`, error);
        throw error;
    }
}

/**
 * Get all patterns from the Rust backend
 */
export async function getPatternsFromRust(type?: string): Promise<Pattern[]> {
    try {
        const patternsJson = await invoke<string>('get_patterns', { patternType: type });
        return JSON.parse(patternsJson);
    } catch (error) {
        console.error('Error getting patterns from Rust:', error);
        throw error;
    }
}

/**
 * Get active patterns from the Rust backend
 */
export async function getActivePatterns(): Promise<Record<string, Pattern>> {
    try {
        const patternsJson = await invoke<string>('get_active_patterns');
        return JSON.parse(patternsJson);
    } catch (error) {
        console.error('Error getting active patterns:', error);
        throw error;
    }
}

/**
 * Add a new pattern to both DB and Rust backend
 */
export async function addPattern(
    name: string,
    type: string,
    pattern: string,
    description: string
): Promise<void> {
    try {
        // First add to DB
        await dbAddPattern(name, type, pattern, description);
        
        // Then add to Rust backend
        await invoke('add_pattern', {
            name,
            patternType: type,
            pattern
        });
    } catch (error) {
        console.error('Error adding pattern:', error);
        throw error;
    }
}

/**
 * Update an existing pattern in both DB and Rust backend
 */
export async function updateExistingPattern(
    id: number,
    name: string,
    pattern: string,
    description: string
): Promise<void> {
    try {
        const patterns = await getPatterns();
        const existingPattern = patterns.find(p => p.id === id);
        
        if (!existingPattern) {
            throw new Error(`Pattern with ID ${id} not found`);
        }
        
        // Update in DB
        await updatePattern(id, name, pattern, description);
        
        // Delete old pattern from Rust backend
        await invoke('delete_pattern', {
            name: existingPattern.name,
            patternType: existingPattern.type
        });
        
        // Add updated pattern to Rust backend
        await invoke('add_pattern', {
            name,
            patternType: existingPattern.type,
            pattern
        });
        
        // If it was active, set it active again
        if (existingPattern.is_active) {
            await setActivePattern(id);
        }
    } catch (error) {
        console.error('Error updating pattern:', error);
        throw error;
    }
}

/**
 * Set a pattern as active in both DB and Rust backend
 */
export async function setActivePattern(id: number): Promise<void> {
    try {
        // First set active in DB
        await setPatternActive(id);
        
        // Get updated pattern info
        const patterns = await getPatterns();
        const activePattern = patterns.find(p => p.id === id);
        
        if (!activePattern) {
            throw new Error(`Pattern with ID ${id} not found`);
        }
        
        // Set active in Rust backend
        await invoke('set_active_pattern', {
            name: activePattern.name,
            patternType: activePattern.type
        });
    } catch (error) {
        console.error('Error setting active pattern:', error);
        throw error;
    }
}

/**
 * Delete a pattern from both DB and Rust backend
 */
export async function deletePatternById(id: number): Promise<void> {
    try {
        // Get pattern info before deletion
        const patterns = await getPatterns();
        const patternToDelete = patterns.find(p => p.id === id);
        
        if (!patternToDelete) {
            throw new Error(`Pattern with ID ${id} not found`);
        }
        
        // Check if it's active
        if (patternToDelete.is_active) {
            throw new Error('Cannot delete an active pattern. Make another pattern active first.');
        }
        
        // Delete from DB
        await deletePattern(id);
        
        // Delete from Rust backend
        await invoke('delete_pattern', {
            name: patternToDelete.name,
            patternType: patternToDelete.type
        });
    } catch (error) {
        console.error('Error deleting pattern:', error);
        throw error;
    }
}

/**
 * Sync patterns between DB and Rust backend
 * This is useful after starting the application
 */
export async function syncPatterns(): Promise<void> {
    try {
        // Get all patterns from DB
        const dbPatterns = await getPatterns();
        
        // For each pattern in DB, add to Rust backend
        for (const pattern of dbPatterns) {
            try {
                await invoke('add_pattern', {
                    name: pattern.name,
                    patternType: pattern.type,
                    pattern: pattern.pattern
                });
                
                if (pattern.is_active) {
                    await invoke('set_active_pattern', {
                        name: pattern.name,
                        patternType: pattern.type
                    });
                }
            } catch (error) {
                console.warn(`Failed to sync pattern ${pattern.name}:`, error);
            }
        }
    } catch (error) {
        console.error('Error syncing patterns:', error);
        throw error;
    }
}