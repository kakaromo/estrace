import Database from '@tauri-apps/plugin-sql';
import { setting } from "$stores/setting";
import type { TestInfo } from "$stores/trace";
import { platform, type } from '@tauri-apps/plugin-os';
import { join, homeDir, appLocalDataDir } from '@tauri-apps/api/path';

let db: Database = null;

async function getDbPath() {
    // OS에 따라 다른 DB 저장 위치 설정
    // Windows는 C:\, Linux와 macOS는 $HOME 디렉토리 사용
    const osType = await type();
    const osName = await platform();
    console.log(`Operating system: ${osType}, Platform: ${osName}`);
    
    let basePath;
    if (osType === 'windows') {
        // Windows: C:\ 디렉토리 사용
        return 'sqlite:\\test.db';
        
    } else {
        // Linux와 macOS: $HOME 디렉토리 사용
        return "sqlite://test.db";
    }    
}

async function open() {
    if(!db) {
        const dbPath = await getDbPath();
        console.log(`DB path: ${dbPath}`);
        // Check if the database file exists
        db = await Database.load(dbPath);    
    }
    return db;
}

export async function initial() {    
    await open();
    await db.execute('CREATE TABLE IF NOT EXISTS setting (id INTEGER PRIMARY KEY, value TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS app (id INTEGER PRIMARY KEY, filename TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS folder (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS file (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS log (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS testinfo (id INTEGER PRIMARY KEY, logtype TEXT, title TEXT, content TEXT, logfolder TEXT, logname TEXT, sourcelog_path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS testtype (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });    
    await db.execute('CREATE TABLE IF NOT EXISTS buffersize (id INTEGER PRIMARY KEY, buffersize INTEGER);').catch((e) => { console.log('error', e); });
    
    // Check if we need to add the sourcelog_path column to existing table
    try {
        const tableInfo = await db.select("PRAGMA table_info(testinfo)");
        const hasSourceLogPath = tableInfo.some(col => col.name === 'sourcelog_path');
        
        if (!hasSourceLogPath) {
            console.log('Adding sourcelog_path column to testinfo table');
            await db.execute('ALTER TABLE testinfo ADD COLUMN sourcelog_path TEXT;');
        }
    } catch(e) {
        console.error('Error checking or adding sourcelog_path column:', e);
    }

    // New table for trace patterns
    await db.execute(`
        CREATE TABLE IF NOT EXISTS trace_patterns (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            description TEXT,
            is_active BOOLEAN DEFAULT 0,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
    `).catch((e) => { console.log('error creating trace_patterns table', e); });
    
    // Insert default patterns if the table is empty
    const patterns = await db.select('SELECT COUNT(*) as count FROM trace_patterns');
    console.log('Patterns:', patterns);
    if (patterns[0].count === 0) {
        // Default UFS pattern - hwq_id에 음수도 허용하도록 수정
        await db.execute(`
            INSERT INTO trace_patterns (name, type, pattern, description, is_active)
            VALUES (?, ?, ?, ?, ?)
        `, [
            'Default UFS Pattern',
            'ufs',
            '^\\s*(?P<process>.*?)\\s+\\[(?P<cpu>[0-9]+)\\].*?(?P<time>[0-9]+\\.[0-9]+):\\s+ufshcd_command:\\s+(?P<command>send_req|complete_rsp):.*?tag:\\s*(?P<tag>\\d+).*?size:\\s*(?P<size>[-]?\\d+).*?LBA:\\s*(?P<lba>\\d+).*?opcode:\\s*(?P<opcode>0x[0-9a-f]+).*?group_id:\\s*0x(?P<group_id>[0-9a-f]+).*?hwq_id:\\s*(?P<hwq_id>[-]?\\d+)',
            'Default pattern for parsing UFS traces',
            1
        ]);
        
        // Default Block pattern
        await db.execute(`
            INSERT INTO trace_patterns (name, type, pattern, description, is_active)
            VALUES (?, ?, ?, ?, ?)
        `, [
            'Default Block Pattern',
            'block',
            '^\\s*(?P<process>.*?)\\s+\\[(?P<cpu>\\d+)\\]\\s+(?P<flags>.+?)\\s+(?P<time>[\\d\\.]+):\\s+(?P<action>\\S+):\\s+(?P<devmajor>\\d+),(?P<devminor>\\d+)\\s+(?P<io_type>[A-Z]+)(?:\\s+(?P<extra>\\d+))?\\s+\\(\\)\\s+(?P<sector>\\d+)\\s+\\+\\s+(?P<size>\\d+)(?:\\s+\\S+)?\\s+\\[(?P<comm>.*?)\\]$',
            'Default pattern for parsing block traces',
            1
        ]);
    }
    
    const result:number[] = await db.select('SELECT * FROM buffersize');
    if(result.length === 0) {
        await db.execute('INSERT OR REPLACE INTO buffersize (id, buffersize) VALUES (1, 500000);');
    }
}

export async function getFolder() {
    await open();
    const result:TestInfo[] = await db.select('SELECT path FROM folder WHERE id = 1');
    
    setting.update((s) => {
        if(result.length > 0) {
            s.logfolder = result[0].path;
        }
        return s;
    });
    
    return result;
}

export async function setFolder(key: string, value: string) {
    await open();
    await db.execute('INSERT OR REPLACE INTO folder (id, path) VALUES (1, ?)', [value]);
    setting.update((s) => {
        s[key] = value;
        return s;
    });
}

export async function getAllTestInfo() {
    await open();
    const result:TestInfo[] = await db.select('SELECT * FROM testinfo');
    return result;
}

export async function getTestInfo(id: number) {
    await open();
    const result:TestInfo[] = await db.select('SELECT * FROM testinfo WHERE id = ?', [id]);
    return result.length === 0? null : result[0];
}

export async function setTestInfo(logtype: string, title: string, content: string, logfolder: string, logname: string, sourcelogPath?: string) {
    await open();
    
    // Include sourcelog_path if provided
    if (sourcelogPath) {
        const result = await db.execute(
            'INSERT INTO testinfo (logtype, title, content, logfolder, logname, sourcelog_path) VALUES (?, ?, ?, ?, ?, ?)', 
            [ logtype, title, content, logfolder, logname, sourcelogPath ]
        );
        return result.lastInsertId;
    } else {
        const result = await db.execute(
            'INSERT INTO testinfo (logtype, title, content, logfolder, logname) VALUES (?, ?, ?, ?, ?)', 
            [ logtype, title, content, logfolder, logname ]
        );
        return result.lastInsertId;
    }
}

/**
 * 특정 ID의 테스트 정보를 삭제합니다.
 */
export async function deleteTestInfo(id: number) {
    await open();
    await db.execute('DELETE FROM testinfo WHERE id = ?', [id]);
}

/**
 * 여러 테스트 정보를 한 번에 삭제합니다.
 */
export async function deleteMultipleTestInfo(ids: number[]) {
    await open();
    
    // ID가 없으면 아무 작업도 하지 않음
    if (!ids || ids.length === 0) return;
    
    // IN 절에서 사용할 플레이스홀더 생성 (?, ?, ? 등)
    const placeholders = ids.map(() => '?').join(',');
    
    await db.execute(`DELETE FROM testinfo WHERE id IN (${placeholders})`, ids);
}

export async function getBufferSize(buffersize: number) {
    await open();
    const result:any[] = await db.select('SELECT * FROM buffersize  WHERE id = 1');
    return result[0].buffersize;
}

export async function setBufferSize(buffersize: number) {
    await open();
    await db.execute('INSERT OR REPLACE INTO buffersize (id, buffersize) VALUES (1, ?)', [ buffersize ]);
}

/**
 * Get all trace patterns from the database
 */
export async function getAllPatterns() {
    await open();
    const patterns = await db.select('SELECT * FROM trace_patterns ORDER BY type, name');
    return patterns;
}

/**
 * Get patterns of a specific type (ufs or block)
 */
export async function getPatternsByType(type: string) {
    await open();
    const patterns = await db.select('SELECT * FROM trace_patterns WHERE type = ? ORDER BY name', [type]);
    return patterns;
}

/**
 * Get active patterns (one for each type)
 */
export async function getActivePatterns() {
    await open();
    const patterns = await db.select('SELECT * FROM trace_patterns WHERE is_active = 1');
    return patterns.reduce((acc, pattern) => {
        acc[pattern.type] = pattern;
        return acc;
    }, {});
}

/**
 * Add a new pattern
 */
export async function addPattern(name: string, type: string, pattern: string, description: string) {
    await open();
    await db.execute(
        'INSERT INTO trace_patterns (name, type, pattern, description) VALUES (?, ?, ?, ?)',
        [name, type, pattern, description]
    );
}

/**
 * Update an existing pattern
 */
export async function updatePattern(id: number, name: string, pattern: string, description: string) {
    await open();
    await db.execute(
        'UPDATE trace_patterns SET name = ?, pattern = ?, description = ? WHERE id = ?',
        [name, pattern, description, id]
    );
}

/**
 * Set a pattern as active (and deactivate others of the same type)
 */
export async function setPatternActive(id: number) {
    await open();
    const pattern = await db.select('SELECT type FROM trace_patterns WHERE id = ?', [id]);
    if (pattern.length === 0) return;
    
    // Begin transaction
    await db.execute('BEGIN TRANSACTION');
    try {
        // Deactivate all patterns of the same type
        await db.execute('UPDATE trace_patterns SET is_active = 0 WHERE type = ?', [pattern[0].type]);
        
        // Activate the selected pattern
        await db.execute('UPDATE trace_patterns SET is_active = 1 WHERE id = ?', [id]);
        
        // Commit transaction
        await db.execute('COMMIT');
    } catch (error) {
        // Rollback on error
        await db.execute('ROLLBACK');
        throw error;
    }
}

/**
 * Delete a pattern
 */
export async function deletePattern(id: number) {
    await open();
    // Check if pattern is active
    const pattern = await db.select('SELECT is_active FROM trace_patterns WHERE id = ?', [id]);
    if (pattern.length === 0) return;
    
    // Don't allow deleting active patterns
    if (pattern[0].is_active) {
        throw new Error('Cannot delete an active pattern. Make another pattern active first.');
    }
    
    await db.execute('DELETE FROM trace_patterns WHERE id = ?', [id]);
}

/**
 * 특정 테스트 정보의 logname(파싱 결과 파일 경로)를 업데이트합니다.
 * 재파싱 후 결과 파일 경로가 변경된 경우 사용됩니다.
 */
export async function updateTestInfoLogName(id: number, logname: string) {
    await open();
    await db.execute('UPDATE testinfo SET logname = ? WHERE id = ?', [logname, id]);
}

/**
 * 재파싱 결과를 데이터베이스에 업데이트합니다.
 */
export async function updateReparseResult(id: number, logname: string) {
    await open();
    
    // 기존 트레이스 정보 확인
    const result: TestInfo[] = await db.select('SELECT * FROM testinfo WHERE id = ?', [id]);
    if (result.length === 0) {
        throw new Error(`테스트 정보를 찾을 수 없습니다 (ID: ${id})`);
    }
    
    // logname 업데이트
    await db.execute('UPDATE testinfo SET logname = ? WHERE id = ?', [logname, id]);
    
    return result[0];
}